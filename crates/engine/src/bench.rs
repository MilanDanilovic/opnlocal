//! A short, fixed benchmark: read a ~300-word passage, write a short summary.
//!
//! Everything reported is measured on this device with wall-clock time. Words per second come
//! from counting the words the model actually wrote, not from a token→word conversion.

use crate::chat::{self, ChatMsg, Piece, ReplyParser, TemplateOptions};
use crate::fit::Placement;
use crate::llm::{GenerateRequest, LlmError, ModelText, Runtime, SamplingParams, StopReason};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use ts_rs::TS;

/// Longest the generation part may run; the whole benchmark stays well under a minute.
pub const TIME_LIMIT: Duration = Duration::from_secs(20);
pub const MAX_TOKENS: u32 = 160;

/// Average adult silent reading speed is about 238 words per minute (Brysbaert 2019,
/// doi:10.1016/j.jml.2019.104047). Used only to put a measured number in context.
pub const READING_WORDS_PER_SECOND: f64 = 238.0 / 60.0;

const PASSAGE: &str = "The town library had stood on the same corner for more than a hundred years. \
It began as a single room above a bakery, where a retired teacher lent out her own books to anyone \
who promised to bring them back. Over time the neighbors donated shelves, then chairs, then a small \
stove for the winter months. When the bakery closed, the town bought the whole building and turned \
the ground floor into a reading room with tall windows facing the square. Children came after school \
to do their homework, and older residents came in the mornings to read the newspapers. In the 1970s \
the library added a record collection, and in the 1990s a row of computers that were always busy. \
Today the building is quieter, but it is far from empty. People come to print documents, to ask for \
help filling in forms, to join a knitting circle on Thursdays, or simply to sit somewhere warm and \
calm. The librarians say their job has changed more in the last twenty years than in the eighty before \
that. They spend less time stamping books and more time teaching people how to use their phones, how \
to spot a scam email, and how to find reliable information. Many of them believe the library matters \
more now than it did when it was the only place in town with books, because it is one of the few places \
left where anyone can walk in, stay as long as they like, and not be asked to buy anything.";

const QUESTION: &str = "Summarize the passage above in one short paragraph.";

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct BenchmarkResult {
    pub model_id: String,
    pub app_version: String,
    #[ts(type = "number")]
    pub measured_at: u64,
    /// What did the work, in plain words ("AMD Radeon RX 7800 XT" or the processor name).
    pub device: String,
    pub placement: Placement,
    pub gpu_layers: i32,
    pub context: u32,
    #[ts(type = "number")]
    pub load_ms: u64,
    pub prompt_tokens: u32,
    pub prompt_tokens_per_second: Option<f64>,
    pub first_token_ms: f64,
    pub generated_tokens: u32,
    pub generation_tokens_per_second: Option<f64>,
    pub words: u32,
    pub words_per_second: Option<f64>,
    /// Measured speed relative to reading speed (see [`verdict`]).
    pub verdict: Option<SpeedVerdict>,
    /// System memory used by the app at the end of the test (includes the model if it's in RAM).
    #[ts(type = "number | null")]
    pub ram_in_use_bytes: Option<u64>,
    /// Graphics memory taken by the model, from the driver's free-memory report.
    #[ts(type = "number | null")]
    pub gpu_memory_used_bytes: Option<u64>,
    /// True if the generation hit the time limit before finishing.
    pub hit_time_limit: bool,
}

/// Plain-language verdict for a measured speed, relative to reading speed.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SpeedVerdict {
    /// More than twice reading speed.
    FasterThanReading,
    /// Roughly reading speed.
    AboutReadingSpeed,
    /// Noticeably slower than reading.
    SlowerThanReading,
    /// Replies will take a long time.
    Slow,
}

pub fn verdict(words_per_second: f64) -> SpeedVerdict {
    let r = READING_WORDS_PER_SECOND;
    if words_per_second >= 2.0 * r {
        SpeedVerdict::FasterThanReading
    } else if words_per_second >= 0.8 * r {
        SpeedVerdict::AboutReadingSpeed
    } else if words_per_second >= 0.35 * r {
        SpeedVerdict::SlowerThanReading
    } else {
        SpeedVerdict::Slow
    }
}

/// Raw measurement of the generation part; the caller adds load time and device facts.
#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    pub prompt_tokens: u32,
    pub prompt_tokens_per_second: Option<f64>,
    pub first_token_ms: f64,
    pub generated_tokens: u32,
    pub generation_tokens_per_second: Option<f64>,
    pub words: u32,
    pub words_per_second: Option<f64>,
    pub hit_time_limit: bool,
}

