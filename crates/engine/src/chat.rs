//! Turning a conversation into a prompt, and a model's output back into thinking + answer.
//!
//! Prompts are rendered with the model's own chat template (Jinja, stored in the GGUF file)
//! using minijinja configured like Hugging Face transformers (trim/lstrip blocks, Python string
//! methods). Output is split by [`ReplyParser`] according to the model's [`ReasoningFormat`].

use crate::catalog::{ReasoningFormat, ThinkingControl};
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

impl ChatMsg {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        ChatMsg {
            role: role.into(),
            content: content.into(),
        }
    }
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum ChatError {
    #[error("the model's chat template could not be used: {0}")]
    Template(String),
    /// Even the latest message alone is longer than the model can read at once.
    #[error("message too long: {prompt_tokens} tokens, room for {room_tokens}")]
    TooLong { prompt_tokens: u32, room_tokens: u32 },
}

pub struct TemplateOptions<'a> {
    pub thinking: &'a ThinkingControl,
    pub think_harder: bool,
    pub bos_token: &'a str,
    pub eos_token: &'a str,
    /// Model-specific variables from the catalog.
    pub vars: &'a std::collections::BTreeMap<String, String>,
}

/// Renders messages with a Hugging Face–style chat template, ending with the assistant prompt.
pub fn render(
    template: &str,
    messages: &[ChatMsg],
    opts: &TemplateOptions,
) -> Result<String, ChatError> {
    let mut env = minijinja::Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    env.add_function("raise_exception", |msg: String| -> Result<String, minijinja::Error> {
        Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            msg,
        ))
    });
    env.add_function("strftime_now", |fmt: String| strftime_now(&fmt));
    env.add_template("chat", template)
        .map_err(|e| ChatError::Template(e.to_string()))?;

    let mut ctx = std::collections::BTreeMap::<&str, minijinja::Value>::new();
    ctx.insert("messages", minijinja::Value::from_serialize(messages));
    ctx.insert("add_generation_prompt", true.into());
    ctx.insert("bos_token", opts.bos_token.into());
    ctx.insert("eos_token", opts.eos_token.into());
    for (k, v) in opts.vars {
        ctx.insert(k.as_str(), v.as_str().into());
    }
    match opts.thinking {
        ThinkingControl::Unsupported => {}
        ThinkingControl::TemplateFlag { variable } => {
            ctx.insert(variable.as_str(), opts.think_harder.into());
        }
        ThinkingControl::AlwaysOn {
            variable,
            normal,
            harder,
        } => {
            let effort = if opts.think_harder { harder } else { normal };
            ctx.insert(variable.as_str(), effort.as_str().into());
        }
    }
    env.get_template("chat")
        .and_then(|t| t.render(minijinja::Value::from_serialize(&ctx)))
        .map_err(|e| ChatError::Template(e.to_string()))
}

