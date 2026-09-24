//! The local inference runtime: one worker thread owns the loaded model and its context.
//!
//! llama.cpp contexts borrow their model, so the worker keeps both as locals in a loop and
//! serves commands from a channel; loading another model simply ends that scope. All public
//! calls block until the worker answers; call them off the UI thread.

use crate::hardware::RuntimeDevice;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use ts_rs::TS;

static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

/// Initializes llama.cpp once. On Windows/Linux the CPU and GPU backends are separate libraries
/// loaded at runtime from `lib_dir` (the app's resource directory) or the executable's folder.
pub fn init_runtime(lib_dir: Option<&Path>) -> &'static LlamaBackend {
    BACKEND.get_or_init(|| {
        let mut backend = LlamaBackend::init().expect("llama backend initializes once");
        backend.void_logs();
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        if llama_cpp_2::list_llama_ggml_backend_devices().is_empty() {
            let dir = lib_dir.map(Path::to_path_buf).or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|e| e.parent().map(Path::to_path_buf))
            });
            if let Some(dir) = dir {
                llama_cpp_2::llama_backend::load_backends_from_path(&dir);
            }
        }
        let _ = lib_dir;
        backend
    })
}

/// Devices llama.cpp can use right now (CPU included).
pub fn devices() -> Vec<RuntimeDevice> {
    use llama_cpp_2::LlamaBackendDeviceType as T;
    llama_cpp_2::list_llama_ggml_backend_devices()
        .into_iter()
        .map(|d| RuntimeDevice {
            index: d.index as u32,
            description: d.description.trim().to_string(),
            backend: d.backend,
            is_gpu: d.device_type == T::Gpu,
            is_integrated: d.device_type == T::IntegratedGpu,
            memory_total: d.memory_total as u64,
            memory_free: d.memory_free as u64,
        })
        .collect()
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum GpuChoice {
    /// Processor only.
    None,
    /// Let llama.cpp place as many layers as fit on this device (the rest stay in system memory).
    Auto { device_index: u32 },
    /// Everything on the GPU (Apple unified memory).
    All,
    /// Advanced override.
    Layers { count: u32 },
}

#[derive(Clone, Debug)]
pub struct LoadOptions {
    pub path: PathBuf,
    pub context: u32,
    pub ubatch: u32,
    pub gpu: GpuChoice,
    pub threads: Option<u32>,
    /// Written before a GPU load and removed once the model has run once. If the app dies in
    /// between, the next launch finds it and falls back to the processor.
    pub crash_marker: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct LoadInfo {
    #[ts(type = "number")]
    pub load_ms: u64,
    pub context: u32,
    /// Layers placed on the GPU (0 = processor only), out of `total_layers`.
    pub gpu_layers: i32,
    pub total_layers: u32,
    pub gpu_device: Option<String>,
    /// Some weights (MoE experts) stay in system memory even though layers run on the GPU.
    pub split_with_cpu: bool,
}

/// Model facts needed to build prompts.
#[derive(Clone, Debug, Default)]
pub struct ModelText {
    pub chat_template: Option<String>,
    pub bos: String,
    pub eos: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq)]
#[ts(export)]
pub struct SamplingParams {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub min_p: f32,
    pub seed: u32,
}

#[derive(Clone)]
pub struct GenerateRequest {
    pub prompt: String,
    pub max_tokens: u32,
    pub sampling: SamplingParams,
    /// Set to true to stop generating; checked between tokens.
    pub stop: Arc<AtomicBool>,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StopReason {
    EndOfText,
    MaxTokens,
    Stopped,
    ContextFull,
}

/// Wall-clock measurements of one generation. These are the only speed numbers opnlocal shows.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq)]
#[ts(export)]
pub struct GenerateStats {
    pub prompt_tokens: u32,
    /// Prompt tokens actually processed (the rest were reused from the previous turn).
    pub prompt_tokens_processed: u32,
    pub prompt_ms: f64,
    /// From the start of the request to the first generated token.
    pub first_token_ms: f64,
    pub generated_tokens: u32,
    /// Time spent generating after the first token.
    pub generation_ms: f64,
    pub stop_reason: StopReason,
}

impl GenerateStats {
    pub fn prompt_tokens_per_second(&self) -> Option<f64> {
        (self.prompt_ms > 0.0 && self.prompt_tokens_processed > 0)
            .then(|| self.prompt_tokens_processed as f64 * 1000.0 / self.prompt_ms)
    }

    /// Tokens after the first, over the time after the first.
    pub fn generation_tokens_per_second(&self) -> Option<f64> {
        (self.generation_ms > 0.0 && self.generated_tokens > 1)
            .then(|| (self.generated_tokens - 1) as f64 * 1000.0 / self.generation_ms)
    }
}

#[derive(thiserror::Error, Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum LlmError {
    #[error("no model is loaded")]
    NotLoaded,
    #[error("the model could not be loaded: {message}")]
    LoadFailed { message: String },
    #[error("the conversation is longer than the model can read at once")]
    ContextFull,
    #[error("the model stopped unexpectedly: {message}")]
    Runtime { message: String },
}

type TextSink = Box<dyn FnMut(&str) + Send>;
type ProgressSink = Box<dyn FnMut(f32) + Send>;

enum Command {
    Load {
        opts: LoadOptions,
        progress: ProgressSink,
        reply: mpsc::Sender<Result<(LoadInfo, ModelText), LlmError>>,
    },
    Unload {
        reply: mpsc::Sender<()>,
    },
    CountTokens {
        text: String,
        reply: mpsc::Sender<Result<u32, LlmError>>,
    },
    Generate {
        req: GenerateRequest,
        on_text: TextSink,
        reply: mpsc::Sender<Result<GenerateStats, LlmError>>,
    },
}

/// Handle to the worker thread. Cheap to clone.
#[derive(Clone)]
pub struct Runtime {
    tx: mpsc::Sender<Command>,
}

impl Runtime {
    pub fn start(lib_dir: Option<&Path>) -> Runtime {
        let backend = init_runtime(lib_dir);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("opnlocal-llm".into())
            // Deep templates and llama.cpp setup can need more than the default stack.
            .stack_size(16 * 1024 * 1024)
            .spawn(move || worker(backend, rx))
            .expect("spawn llm worker");
        Runtime { tx }
    }

    pub fn load(
        &self,
        opts: LoadOptions,
        progress: impl FnMut(f32) + Send + 'static,
    ) -> Result<(LoadInfo, ModelText), LlmError> {
        let (reply, rx) = mpsc::channel();
        self.send(Command::Load {
            opts,
            progress: Box::new(progress),
            reply,
        })?;
        rx.recv().map_err(|_| worker_gone())?
    }

    pub fn unload(&self) {
        let (reply, rx) = mpsc::channel();
        if self.send(Command::Unload { reply }).is_ok() {
            let _ = rx.recv();
        }
    }

    pub fn count_tokens(&self, text: &str) -> Result<u32, LlmError> {
        let (reply, rx) = mpsc::channel();
        self.send(Command::CountTokens {
            text: text.to_string(),
            reply,
        })?;
        rx.recv().map_err(|_| worker_gone())?
    }

    pub fn generate(
        &self,
        req: GenerateRequest,
        on_text: impl FnMut(&str) + Send + 'static,
    ) -> Result<GenerateStats, LlmError> {
        let (reply, rx) = mpsc::channel();
        self.send(Command::Generate {
            req,
            on_text: Box::new(on_text),
            reply,
        })?;
        rx.recv().map_err(|_| worker_gone())?
    }

    fn send(&self, c: Command) -> Result<(), LlmError> {
        self.tx.send(c).map_err(|_| worker_gone())
    }
}

fn worker_gone() -> LlmError {
    LlmError::Runtime {
        message: "the inference worker stopped".into(),
    }
}

fn worker(backend: &'static LlamaBackend, rx: mpsc::Receiver<Command>) {
    let mut next_load: Option<Command> = None;
    loop {
        let cmd = match next_load.take() {
            Some(c) => c,
            None => match rx.recv() {
                Ok(c) => c,
                Err(_) => return,
            },
        };
        let Command::Load {
            opts,
            progress,
            reply,
        } = cmd
        else {
            answer_not_loaded(cmd);
            continue;
        };

        let started = Instant::now();
        let loaded = load_model(backend, &opts, progress);
        let Placed { model, gpu_layers, gpu_device, split_with_cpu } = match loaded {
            Ok(v) => v,
            Err(e) => {
                clear_marker(&opts);
                let _ = reply.send(Err(e));
                continue;
            }
        };
        let mut ctx = match model.new_context(backend, context_params(&opts)) {
            Ok(c) => c,
            Err(e) => {
                clear_marker(&opts);
                let _ = reply.send(Err(LlmError::LoadFailed {
                    message: e.to_string(),
                }));
                continue;
            }
        };
        // Warm up: run one token so GPU buffers exist and a driver crash happens now, inside the
        // crash-guard window, rather than in the middle of the first reply.
        let warm = model.token_bos();
        let mut batch = LlamaBatch::new(1, 1);
        let warm_ok = batch
            .add(warm, 0, &[0], true)
            .ok()
            .and_then(|_| ctx.decode(&mut batch).ok())
            .is_some();
        ctx.clear_kv_cache();
        clear_marker(&opts);
        if !warm_ok {
            let _ = reply.send(Err(LlmError::LoadFailed {
                message: "the model failed its first run".into(),
            }));
            continue;
        }

        let text = ModelText {
            chat_template: model
                .chat_template(None)
                .ok()
                .and_then(|t| t.to_string().ok()),
            bos: piece(&model, model.token_bos()),
            eos: piece(&model, model.token_eos()),
        };
        let info = LoadInfo {
            load_ms: started.elapsed().as_millis() as u64,
            context: ctx.n_ctx(),
            // llama.cpp uses -1 (and any number above the layer count) for "all layers".
            gpu_layers: if gpu_layers < 0 {
                model.n_layer() as i32
            } else {
                gpu_layers.min(model.n_layer() as i32)
            },
            total_layers: model.n_layer(),
            gpu_device,
            split_with_cpu,
        };
        let _ = reply.send(Ok((info, text)));

        let mut cache: Vec<LlamaToken> = Vec::new();
        loop {
            match rx.recv() {
                Ok(c @ Command::Load { .. }) => {
                    next_load = Some(c);
                    break;
                }
                Ok(Command::Unload { reply }) => {
                    let _ = reply.send(());
                    break;
                }
                Ok(Command::CountTokens { text, reply }) => {
                    let n = model
                        .str_to_token(&text, AddBos::Never)
                        .map(|t| t.len() as u32)
                        .map_err(|e| LlmError::Runtime {
                            message: e.to_string(),
                        });
                    let _ = reply.send(n);
                }
                Ok(Command::Generate {
                    req,
                    mut on_text,
                    reply,
                }) => {
                    let r = generate(&model, &mut ctx, &mut cache, &req, &mut on_text);
                    let _ = reply.send(r);
                }
                Err(_) => return,
            }
        }
        // ctx is dropped before model at the end of this scope.
    }
}

fn answer_not_loaded(cmd: Command) {
    match cmd {
        Command::Unload { reply } => {
            let _ = reply.send(());
        }
        Command::CountTokens { reply, .. } => {
            let _ = reply.send(Err(LlmError::NotLoaded));
        }
        Command::Generate { reply, .. } => {
            let _ = reply.send(Err(LlmError::NotLoaded));
        }
        Command::Load { .. } => unreachable!(),
    }
}

fn load_model(
    backend: &LlamaBackend,
    opts: &LoadOptions,
    mut progress: ProgressSink,
) -> Result<Placed, LlmError> {
    let base = LlamaModelParams::default;
    let gpu_device = match opts.gpu {
        GpuChoice::Auto { device_index } => devices()
            .into_iter()
            .find(|d| d.index == device_index)
            .map(|d| d.description),
        GpuChoice::All => devices()
            .into_iter()
            .find(|d| d.is_gpu || d.is_integrated)
            .map(|d| d.description),
        _ => None,
    };

    let mut params = match opts.gpu {
        GpuChoice::None => base().with_n_gpu_layers(0),
        GpuChoice::All => base().with_n_gpu_layers(u32::MAX >> 1),
        GpuChoice::Layers { count } => base().with_n_gpu_layers(count),
        GpuChoice::Auto { device_index } => base()
            .with_devices(&[device_index as usize])
            .map_err(|e| LlmError::LoadFailed {
                message: e.to_string(),
            })?,
    };

    if let GpuChoice::Auto { .. } = opts.gpu {
        // llama.cpp's own fitter decides how many layers (and which MoE experts) go on the GPU.
        let mut cparams = context_params(opts);
        let mut margins = vec![768 * 1024 * 1024usize; llama_max_devices()];
        let path = std::ffi::CString::new(opts.path.to_string_lossy().as_bytes()).map_err(|e| {
            LlmError::LoadFailed {
                message: e.to_string(),
            }
        })?;
        let mut pinned = std::pin::pin!(params);
        let fitted = pinned.as_mut().fit_params(
            &path,
            &mut cparams,
            &mut margins,
            opts.context,
            llama_cpp_sys_2::GGML_LOG_LEVEL_ERROR,
        );
        params = if fitted.is_ok() {
            std::mem::take(&mut *pinned)
        } else {
            // Doesn't fit on the card at all: processor only.
            base().with_n_gpu_layers(0)
        };
    }

    let uses_gpu = !matches!(opts.gpu, GpuChoice::None) && params.n_gpu_layers() != 0;
    if uses_gpu && let Some(marker) = &opts.crash_marker {
        let _ = std::fs::write(
            marker,
            format!(
                "{}\n{}",
                opts.path.display(),
                gpu_device.clone().unwrap_or_default()
            ),
        );
    }
    let gpu_layers = if uses_gpu { params.n_gpu_layers() } else { 0 };
    // The fitter may keep some tensors (MoE experts) in system memory even with every layer
    // "on the GPU"; then the work is really split.
    let split_with_cpu = uses_gpu && !params.tensor_buft_override_patterns().is_empty();
    let params = params.with_progress_callback(move |p| {
        progress(p);
        true
    });
    let model =
        LlamaModel::load_from_file(backend, &opts.path, &params).map_err(|e| LlmError::LoadFailed {
            message: e.to_string(),
        })?;
    Ok(Placed {
        model,
        gpu_layers,
        gpu_device: if uses_gpu { gpu_device } else { None },
        split_with_cpu,
    })
}

struct Placed {
    model: LlamaModel,
    gpu_layers: i32,
    gpu_device: Option<String>,
    split_with_cpu: bool,
}

fn llama_max_devices() -> usize {
    llama_cpp_2::max_devices().max(1)
}

fn context_params(opts: &LoadOptions) -> LlamaContextParams {
    let mut p = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(opts.context))
        .with_n_batch(opts.ubatch.max(512))
        .with_n_ubatch(opts.ubatch)
        .with_no_perf(false);
    if let Some(t) = opts.threads {
        p = p.with_n_threads(t as i32).with_n_threads_batch(t as i32);
    }
    if matches!(opts.gpu, GpuChoice::None) {
        // Truly processor-only: no opportunistic GPU use for large prompt batches.
        p = p.with_offload_kqv(false).with_op_offload(false);
    }
    p
}