/// Runs the fixed prompt on the loaded model. Thinking is off; for models that always reason,
/// every word they write (reasoning included) counts, since all of it takes time to produce.
pub fn measure(
    rt: &Runtime,
    text: &ModelText,
    chat_profile: &crate::catalog::ChatProfile,
    fallback_template: &str,
) -> Result<Measurement, LlmError> {
    let template = text.chat_template.as_deref().unwrap_or(fallback_template);
    let prompt = chat::render(
        template,
        &[ChatMsg::new("user", format!("{PASSAGE}\n\n{QUESTION}"))],
        &TemplateOptions {
            thinking: &chat_profile.thinking,
            think_harder: false,
            bos_token: &text.bos,
            eos_token: &text.eos,
            vars: &chat_profile.template_vars,
        },
    )
    .map_err(|e| LlmError::Runtime {
        message: e.to_string(),
    })?;

    let stop = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let output = Arc::new(std::sync::Mutex::new(String::new()));
    let (out2, stop2, timed2) = (output.clone(), stop.clone(), timed_out.clone());
    let mut first_at: Option<std::time::Instant> = None;
    let s = &chat_profile.sampling;
    let stats = rt.generate(
        GenerateRequest {
            prompt: prompt.clone(),
            max_tokens: MAX_TOKENS,
            sampling: SamplingParams {
                temperature: s.temperature,
                top_p: s.top_p,
                top_k: s.top_k,
                min_p: s.min_p,
                // Fixed seed: the same model on the same device writes the same text.
                seed: 42,
            },
            stop,
        },
        move |piece| {
            let now = std::time::Instant::now();
            let first = *first_at.get_or_insert(now);
            if now.duration_since(first) > TIME_LIMIT {
                timed2.store(true, Ordering::Relaxed);
                stop2.store(true, Ordering::Relaxed);
            }
            out2.lock().unwrap().push_str(piece);
        },
    )?;

    let raw = output.lock().unwrap().clone();
    let mut parser = ReplyParser::new(chat_profile.reasoning, &prompt);
    let mut visible = String::new();
    for p in parser.push(&raw).into_iter().chain(parser.finish()) {
        let (Piece::Thinking(t) | Piece::Answer(t)) = p;
        visible.push_str(&t);
        visible.push(' ');
    }
    let words = chat::count_words(&visible);
    let secs = stats.generation_ms / 1000.0;
    Ok(Measurement {
        prompt_tokens: stats.prompt_tokens,
        prompt_tokens_per_second: stats.prompt_tokens_per_second(),
        first_token_ms: stats.first_token_ms,
        generated_tokens: stats.generated_tokens,
        generation_tokens_per_second: stats.generation_tokens_per_second(),
        words,
        words_per_second: (secs > 0.0 && words > 1).then(|| words as f64 / secs),
        hit_time_limit: timed_out.load(Ordering::Relaxed)
            || (stats.stop_reason == StopReason::Stopped),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_relative_to_reading_speed() {
        assert_eq!(verdict(12.0), SpeedVerdict::FasterThanReading);
        assert_eq!(verdict(4.0), SpeedVerdict::AboutReadingSpeed);
        assert_eq!(verdict(2.0), SpeedVerdict::SlowerThanReading);
        assert_eq!(verdict(0.5), SpeedVerdict::Slow);
    }

    #[test]
    fn passage_is_a_realistic_length() {
        let words = chat::count_words(PASSAGE);
        assert!((250..=320).contains(&words), "{words} words");
    }

    #[test]
    fn measures_a_real_model() {
        let _serial = crate::llm::tests::serial();
        let rt = Runtime::start(None);
        let Some(text) = crate::llm::tests::load_test_model(&rt, 1024) else {
            eprintln!("skipped: test model missing");
            return;
        };
        let profile = crate::catalog::ChatProfile {
            reasoning: crate::catalog::ReasoningFormat::None,
            thinking: crate::catalog::ThinkingControl::Unsupported,
            sampling: crate::catalog::Sampling {
                temperature: 0.0,
                top_p: 1.0,
                top_k: 1,
                min_p: 0.0,
            },
            thinking_sampling: None,
            template_vars: Default::default(),
        };
        // The tiny story model has no chat template; a plain one is enough to measure.
        let m = measure(&rt, &text, &profile, "{% for m in messages %}{{ m.content }}{% endfor %}")
            .unwrap();
        assert!(m.prompt_tokens > 250);
        assert!(m.generated_tokens > 1);
        assert!(m.generation_tokens_per_second.unwrap() > 0.0);
        assert!(m.words > 0);
        assert!(m.first_token_ms > 0.0);
    }
}