/// Minimal strftime for templates that print today's date (e.g. "%d %b %Y", "%Y-%m-%d").
fn strftime_now(fmt: &str) -> String {
    let secs = crate::store::now() as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const FULL: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut out = String::new();
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('Y') => out.push_str(&y.to_string()),
            Some('m') => out.push_str(&format!("{m:02}")),
            Some('d') => out.push_str(&format!("{d:02}")),
            Some('b') => out.push_str(MONTHS[(m - 1) as usize]),
            Some('B') => out.push_str(FULL[(m - 1) as usize]),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// How a prompt was fitted into the context window.
#[derive(Debug, PartialEq)]
pub struct PromptPlan {
    pub prompt: String,
    pub prompt_tokens: u32,
    /// Oldest messages left out so the conversation fits.
    pub dropped_messages: usize,
}

/// Renders `system + history`, dropping the oldest history messages until the prompt plus
/// `reserve` reply tokens fit in `n_ctx`. The newest message is never dropped.
pub fn plan_prompt(
    render_fn: impl Fn(&[ChatMsg]) -> Result<String, ChatError>,
    count_tokens: impl Fn(&str) -> u32,
    system: Option<&str>,
    history: &[ChatMsg],
    n_ctx: u32,
    reserve: u32,
) -> Result<PromptPlan, ChatError> {
    let room = n_ctx.saturating_sub(reserve);
    let mut start = 0;
    loop {
        let mut msgs = Vec::with_capacity(history.len() + 1);
        if let Some(s) = system.filter(|s| !s.trim().is_empty()) {
            msgs.push(ChatMsg::new("system", s));
        }
        msgs.extend_from_slice(&history[start..]);
        let prompt = render_fn(&msgs)?;
        let tokens = count_tokens(&prompt);
        if tokens <= room {
            return Ok(PromptPlan {
                prompt,
                prompt_tokens: tokens,
                dropped_messages: start,
            });
        }
        if start + 1 >= history.len() {
            return Err(ChatError::TooLong {
                prompt_tokens: tokens,
                room_tokens: room,
            });
        }
        // Drop a whole exchange where possible, so the history still starts with the user.
        start += 1;
        while start + 1 < history.len() && history[start].role != "user" {
            start += 1;
        }
    }
}

/// The user's message with attached documents placed before it.
pub fn user_content(text: &str, attachments: &[(String, String)]) -> String {
    let mut out = String::new();
    for (name, body) in attachments {
        out.push_str(&format!(
            "[Attached document: {name}]\n{}\n[End of document: {name}]\n\n",
            body.trim()
        ));
    }
    out.push_str(text);
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Thinking(String),
    Answer(String),
}

/// Splits streamed model output into thinking and answer text. Tags may arrive split across
/// chunks, so text that could be the start of a tag is held back until it's decided.
pub struct ReplyParser {
    format: ReasoningFormat,
    thinking: bool,
    pending: String,
    /// Harmony: we're reading a header (role/channel name), not content.
    in_header: bool,
    answer_started: bool,
}

impl ReplyParser {
    /// `prompt` is the rendered prompt: some templates open the thinking block themselves.
    pub fn new(format: ReasoningFormat, prompt: &str) -> ReplyParser {
        let tail = prompt.trim_end();
        let thinking = match format {
            ReasoningFormat::ThinkTags => tail.ends_with("<think>"),
            ReasoningFormat::GemmaChannel => tail.ends_with(GEMMA_OPEN),
            _ => false,
        };
        ReplyParser {
            format,
            thinking,
            pending: String::new(),
            in_header: false,
            answer_started: false,
        }
    }

    pub fn push(&mut self, text: &str) -> Vec<Piece> {
        self.pending.push_str(text);
        let mut out = Vec::new();
        match self.format {
            ReasoningFormat::None => {
                let t = std::mem::take(&mut self.pending);
                self.emit_answer(t, &mut out);
            }
            ReasoningFormat::ThinkTags => self.tags("<think>", "</think>", &mut out),
            ReasoningFormat::GemmaChannel => self.tags(GEMMA_OPEN, GEMMA_CLOSE, &mut out),
            ReasoningFormat::Harmony => self.harmony(&mut out),
        }
        out
    }

    /// Flushes whatever is held back at the end of the reply.
    pub fn finish(&mut self) -> Vec<Piece> {
        let rest = std::mem::take(&mut self.pending);
        let mut out = Vec::new();
        if self.format == ReasoningFormat::Harmony && self.in_header {
            return out;
        }
        if self.thinking {
            push_piece(&mut out, Piece::Thinking(rest));
        } else {
            self.emit_answer(rest, &mut out);
        }
        out
    }

    fn emit_answer(&mut self, mut text: String, out: &mut Vec<Piece>) {
        if !self.answer_started {
            // Models often put blank lines between thinking and answer.
            text = text.trim_start().to_string();
            if text.is_empty() {
                return;
            }
            self.answer_started = true;
        }
        push_piece(out, Piece::Answer(text));
    }

    fn tags(&mut self, open: &str, close: &str, out: &mut Vec<Piece>) {
        loop {
            let tag = if self.thinking { close } else { open };
            if let Some(i) = self.pending.find(tag) {
                let before: String = self.pending[..i].to_string();
                self.pending.drain(..i + tag.len());
                if self.thinking {
                    push_piece(out, Piece::Thinking(before));
                } else {
                    self.emit_answer(before, out);
                }
                self.thinking = !self.thinking;
                continue;
            }
            // Hold back a suffix that could be the beginning of the tag.
            let keep = partial_suffix(&self.pending, tag);
            let emit: String = self.pending[..self.pending.len() - keep].to_string();
            self.pending.drain(..self.pending.len() - keep);
            if self.thinking {
                push_piece(out, Piece::Thinking(emit));
            } else {
                self.emit_answer(emit, out);
            }
            return;
        }
    }

    /// Harmony: `<|channel|>analysis<|message|>…<|end|><|start|>assistant<|channel|>final<|message|>…`
    fn harmony(&mut self, out: &mut Vec<Piece>) {
        loop {
            let Some(i) = self.pending.find("<|") else {
                let t = std::mem::take(&mut self.pending);
                self.harmony_text(t, out);
                return;
            };
            let Some(j) = self.pending[i..].find("|>") else {
                // Possibly an incomplete special token: emit what's before it, keep the rest.
                let before: String = self.pending[..i].to_string();
                self.pending.drain(..i);
                self.harmony_text(before, out);
                return;
            };
            let before: String = self.pending[..i].to_string();
            let token: String = self.pending[i..i + j + 2].to_string();
            self.pending.drain(..i + j + 2);
            if self.in_header {
                // Header text such as "analysis" or "final" before <|message|>.
                if before.contains("final") {
                    self.thinking = false;
                } else if before.contains("analysis") || before.contains("commentary") {
                    self.thinking = true;
                }
            } else {
                self.harmony_text(before, out);
            }
            match token.as_str() {
                "<|channel|>" | "<|start|>" => self.in_header = true,
                "<|message|>" => self.in_header = false,
                _ => {} // <|end|>, <|return|>, <|call|>: separators
            }
        }
    }

    fn harmony_text(&mut self, text: String, out: &mut Vec<Piece>) {
        if self.in_header {
            // Keep header text until its <|message|>; it decides the channel.
            self.pending.insert_str(0, &text);
            return;
        }
        if self.thinking {
            push_piece(out, Piece::Thinking(text));
        } else {
            self.emit_answer(text, out);
        }
    }
}

const GEMMA_OPEN: &str = "<|channel>thought";
const GEMMA_CLOSE: &str = "<channel|>";

fn push_piece(out: &mut Vec<Piece>, p: Piece) {
    match &p {
        Piece::Thinking(t) | Piece::Answer(t) if t.is_empty() => {}
        _ => out.push(p),
    }
}

/// Length of the longest suffix of `s` that is a proper prefix of `tag`.
fn partial_suffix(s: &str, tag: &str) -> usize {
    (1..tag.len().min(s.len() + 1))
        .rev()
        .find(|&n| s.is_char_boundary(s.len() - n) && tag.starts_with(&s[s.len() - n..]))
        .unwrap_or(0)
}

/// Counts words the way a reader would: whitespace-separated runs containing a letter or digit.
pub fn count_words(text: &str) -> u32 {
    text.split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHATML: &str = "{% for m in messages %}<|im_start|>{{ m.role }}\n{{ m.content }}<|im_end|>\n{% endfor %}{% if add_generation_prompt %}<|im_start|>assistant\n{% if enable_thinking is defined and not enable_thinking %}<think>\n\n</think>\n\n{% endif %}{% endif %}";

    static NO_VARS: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();

    fn opts(thinking: &ThinkingControl, harder: bool) -> TemplateOptions<'_> {
        TemplateOptions {
            thinking,
            think_harder: harder,
            bos_token: "",
            eos_token: "<|im_end|>",
            vars: &NO_VARS,
        }
    }

    fn collect(p: &mut ReplyParser, chunks: &[&str]) -> (String, String) {
        let mut pieces = Vec::new();
        for c in chunks {
            pieces.extend(p.push(c));
        }
        pieces.extend(p.finish());
        let mut thinking = String::new();
        let mut answer = String::new();
        for piece in pieces {
            match piece {
                Piece::Thinking(t) => thinking.push_str(&t),
                Piece::Answer(a) => answer.push_str(&a),
            }
        }
        (thinking, answer)
    }

    #[test]
    fn renders_chatml_with_thinking_off() {
        let flag = ThinkingControl::TemplateFlag {
            variable: "enable_thinking".into(),
        };
        let p = render(CHATML, &[ChatMsg::new("user", "Hi")], &opts(&flag, false)).unwrap();
        assert_eq!(
            p,
            "<|im_start|>user\nHi<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
        );
        let p = render(CHATML, &[ChatMsg::new("user", "Hi")], &opts(&flag, true)).unwrap();
        assert!(p.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn templates_can_use_python_string_methods_and_raise() {
        let t = "{% if messages[0].content.startswith('x') %}{{ raise_exception('no x') }}{% endif %}{{ messages[0].content.upper() }}";
        let none = ThinkingControl::Unsupported;
        assert_eq!(
            render(t, &[ChatMsg::new("user", "hey")], &opts(&none, false)).unwrap(),
            "HEY"
        );
        assert!(matches!(
            render(t, &[ChatMsg::new("user", "xx")], &opts(&none, false)),
            Err(ChatError::Template(_))
        ));
    }

    #[test]
    fn effort_variable_for_always_reasoning_models() {
        let t = "effort={{ reasoning_effort }}";
        let c = ThinkingControl::AlwaysOn {
            variable: "reasoning_effort".into(),
            normal: "low".into(),
            harder: "high".into(),
        };
        assert_eq!(render(t, &[], &opts(&c, false)).unwrap(), "effort=low");
        assert_eq!(render(t, &[], &opts(&c, true)).unwrap(), "effort=high");
    }

    #[test]
    fn strftime_formats_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_720), (2026, 9, 24));
        let s = strftime_now("%d %b %Y");
        assert_eq!(s.len(), 11);
    }

    #[test]
    fn think_tags_split_even_when_tags_are_cut_across_chunks() {
        let mut p = ReplyParser::new(ReasoningFormat::ThinkTags, "<|im_start|>assistant\n");
        let (t, a) = collect(&mut p, &["<th", "ink>Let me", " think.</thi", "nk>\n\nThe answer", " is 4."]);
        assert_eq!(t, "Let me think.");
        assert_eq!(a, "The answer is 4.");
    }

    #[test]
    fn think_block_opened_by_the_template() {
        let mut p = ReplyParser::new(ReasoningFormat::ThinkTags, "assistant\n<think>\n");
        let (t, a) = collect(&mut p, &["hmm", "</think>", "Done."]);
        assert_eq!((t.as_str(), a.as_str()), ("hmm", "Done."));
    }

    #[test]
    fn empty_think_block_from_thinking_off_is_invisible() {
        let prompt = "assistant\n<think>\n\n</think>\n\n";
        let mut p = ReplyParser::new(ReasoningFormat::ThinkTags, prompt);
        let (t, a) = collect(&mut p, &["Hello", " there"]);
        assert_eq!((t.as_str(), a.as_str()), ("", "Hello there"));
    }

    #[test]
    fn less_than_signs_in_answers_are_not_eaten() {
        let mut p = ReplyParser::new(ReasoningFormat::ThinkTags, "");
        let (_, a) = collect(&mut p, &["if a <", " b then <t", "ag> ok"]);
        assert_eq!(a, "if a < b then <tag> ok");
    }

    #[test]
    fn harmony_channels() {
        let mut p = ReplyParser::new(ReasoningFormat::Harmony, "<|start|>assistant");
        let (t, a) = collect(
            &mut p,
            &[
                "<|channel|>analysis<|mess",
                "age|>User wants 2+2.<|end|><|start|>assistant<|channel|>final<|message|>It's ",
                "4.<|return|>",
            ],
        );
        assert_eq!(t, "User wants 2+2.");
        assert_eq!(a, "It's 4.");
    }

    #[test]
    fn no_reasoning_format_passes_text_through() {
        let mut p = ReplyParser::new(ReasoningFormat::None, "");
        let (t, a) = collect(&mut p, &["\n\nHi <think> there"]);
        assert_eq!((t.as_str(), a.as_str()), ("", "Hi <think> there"));
    }

    fn words_tokenizer(s: &str) -> u32 {
        s.split_whitespace().count() as u32
    }

    fn joined(msgs: &[ChatMsg]) -> Result<String, ChatError> {
        Ok(msgs
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join(" "))
    }

    #[test]
    fn plan_keeps_everything_when_it_fits() {
        let h = vec![ChatMsg::new("user", "a b c"), ChatMsg::new("assistant", "d e")];
        let plan = plan_prompt(joined, words_tokenizer, Some("sys"), &h, 100, 10).unwrap();
        assert_eq!(plan.dropped_messages, 0);
        assert_eq!(plan.prompt, "sys a b c d e");
    }

    #[test]
    fn plan_drops_oldest_exchanges_first() {
        let h = vec![
            ChatMsg::new("user", "one two three four"),
            ChatMsg::new("assistant", "five six seven eight"),
            ChatMsg::new("user", "nine ten"),
        ];
        // room = 12 - 4 = 8 tokens: "sys nine ten" (3) fits, whole history (11) doesn't.
        let plan = plan_prompt(joined, words_tokenizer, Some("sys"), &h, 12, 4).unwrap();
        assert_eq!(plan.dropped_messages, 2);
        assert_eq!(plan.prompt, "sys nine ten");
    }

    #[test]
    fn plan_reports_message_too_long() {
        let h = vec![ChatMsg::new("user", "w ".repeat(50))];
        assert_eq!(
            plan_prompt(joined, words_tokenizer, None, &h, 40, 10),
            Err(ChatError::TooLong {
                prompt_tokens: 50,
                room_tokens: 30
            })
        );
    }

    #[test]
    fn attachments_come_before_the_question() {
        let c = user_content(
            "Summarize it",
            &[("notes.txt".into(), "  Line one.\n".into())],
        );
        assert_eq!(
            c,
            "[Attached document: notes.txt]\nLine one.\n[End of document: notes.txt]\n\nSummarize it"
        );
    }

    #[test]
    fn word_count_ignores_punctuation_runs() {
        assert_eq!(count_words("Hello, world — this is *fine*."), 5);
        assert_eq!(count_words("```\n```"), 0);
    }
}