fn clear_marker(opts: &LoadOptions) {
    if let Some(m) = &opts.crash_marker {
        let _ = std::fs::remove_file(m);
    }
}

fn piece(model: &LlamaModel, token: LlamaToken) -> String {
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    model
        .token_to_piece(token, &mut decoder, true, None)
        .unwrap_or_default()
}

fn sampler(p: &SamplingParams) -> LlamaSampler {
    if p.temperature <= 0.0 {
        return LlamaSampler::greedy();
    }
    let mut chain = Vec::new();
    // top_k 0 means "no limit" (gpt-oss recommends sampling from the full distribution).
    if p.top_k > 0 {
        chain.push(LlamaSampler::top_k(p.top_k));
    }
    if p.top_p < 1.0 {
        chain.push(LlamaSampler::top_p(p.top_p.max(0.01), 1));
    }
    if p.min_p > 0.0 {
        chain.push(LlamaSampler::min_p(p.min_p, 1));
    }
    chain.push(LlamaSampler::temp(p.temperature));
    chain.push(LlamaSampler::dist(p.seed));
    LlamaSampler::chain_simple(chain)
}

fn generate(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    cache: &mut Vec<LlamaToken>,
    req: &GenerateRequest,
    on_text: &mut TextSink,
) -> Result<GenerateStats, LlmError> {
    let started = Instant::now();
    let rt = |e: &dyn std::fmt::Display| LlmError::Runtime {
        message: e.to_string(),
    };
    let tokens = model
        .str_to_token(&req.prompt, AddBos::Never)
        .map_err(|e| rt(&e))?;
    let n_ctx = ctx.n_ctx() as usize;
    if tokens.len() + 1 >= n_ctx {
        return Err(LlmError::ContextFull);
    }

    // Reuse the part of the previous prompt that is unchanged (the conversation so far).
    let mut keep = cache
        .iter()
        .zip(&tokens)
        .take_while(|(a, b)| a == b)
        .count();
    // We need at least one new token to get fresh logits.
    keep = keep.min(tokens.len() - 1);
    if keep < cache.len() {
        // Recurrent/hybrid models can't roll back part of their state: start over.
        let partial_ok = !model.is_recurrent()
            && !model.is_hybrid()
            && ctx
                .clear_kv_cache_seq(Some(0), Some(keep as u32), None)
                .unwrap_or(false);
        if !partial_ok {
            ctx.clear_kv_cache();
            keep = 0;
        }
        cache.truncate(keep);
    }

    let n_batch = ctx.n_batch() as usize;
    let mut batch = LlamaBatch::new(n_batch.max(1), 1);
    let new_tokens = &tokens[keep..];
    let prompt_started = Instant::now();
    for (ci, chunk) in new_tokens.chunks(n_batch).enumerate() {
        if req.stop.load(Ordering::Relaxed) {
            cache.clear();
            ctx.clear_kv_cache();
            return Ok(stats(&tokens, 0, 0.0, 0.0, 0, 0.0, StopReason::Stopped));
        }
        batch.clear();
        let is_last_chunk = (ci + 1) * n_batch >= new_tokens.len();
        for (i, tok) in chunk.iter().enumerate() {
            let pos = keep + ci * n_batch + i;
            let logits = is_last_chunk && i == chunk.len() - 1;
            batch.add(*tok, pos as i32, &[0], logits).map_err(|e| rt(&e))?;
        }
        if let Err(e) = ctx.decode(&mut batch) {
            cache.clear();
            ctx.clear_kv_cache();
            return Err(rt(&e));
        }
    }
    let prompt_ms = prompt_started.elapsed().as_secs_f64() * 1000.0;
    cache.clear();
    cache.extend_from_slice(&tokens);

    let mut sampler = sampler(&req.sampling);
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut generated = 0u32;
    let mut first_token_ms = 0.0;
    let mut first_at: Option<Instant> = None;
    let mut reason = StopReason::MaxTokens;
    while generated < req.max_tokens {
        let tok = sampler.sample(ctx, -1);
        sampler.accept(tok);
        generated += 1;
        if first_at.is_none() {
            first_at = Some(Instant::now());
            first_token_ms = started.elapsed().as_secs_f64() * 1000.0;
        }
        if model.is_eog_token(tok) {
            reason = StopReason::EndOfText;
            break;
        }
        if let Ok(text) = model.token_to_piece(tok, &mut decoder, true, None)
            && !text.is_empty()
        {
            on_text(&text);
        }
        if req.stop.load(Ordering::Relaxed) {
            reason = StopReason::Stopped;
            break;
        }
        if cache.len() + 1 >= n_ctx {
            reason = StopReason::ContextFull;
            break;
        }
        batch.clear();
        batch
            .add(tok, cache.len() as i32, &[0], true)
            .map_err(|e| rt(&e))?;
        if let Err(e) = ctx.decode(&mut batch) {
            cache.clear();
            ctx.clear_kv_cache();
            return Err(rt(&e));
        }
        cache.push(tok);
    }
    let generation_ms = first_at
        .map(|t| t.elapsed().as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    Ok(stats(
        &tokens,
        new_tokens.len() as u32,
        prompt_ms,
        first_token_ms,
        generated,
        generation_ms,
        reason,
    ))
}

fn stats(
    tokens: &[LlamaToken],
    processed: u32,
    prompt_ms: f64,
    first_token_ms: f64,
    generated: u32,
    generation_ms: f64,
    stop_reason: StopReason,
) -> GenerateStats {
    GenerateStats {
        prompt_tokens: tokens.len() as u32,
        prompt_tokens_processed: processed,
        prompt_ms,
        first_token_ms,
        generated_tokens: generated,
        generation_ms,
        stop_reason,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// llama.cpp tests share one process-wide backend; the app only ever has one runtime, so
    /// tests take turns rather than loading models concurrently.
    pub fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Real-model tests run only when the small test model has been downloaded
    /// (see README: `.test-models/`). They exercise llama.cpp for real, on the CPU.
    pub fn test_model() -> Option<PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-models/stories15M-q4_0.gguf");
        p.exists().then_some(p)
    }

    pub fn load_test_model(rt: &Runtime, context: u32) -> Option<ModelText> {
        let path = test_model()?;
        let (info, text) = rt
            .load(
                LoadOptions {
                    path,
                    context,
                    ubatch: 256,
                    gpu: GpuChoice::None,
                    threads: Some(4),
                    crash_marker: None,
                },
                |_| {},
            )
            .unwrap();
        assert_eq!(info.gpu_layers, 0);
        Some(text)
    }

    fn greedy(prompt: &str, max: u32) -> GenerateRequest {
        GenerateRequest {
            prompt: prompt.into(),
            max_tokens: max,
            sampling: SamplingParams {
                temperature: 0.0,
                top_p: 1.0,
                top_k: 1,
                min_p: 0.0,
                seed: 1,
            },
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn generate_without_model_reports_not_loaded() {
        let _serial = serial();
        let rt = Runtime::start(None);
        assert_eq!(
            rt.generate(greedy("hi", 3), |_| {}),
            Err(LlmError::NotLoaded)
        );
    }

    #[test]
    fn missing_file_fails_to_load_cleanly() {
        let _serial = serial();
        let rt = Runtime::start(None);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.gguf"), b"not a model").unwrap();
        let r = rt.load(
            LoadOptions {
                path: dir.path().join("bad.gguf"),
                context: 512,
                ubatch: 128,
                gpu: GpuChoice::None,
                threads: None,
                crash_marker: None,
            },
            |_| {},
        );
        assert!(matches!(r, Err(LlmError::LoadFailed { .. })));
    }

    #[test]
    fn generates_streams_and_measures() {
        let _serial = serial();
        let rt = Runtime::start(None);
        if load_test_model(&rt, 512).is_none() {
            eprintln!("skipped: test model missing");
            return;
        }
        let text = Arc::new(std::sync::Mutex::new(String::new()));
        let t2 = text.clone();
        let s = rt
            .generate(greedy("<s>Once upon a time", 24), move |p| {
                t2.lock().unwrap().push_str(p)
            })
            .unwrap();
        assert!(s.generated_tokens > 1);
        assert!(!text.lock().unwrap().is_empty());
        assert!(s.generation_tokens_per_second().unwrap() > 0.0);
        assert!(s.first_token_ms > 0.0);
        // Greedy is deterministic: same prompt, same output.
        let text2 = Arc::new(std::sync::Mutex::new(String::new()));
        let t3 = text2.clone();
        rt.generate(greedy("<s>Once upon a time", 24), move |p| {
            t3.lock().unwrap().push_str(p)
        })
        .unwrap();
        assert_eq!(*text.lock().unwrap(), *text2.lock().unwrap());
    }

    #[test]
    fn reuses_the_unchanged_prefix_between_turns() {
        let _serial = serial();
        let rt = Runtime::start(None);
        if load_test_model(&rt, 512).is_none() {
            return;
        }
        let first = rt.generate(greedy("<s>Once upon a time there was", 4), |_| {}).unwrap();
        let second = rt
            .generate(greedy("<s>Once upon a time there was a little dog", 4), |_| {})
            .unwrap();
        assert_eq!(first.prompt_tokens_processed, first.prompt_tokens);
        assert!(second.prompt_tokens_processed < second.prompt_tokens);
    }

    #[test]
    fn stop_flag_ends_generation_early() {
        let _serial = serial();
        let rt = Runtime::start(None);
        if load_test_model(&rt, 512).is_none() {
            return;
        }
        let req = greedy("<s>Once upon a time", 200);
        let stop = req.stop.clone();
        let mut seen = 0;
        let s = rt
            .generate(req, move |_| {
                seen += 1;
                if seen == 3 {
                    stop.store(true, Ordering::Relaxed);
                }
            })
            .unwrap();
        assert_eq!(s.stop_reason, StopReason::Stopped);
        assert!(s.generated_tokens < 10);
    }

    #[test]
    fn prompt_longer_than_context_is_refused() {
        let _serial = serial();
        let rt = Runtime::start(None);
        if load_test_model(&rt, 256).is_none() {
            return;
        }
        let long = "<s>".to_string() + &"once upon a time ".repeat(200);
        assert_eq!(rt.generate(greedy(&long, 4), |_| {}), Err(LlmError::ContextFull));
    }

    #[test]
    fn crash_marker_is_removed_after_successful_load() {
        let _serial = serial();
        let Some(path) = test_model() else { return };
        let rt = Runtime::start(None);
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("gpu-load.marker");
        // CPU loads never write the marker; the path just must not linger either way.
        rt.load(
            LoadOptions {
                path,
                context: 256,
                ubatch: 128,
                gpu: GpuChoice::None,
                threads: None,
                crash_marker: Some(marker.clone()),
            },
            |_| {},
        )
        .unwrap();
        assert!(!marker.exists());
    }
}
