//! Everything the UI can ask for, independent of any UI toolkit.
//!
//! The Tauri app is a thin adapter over [`Engine`]: each command maps to one method, and
//! [`Event`]s are forwarded to the web view. Blocking methods (loading, chatting, benchmarking)
//! must be called off the UI thread.

use crate::bench::{self, BenchmarkResult};
use crate::catalog::{Catalog, CatalogModel, ChatProfile, License, ReasoningFormat, Sampling, ThinkingControl, UseCase};
use crate::chat::{self, ChatError, ChatMsg, Piece, ReplyParser, TemplateOptions};
use crate::docs;
use crate::download::{self, DownloadError, DownloadRequest, Progress};
use crate::ocr;
use crate::fit::{self, MemoryPools, Placement};
use crate::hardware::{self, DeviceInfo, GpuKind, Platform};
use crate::llm::{GenerateRequest, GpuChoice, LlmError, LoadInfo, LoadOptions, ModelText, Runtime, SamplingParams, StopReason};
use crate::recommend::{self, Recommendations};
use crate::retrieval;
use crate::store::{
    self, Attachment, Conversation, ConversationSummary, GpuCrash, ImportedModel, Message, ModelSettings,
    ReplyStats, Role, Settings, Store,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

/// Used when a model file has no chat template of its own (rare; imported files only).
const FALLBACK_TEMPLATE: &str = "{% for m in messages %}<|im_start|>{{ m.role }}\n{{ m.content }}<|im_end|>\n{% endfor %}{% if add_generation_prompt %}<|im_start|>assistant\n{% endif %}";

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Event {
    DownloadProgress {
        model_id: String,
        progress: Progress,
    },
    DownloadFinished {
        model_id: String,
        error: Option<DownloadError>,
    },
    ImportProgress {
        #[ts(type = "number")]
        done: u64,
        #[ts(type = "number")]
        total: u64,
    },
    ModelLoading {
        model_id: String,
        fraction: f32,
    },
    ChatStarted {
        conversation_id: String,
        message_id: String,
        /// Oldest messages left out so the conversation fits the model.
        dropped_messages: u32,
        /// Attached documents were longer than the model can read at once, so only the parts
        /// most relevant to the question were used.
        partial_documents: bool,
    },
    /// Progress through a long prompt (a document) before the first word of the reply.
    ChatReading {
        conversation_id: String,
        fraction: f32,
    },
    ChatDelta {
        conversation_id: String,
        message_id: String,
        thinking: Option<String>,
        answer: Option<String>,
    },
    BenchmarkStage {
        model_id: String,
        stage: BenchStage,
    },
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BenchStage {
    Loading,
    Reading,
    Writing,
    ComparingProcessor,
}

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

#[derive(thiserror::Error, Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum EngineError {
    #[error("unknown model {id}")]
    UnknownModel { id: String },
    #[error("model {id} can't look at images")]
    NoVision { id: String },
    #[error("model {id} is not installed")]
    NotInstalled { id: String },
    #[error("the license for this model must be accepted first")]
    LicenseNotAccepted { license_id: String },
    #[error("another download is in progress")]
    DownloadInProgress { model_id: String },
    #[error("{error}")]
    Download { error: DownloadError },
    #[error("{error}")]
    Llm { error: LlmError },
    #[error("the message is too long for this model")]
    MessageTooLong {
        /// Prompt size relative to what fits, e.g. 2.3 = "about 2.3 times too long".
        times_too_long: f32,
    },
    #[error("{message}")]
    Document { message: String },
    #[error("this model's chat format isn't supported: {message}")]
    Template { message: String },
    #[error("choose a model first")]
    NoModelSelected,
    #[error("conversation not found")]
    UnknownConversation,
    #[error("not a usable model file: {message}")]
    BadModelFile { message: String },
    #[error("couldn't check for new models: {message}")]
    CatalogUnavailable { message: String },
    #[error("{message}")]
    Io { message: String },
}

impl From<LlmError> for EngineError {
    fn from(e: LlmError) -> Self {
        EngineError::Llm { error: e }
    }
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        EngineError::Io {
            message: e.to_string(),
        }
    }
}

/// A model we can run: from the catalog or imported by the user.
#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub memory: crate::catalog::MemoryProfile,
    pub chat: ChatProfile,
    pub license: Option<License>,
    /// The installed image encoder (path, size), if the model has one and it was downloaded.
    pub vision: Option<(PathBuf, u64)>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct PartialDownload {
    pub model_id: String,
    #[ts(type = "number")]
    pub bytes: u64,
}

/// Everything the UI needs to render its screens, in one call.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct AppState {
    pub app_version: String,
    pub platform: Platform,
    pub settings: Settings,
    #[ts(type = "number")]
    pub catalog_version: u64,
    pub models: Vec<CatalogModel>,
    pub imported: Vec<ImportedModel>,
    pub installed: Vec<String>,
    /// Installed models whose image encoder is installed too.
    pub vision_installed: Vec<String>,
    pub partial_downloads: Vec<PartialDownload>,
    pub active_download: Option<String>,
    pub benchmarks: BTreeMap<String, BenchmarkResult>,
    pub conversations: Vec<ConversationSummary>,
    pub storage_path: String,
    /// Loaded model and context, if any.
    pub loaded: Option<LoadedModel>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct LoadedModel {
    pub model_id: String,
    pub info: LoadInfo,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct CatalogRefresh {
    /// False when skipped (checked recently, or automatic checks are off).
    pub checked: bool,
    pub updated: bool,
    #[ts(type = "number")]
    pub version: u64,
}

#[derive(Clone)]
struct Active {
    model_id: String,
    context: u32,
    gpu: GpuChoice,
    /// The image encoder is loaded too.
    vision: bool,
    info: LoadInfo,
    text: ModelText,
}

pub struct Engine {
    store: Store,
    runtime: Runtime,
    catalog: RwLock<Catalog>,
    app_version: String,
    http: reqwest::Client,
    events: EventSink,
    active: Mutex<Option<Active>>,
    download: Mutex<Option<(String, CancellationToken)>>,
    stop: Mutex<Option<Arc<AtomicBool>>>,
    /// One chat/benchmark/load at a time.
    busy: Mutex<()>,
    /// Text recognition for attached images, loaded on first use.
    ocr: OnceLock<ocr::Ocr>,
}

impl Engine {
    /// `root`: app data folder. `lib_dir`: where the llama.cpp backends live (desktop resources).
    pub fn new(
        root: &Path,
        lib_dir: Option<&Path>,
        app_version: &str,
        events: EventSink,
    ) -> Result<Engine, EngineError> {
        let store = Store::open(root)?;
        let runtime = Runtime::start(lib_dir);
        let builtin = Catalog::parse_unverified(crate::BUILTIN_CATALOG).expect("built-in catalog is valid");
        let (json, sig) = store.catalog_cache();
        let cached = match (std::fs::read(&json), std::fs::read_to_string(&sig)) {
            (Ok(bytes), Ok(sig)) => Catalog::parse_verified(&bytes, &sig).ok(),
            _ => None,
        };
        let engine = Engine {
            store,
            runtime,
            catalog: RwLock::new(Catalog::newest(builtin, cached)),
            app_version: app_version.to_string(),
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(20))
                .user_agent(concat!("opnlocal/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("static client config"),
            events,
            active: Mutex::new(None),
            download: Mutex::new(None),
            stop: Mutex::new(None),
            busy: Mutex::new(()),
            ocr: OnceLock::new(),
        };
        engine.check_gpu_crash();
        Ok(engine)
    }

    /// If the previous run died while loading a model on the GPU, remember that and use the
    /// processor from now on (until the user turns graphics back on in Advanced).
    fn check_gpu_crash(&self) {
        let marker = self.store.gpu_marker();
        let Ok(content) = std::fs::read_to_string(&marker) else {
            return;
        };
        let mut lines = content.lines();
        let path = lines.next().unwrap_or_default();
        let device = lines.next().unwrap_or_default().to_string();
        let model_id = Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut s = self.store.settings();
        s.gpu_crash = Some(GpuCrash {
            model_id,
            device,
            at: store::now(),
        });
        let _ = self.store.save_settings(&s);
        let _ = std::fs::remove_file(marker);
    }

    fn emit(&self, e: Event) {
        (self.events)(e)
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn state(&self) -> AppState {
        let catalog = self.catalog.read().unwrap();
        let models: Vec<CatalogModel> = catalog.runnable(&self.app_version).into_iter().cloned().collect();
        let imported = self.store.imported();
        let installed: Vec<String> = models
            .iter()
            .map(|m| m.id.clone())
            .chain(imported.iter().map(|m| m.id.clone()))
            .filter(|id| self.store.is_installed(id))
            .collect();
        let vision_installed = installed.iter().filter(|id| self.store.has_vision(id)).cloned().collect();
        let partial_downloads = models
            .iter()
            .filter_map(|m| {
                let bytes = download::partial_bytes(&self.store.model_path(&m.id));
                (bytes > 0).then(|| PartialDownload {
                    model_id: m.id.clone(),
                    bytes,
                })
            })
            .collect();
        AppState {
            app_version: self.app_version.clone(),
            platform: Platform::current(),
            settings: self.store.settings(),
            catalog_version: catalog.version,
            models,
            imported,
            installed,
            vision_installed,
            partial_downloads,
            active_download: self.download.lock().unwrap().as_ref().map(|(id, _)| id.clone()),
            benchmarks: self.store.benchmarks(),
            conversations: self.store.conversations(),
            storage_path: self.store.root().display().to_string(),
            loaded: self.active.lock().unwrap().as_ref().map(|a| LoadedModel {
                model_id: a.model_id.clone(),
                info: a.info.clone(),
            }),
        }
    }

    pub fn detect(&self) -> DeviceInfo {
        hardware::detect(&self.store.models_dir())
    }

    pub fn recommend(&self, use_case: UseCase) -> Recommendations {
        let device = self.detect();
        let settings = self.store.settings();
        let pools = MemoryPools::for_device(&device, settings.gpu_allowed());
        let catalog = self.catalog.read().unwrap();
        let models = catalog.runnable(&self.app_version);
        recommend::recommend(
            &models,
            &device,
            &pools,
            use_case,
            |id| self.store.is_installed(id),
            |id| download::partial_bytes(&self.store.model_path(id)),
        )
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings, EngineError> {
        let mut s = self.store.settings();
        f(&mut s);
        self.store.save_settings(&s)?;
        Ok(s)
    }

    pub fn accept_license(&self, license_id: &str) -> Result<Settings, EngineError> {
        self.update_settings(|s| {
            s.accepted_licenses.insert(license_id.to_string(), store::now());
        })
    }

    // ---- catalog -------------------------------------------------------------------------

    /// Fetches a newer signed catalog. Automatic checks (`force = false`) run at most once a day
    /// and only if the user hasn't turned them off. Nothing about the device is sent.
    pub async fn refresh_catalog(&self, force: bool) -> Result<CatalogRefresh, EngineError> {
        let settings = self.store.settings();
        let current = self.catalog.read().unwrap().version;
        let recent = settings
            .last_catalog_check
            .is_some_and(|t| store::now().saturating_sub(t) < 24 * 3600);
        if !force && (!settings.auto_refresh_catalog || recent) {
            return Ok(CatalogRefresh {
                checked: false,
                updated: false,
                version: current,
            });
        }
        let fetched = async {
            let body = self.http.get(crate::catalog::CATALOG_URL).send().await?.error_for_status()?.bytes().await?;
            let sig = self.http.get(crate::catalog::SIGNATURE_URL).send().await?.error_for_status()?.text().await?;
            Ok::<_, reqwest::Error>((body, sig))
        }
        .await;
        self.update_settings(|s| s.last_catalog_check = Some(store::now()))?;
        let (body, sig) = fetched.map_err(|e| EngineError::CatalogUnavailable { message: e.to_string() })?;
        let catalog = Catalog::parse_verified(&body, &sig).map_err(|e| EngineError::CatalogUnavailable {
            message: e.to_string(),
        })?;
        let updated = catalog.version > current;
        if updated {
            let (json, sig_path) = self.store.catalog_cache();
            std::fs::write(json, &body)?;
            std::fs::write(sig_path, sig.as_bytes())?;
            *self.catalog.write().unwrap() = catalog.clone();
        }
        Ok(CatalogRefresh {
            checked: true,
            updated,
            version: catalog.version.max(current),
        })
    }

    fn catalog_model(&self, id: &str) -> Option<CatalogModel> {
        self.catalog.read().unwrap().model(id).cloned()
    }

    fn spec(&self, id: &str) -> Result<ModelSpec, EngineError> {
        if let Some(m) = self.catalog_model(id) {
            return Ok(ModelSpec {
                id: m.id.clone(),
                name: m.name.clone(),
                path: self.store.model_path(&m.id),
                memory: m.memory,
                chat: m.chat.clone(),
                license: Some(m.license.clone()),
                vision: m
                    .vision
                    .as_ref()
                    .filter(|_| self.store.has_vision(&m.id))
                    .map(|v| (self.store.vision_path(&m.id), v.size)),
            });
        }
        let imported = self
            .store
            .imported()
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| EngineError::UnknownModel { id: id.to_string() })?;
        let path = self.store.model_path(&imported.id);
        let template = crate::gguf::read_file(&path)
            .ok()
            .and_then(|h| h.str("tokenizer.chat_template").map(String::from))
            .unwrap_or_default();
        Ok(ModelSpec {
            id: imported.id.clone(),
            name: imported.name.clone(),
            path,
            memory: imported.memory,
            chat: guess_chat_profile(&template),
            license: None,
            vision: None,
        })
    }

    // ---- downloads -----------------------------------------------------------------------

    /// Downloads a catalog model. One download at a time; resumes a partial file if present.
    pub async fn download(&self, model_id: &str) -> Result<(), EngineError> {
        self.download_part(model_id, false).await
    }

    /// Downloads a model's image encoder, so it can look at attached images.
    pub async fn download_vision(&self, model_id: &str) -> Result<(), EngineError> {
        self.download_part(model_id, true).await
    }

    async fn download_part(&self, model_id: &str, vision: bool) -> Result<(), EngineError> {
        let model = self
            .catalog_model(model_id)
            .ok_or_else(|| EngineError::UnknownModel { id: model_id.into() })?;
        if model.license.requires_acceptance
            && !self.store.settings().accepted_licenses.contains_key(&model.license.id)
        {
            return Err(EngineError::LicenseNotAccepted {
                license_id: model.license.id.clone(),
            });
        }
        let (file, dest) = if vision {
            let file = model.vision.as_ref().ok_or_else(|| EngineError::NoVision { id: model_id.into() })?;
            (file, self.store.vision_path(model_id))
        } else {
            (&model.file, self.store.model_path(model_id))
        };
        let token = CancellationToken::new();
        {
            let mut d = self.download.lock().unwrap();
            if let Some((id, _)) = d.as_ref() {
                return Err(EngineError::DownloadInProgress { model_id: id.clone() });
            }
            *d = Some((model_id.to_string(), token.clone()));
        }
        let req = DownloadRequest {
            url: file.url(),
            dest,
            size: file.size,
            sha256: file.sha256.clone(),
        };
        let events = self.events.clone();
        let id = model_id.to_string();
        let result = download::download(
            &download::client(),
            &req,
            &token,
            &download::Options::default(),
            &move |progress| {
                events(Event::DownloadProgress {
                    model_id: id.clone(),
                    progress,
                })
            },
        )
        .await;
        *self.download.lock().unwrap() = None;
        self.emit(Event::DownloadFinished {
            model_id: model_id.to_string(),
            error: result.clone().err(),
        });
        result.map_err(|error| EngineError::Download { error })
    }

    pub fn cancel_download(&self) {
        if let Some((_, token)) = self.download.lock().unwrap().as_ref() {
            token.cancel();
        }
    }

    /// Removes a model (or a partial download of it) from disk.
    pub fn delete_model(&self, id: &str) -> Result<(), EngineError> {
        let _busy = self.busy.lock().unwrap();
        let mut active = self.active.lock().unwrap();
        if active.as_ref().is_some_and(|a| a.model_id == id) {
            self.runtime.unload();
            *active = None;
        }
        self.store.delete_model(id)?;
        Ok(())
    }

    /// Copies a user-provided GGUF file into the models folder. Unverified: never recommended.
    pub fn import_model(&self, file_name: &str, mut reader: impl Read, size: u64) -> Result<ImportedModel, EngineError> {
        let stem = Path::new(file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "model".into());
        let slug: String = stem
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .chars()
            .take(48)
            .collect();
        let id = format!("imported-{}-{}", if slug.is_empty() { "model" } else { &slug }, &store::new_id()[..6]);
        let dest = self.store.model_path(&id);
        let tmp = download::part_path(&dest);
        let copy = (|| -> std::io::Result<u64> {
            let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            let mut buf = vec![0u8; 4 * 1024 * 1024];
            let mut done = 0u64;
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                std::io::Write::write_all(&mut out, &buf[..n])?;
                done += n as u64;
                self.emit(Event::ImportProgress { done, total: size });
            }
            std::io::Write::flush(&mut out)?;
            Ok(done)
        })();
        let written = match copy {
            Ok(n) => n,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
        };
        let header = match crate::gguf::read_file(&tmp) {
            Ok(h) => h,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(EngineError::BadModelFile { message: e.to_string() });
            }
        };
        std::fs::rename(&tmp, &dest)?;
        let model = ImportedModel {
            id: id.clone(),
            name: header.str("general.name").map(String::from).unwrap_or(stem),
            file: format!("{id}.gguf"),
            size: written,
            memory: header.memory_profile(written),
            imported_at: store::now(),
        };
        let mut list = self.store.imported();
        list.push(model.clone());
        self.store.save_imported(&list)?;
        Ok(model)
    }

    // ---- loading -------------------------------------------------------------------------

    fn choose_gpu(&self, device: &DeviceInfo, settings: &Settings, ms: &ModelSettings, placement: Placement) -> GpuChoice {
        if !settings.gpu_allowed() {
            return GpuChoice::None;
        }
        match ms.gpu_layers {
            Some(0) => return GpuChoice::None,
            Some(n) => return GpuChoice::Layers { count: n },
            None => {}
        }
        let best = |kind: GpuKind| {
            device
                .gpus
                .iter()
                .filter(|g| g.kind == kind)
                .max_by_key(|g| g.memory_free)
        };
        if best(GpuKind::Unified).is_some() {
            return GpuChoice::All;
        }
        if let Some(g) = best(GpuKind::Discrete)
            && placement != Placement::Cpu
        {
            return GpuChoice::Auto { device_index: g.device_index };
        }
        if ms.use_integrated_gpu
            && let Some(g) = best(GpuKind::Integrated)
        {
            return GpuChoice::Auto { device_index: g.device_index };
        }
        GpuChoice::None
    }

    /// Loads the model unless it is already loaded with a context of at least `needed` tokens
    /// (prompt plus reply room) on the same device.
    fn ensure_loaded(
        &self,
        spec: &ModelSpec,
        use_case: UseCase,
        gpu_override: Option<GpuChoice>,
        needed: u32,
        vision: bool,
    ) -> Result<Active, EngineError> {
        if !spec.path.is_file() {
            return Err(EngineError::NotInstalled { id: spec.id.clone() });
        }
        let settings = self.store.settings();
        let ms = settings.per_model.get(&spec.id).cloned().unwrap_or_default();
        let device = self.detect();
        let ubatch = fit::ubatch_for(device.platform);
        let vision = vision && spec.vision.is_some();
        let (context, fit) = planned_context(spec, use_case, &settings, &device, needed, vision);
        let gpu = gpu_override.unwrap_or_else(|| self.choose_gpu(&device, &settings, &ms, fit.placement));
        let threads = ms.threads.or(Some(if device.platform.is_mobile() {
            device.cpu.cores.clamp(1, 4)
        } else {
            device.cpu.cores.max(1)
        }));

        let mut active = self.active.lock().unwrap();
        if let Some(a) = active.as_ref()
            && a.model_id == spec.id
            && a.context >= context
            && a.gpu == gpu
            && (a.vision || !vision)
        {
            return Ok(a.clone());
        }
        *active = None;
        let events = self.events.clone();
        let id = spec.id.clone();
        let (info, text) = self.runtime.load(
            LoadOptions {
                path: spec.path.clone(),
                context,
                ubatch,
                gpu,
                threads,
                crash_marker: Some(self.store.gpu_marker()),
                vision: spec.vision.as_ref().filter(|_| vision).map(|(p, _)| p.clone()),
            },
            move |fraction| {
                events(Event::ModelLoading {
                    model_id: id.clone(),
                    fraction,
                })
            },
        )?;
        let a = Active {
            model_id: spec.id.clone(),
            context: info.context,
            gpu,
            vision: info.vision,
            info,
            text,
        };
        *active = Some(a.clone());
        Ok(a)
    }

    // ---- benchmark -----------------------------------------------------------------------

    /// Loads the model fresh and runs the fixed benchmark. On machines whose only GPU is
    /// integrated, it also measures the processor and keeps whichever is faster.
    pub fn benchmark(&self, model_id: &str) -> Result<BenchmarkResult, EngineError> {
        let _busy = self.busy.lock().unwrap();
        let spec = self.spec(model_id)?;
        let use_case = self.store.settings().use_case.unwrap_or(UseCase::Everyday);
        let stage = |stage| {
            self.emit(Event::BenchmarkStage {
                model_id: model_id.to_string(),
                stage,
            })
        };
        let device = self.detect();
        let has_discrete_or_unified = device
            .gpus
            .iter()
            .any(|g| matches!(g.kind, GpuKind::Discrete | GpuKind::Unified));
        let integrated = device.gpus.iter().find(|g| g.kind == GpuKind::Integrated).cloned();

        let run = |gpu: Option<GpuChoice>| -> Result<BenchmarkResult, EngineError> {
            stage(BenchStage::Loading);
            self.unload();
            let gpu_free_before = gpu_free_total();
            let active = self.ensure_loaded(&spec, use_case, gpu, 0, false)?;
            stage(BenchStage::Reading);
            let m = bench::measure(&self.runtime, &active.text, &spec.chat, FALLBACK_TEMPLATE)?;
            stage(BenchStage::Writing);
            let gpu_used = gpu_free_before.saturating_sub(gpu_free_total());
            let on_gpu = active.info.gpu_layers > 0;
            let placement = if !on_gpu {
                Placement::Cpu
            } else if active.info.gpu_layers as u32 >= active.info.total_layers && !active.info.split_with_cpu {
                Placement::Gpu
            } else {
                Placement::Mixed
            };
            Ok(BenchmarkResult {
                model_id: spec.id.clone(),
                app_version: self.app_version.clone(),
                measured_at: store::now(),
                device: active.info.gpu_device.clone().unwrap_or_else(|| device.cpu.name.clone()),
                placement,
                gpu_layers: active.info.gpu_layers,
                context: active.info.context,
                load_ms: active.info.load_ms,
                prompt_tokens: m.prompt_tokens,
                prompt_tokens_per_second: m.prompt_tokens_per_second,
                first_token_ms: m.first_token_ms,
                generated_tokens: m.generated_tokens,
                generation_tokens_per_second: m.generation_tokens_per_second,
                words: m.words,
                words_per_second: m.words_per_second,
                verdict: m.words_per_second.map(bench::verdict),
                ram_in_use_bytes: process_memory(),
                gpu_memory_used_bytes: (on_gpu && gpu_used > 0).then_some(gpu_used),
                hit_time_limit: m.hit_time_limit,
            })
        };

        let settings = self.store.settings();
        let result = match integrated {
            Some(ig) if !has_discrete_or_unified && settings.gpu_allowed() => {
                let on_gpu = run(Some(GpuChoice::Auto { device_index: ig.device_index }));
                stage(BenchStage::ComparingProcessor);
                let on_cpu = run(Some(GpuChoice::None))?;
                let gpu_wins = on_gpu.as_ref().is_ok_and(|g| {
                    g.generation_tokens_per_second.unwrap_or(0.0) > on_cpu.generation_tokens_per_second.unwrap_or(0.0) * 1.1
                });
                self.update_settings(|s| {
                    s.per_model.entry(model_id.to_string()).or_default().use_integrated_gpu = gpu_wins;
                })?;
                if gpu_wins { on_gpu? } else { on_cpu }
            }
            _ => run(None)?,
        };
        self.store.save_benchmark(model_id, &result)?;
        Ok(result)
    }

    pub fn unload(&self) {
        let mut active = self.active.lock().unwrap();
        if active.take().is_some() {
            self.runtime.unload();
        }
    }

    // ---- chat ----------------------------------------------------------------------------

    pub fn conversations(&self) -> Vec<ConversationSummary> {
        self.store.conversations()
    }

    pub fn conversation(&self, id: &str) -> Result<Conversation, EngineError> {
        self.store.conversation(id).ok_or(EngineError::UnknownConversation)
    }

    pub fn delete_conversation(&self, id: &str) -> Result<(), EngineError> {
        Ok(self.store.delete_conversation(id)?)
    }

    pub fn delete_all_conversations(&self) -> Result<(), EngineError> {
        Ok(self.store.delete_all_conversations()?)
    }

    /// Reads an attached document (any platform: the caller passes the bytes).
    pub fn extract_document(&self, name: &str, bytes: &[u8]) -> Result<Attachment, EngineError> {
        if bytes.len() as u64 > docs::MAX_FILE_BYTES {
            return Err(EngineError::Document { message: docs::DocError::TooLarge.to_string() });
        }
        let document = |e: docs::DocError| EngineError::Document { message: e.to_string() };
        // Without an extension in the name (Android's picker often gives just an id, like
        // "image:1234"), the type comes from the bytes and the name gets the extension.
        let named = Path::new(name).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        let (ext, name) = match named {
            Some(ext) => (ext, name.to_string()),
            None => {
                let sniffed = docs::sniff_extension(bytes).ok_or_else(|| document(docs::DocError::Unsupported))?;
                (sniffed.to_string(), format!("{name}.{sniffed}"))
            }
        };
        let name = name.as_str();
        if !ocr::IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            let text = docs::extract_bytes(&ext, bytes).map_err(document)?;
            return Ok(Attachment { name: name.to_string(), text, image: None });
        }
        // Images: read their text now (for models that can't see) and keep a shrunk copy (for
        // models that can). Whether either is enough is judged against the model when attached.
        let text = self.ocr()?.read(bytes).map_err(document)?;
        let image = self.store.save_image(&ocr::shrink_image(bytes, IMAGE_MAX_SIDE).map_err(document)?)?;
        Ok(Attachment { name: name.to_string(), text, image: Some(image) })
    }

    fn ocr(&self) -> Result<&ocr::Ocr, EngineError> {
        if let Some(o) = self.ocr.get() {
            return Ok(o);
        }
        let loaded = ocr::Ocr::load().map_err(|message| EngineError::Document { message })?;
        Ok(self.ocr.get_or_init(|| loaded))
    }

    /// Whether these attachments are of use to this model, judged when they are attached so the
    /// app can say no right away. Length is no obstacle: documents longer than the model can
    /// read at once are cut down to the parts relevant to each question (see `retrieval`).
    pub fn check_attachments(&self, model_id: &str, attachments: &[Attachment]) -> Result<(), EngineError> {
        let spec = self.spec(model_id)?;
        if spec.vision.is_none()
            && let Some(a) = attachments.iter().find(|a| a.image.is_some() && a.text.trim().is_empty())
        {
            return Err(EngineError::Document {
                message: format!("no text was found in {}, and this model can't look at images", a.name),
            });
        }
        Ok(())
    }

    /// Adds a user message (to a new conversation if `conversation_id` is None) and streams the
    /// reply as [`Event::ChatDelta`]s. Returns the saved conversation.
    pub fn send_message(
        &self,
        conversation_id: Option<&str>,
        text: &str,
        attachments: Vec<Attachment>,
        think_harder: bool,
    ) -> Result<Conversation, EngineError> {
        let settings = self.store.settings();
        let mut conv = match conversation_id {
            Some(id) => self.conversation(id)?,
            None => {
                let model_id = settings.active_model.clone().ok_or(EngineError::NoModelSelected)?;
                Conversation {
                    id: store::new_id(),
                    title: title_from(text, &attachments),
                    model_id,
                    use_case: settings.use_case.unwrap_or(UseCase::Everyday),
                    think_harder,
                    created_at: store::now(),
                    updated_at: store::now(),
                    messages: vec![],
                }
            }
        };
        conv.think_harder = think_harder;
        conv.messages.push(Message {
            id: store::new_id(),
            role: Role::User,
            content: text.to_string(),
            thinking: None,
            attachments,
            created_at: store::now(),
            stats: None,
            stopped: false,
        });
        conv.updated_at = store::now();
        self.store.save_conversation(&conv)?;
        self.reply(conv)
    }

    /// Replaces the last reply with a new one.
    pub fn regenerate(&self, conversation_id: &str, think_harder: bool) -> Result<Conversation, EngineError> {
        let mut conv = self.conversation(conversation_id)?;
        while conv.messages.last().is_some_and(|m| m.role == Role::Assistant) {
            conv.messages.pop();
        }
        conv.think_harder = think_harder;
        self.store.save_conversation(&conv)?;
        self.reply(conv)
    }

    /// Switches a conversation to another installed model.
    pub fn set_conversation_model(&self, conversation_id: &str, model_id: &str) -> Result<Conversation, EngineError> {
        let mut conv = self.conversation(conversation_id)?;
        self.spec(model_id)?;
        conv.model_id = model_id.to_string();
        self.store.save_conversation(&conv)?;
        Ok(conv)
    }

    pub fn stop_generation(&self) {
        if let Some(stop) = self.stop.lock().unwrap().as_ref() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    fn reply(&self, mut conv: Conversation) -> Result<Conversation, EngineError> {
        let _busy = self.busy.lock().unwrap();
        let spec = self.spec(&conv.model_id)?;
        let settings = self.store.settings();
        let ms = settings.per_model.get(&spec.id).cloned().unwrap_or_default();
        let system = ms.system_prompt.clone().unwrap_or_else(|| default_system_prompt(conv.use_case).into());

        // Images go to the model as pictures when its image encoder is installed, otherwise as
        // the text read from them.
        let see_images = spec.vision.is_some()
            && conv.messages.iter().flat_map(|m| &m.attachments).any(|a| a.image.is_some());
        let mut images: Vec<Vec<u8>> = Vec::new();
        let build_history = |trim: Option<u32>| -> Result<Vec<ChatMsg>, EngineError> {
            let mut history = Vec::with_capacity(conv.messages.len());
            let question = conv.messages.iter().rev().find(|m| m.role == Role::User).map(|m| m.content.as_str()).unwrap_or("");
            for m in &conv.messages {
                match m.role {
                    Role::User => {
                        let mut attachments = m.attachments.clone();
                        if let Some(budget) = trim {
                            for a in attachments.iter_mut().filter(|a| a.image.is_none() || !see_images) {
                                a.text = retrieval::select(question, &a.text, budget);
                            }
                        }
                        history.push(ChatMsg::new("user", chat::user_content(&m.content, &attachments, see_images)));
                    }
                    Role::Assistant => history.push(ChatMsg::new("assistant", m.content.clone())),
                }
            }
            Ok(history)
        };
        if see_images {
            for file in conv.messages.iter().flat_map(|m| &m.attachments).filter_map(|a| a.image.as_deref()) {
                let path = self.store.image_path(file).ok_or(EngineError::UnknownConversation)?;
                images.push(std::fs::read(path)?);
            }
        }
        let mut history = build_history(None)?;

        // The context is sized for the whole conversation from a rough token count. Should the
        // exact count (known once the model is loaded) not fit, reload once with room for it.
        let rough = |history: &[ChatMsg]| -> u32 {
            history.iter().map(|m| chat::estimate_tokens(&m.content)).sum::<u32>()
                + chat::estimate_tokens(&system)
                + images.len() as u32 * IMAGE_TOKENS
                + reply_room(conv.think_harder)
        };
        let mut needed = rough(&history);
        // Documents longer than the largest context this device can give the model are cut
        // down to the parts that matter for the question (see `retrieval`).
        let (largest, _) = planned_context(&spec, conv.use_case, &settings, &self.detect(), needed, see_images);
        let docs: usize = conv.messages.iter().map(|m| m.attachments.len()).sum();
        let mut partial_documents = needed > largest && docs > 0;
        // Tokens per attached document; None = whole documents.
        let mut budget: Option<u32> = None;
        if partial_documents {
            let without_docs = rough(&build_history(Some(0))?);
            budget = Some(largest.saturating_sub(without_docs) / docs as u32);
            history = build_history(budget)?;
            needed = rough(&history);
        }
        let mut active = self.ensure_loaded(&spec, conv.use_case, None, needed, see_images)?;
        let mut reloaded = false;
        let mut trims = 0;
        let (plan, reserve) = loop {
            let template = active.text.chat_template.clone().unwrap_or_else(|| FALLBACK_TEMPLATE.into());
            let reserve = reply_reserve(conv.think_harder, active.context);
            let opts = TemplateOptions {
                thinking: &spec.chat.thinking,
                think_harder: conv.think_harder,
                bos_token: &active.text.bos,
                eos_token: &active.text.eos,
                vars: &spec.chat.template_vars,
            };
            match chat::plan_prompt(
                |msgs| chat::render(&template, msgs, &opts),
                // Each image marker in the text becomes a block of image tokens.
                |t| self.runtime.count_tokens(t).unwrap_or(u32::MAX).saturating_add(images.len() as u32 * IMAGE_TOKENS),
                Some(&system),
                &history,
                active.context,
                reserve,
            ) {
                Ok(plan) => break (plan, reserve),
                Err(ChatError::TooLong { prompt_tokens, room_tokens }) => {
                    let exact = prompt_tokens.saturating_add(reply_room(conv.think_harder));
                    let (bigger, _) = planned_context(&spec, conv.use_case, &settings, &self.detect(), exact, see_images);
                    if !reloaded && bigger > active.context {
                        reloaded = true;
                        active = self.ensure_loaded(&spec, conv.use_case, None, exact, see_images)?;
                        continue;
                    }
                    // The rough count under-counted (some tokenizers, some languages): cut the
                    // documents down by the ratio the exact count showed, a few times at most.
                    if docs > 0 && trims < 3 {
                        trims += 1;
                        let ratio = prompt_tokens as f32 / room_tokens.max(1) as f32;
                        let current = budget.unwrap_or(active.context);
                        budget = Some((current as f32 / ratio * 0.85) as u32);
                        history = build_history(budget)?;
                        partial_documents = true;
                        continue;
                    }
                    return Err(EngineError::MessageTooLong {
                        times_too_long: prompt_tokens as f32 / room_tokens.max(1) as f32,
                    });
                }
                Err(ChatError::Template(message)) => return Err(EngineError::Template { message }),
            }
        };

        let base = if conv.think_harder {
            spec.chat.thinking_sampling.unwrap_or(spec.chat.sampling)
        } else {
            spec.chat.sampling
        };
        let sampling = SamplingParams {
            temperature: ms.temperature.unwrap_or(base.temperature),
            top_p: ms.top_p.unwrap_or(base.top_p),
            top_k: base.top_k,
            min_p: base.min_p,
            seed: rand_seed(),
        };
        let stop = Arc::new(AtomicBool::new(false));
        *self.stop.lock().unwrap() = Some(stop.clone());

        let message_id = store::new_id();
        self.emit(Event::ChatStarted {
            conversation_id: conv.id.clone(),
            message_id: message_id.clone(),
            dropped_messages: plan.dropped_messages as u32,
            partial_documents,
        });
        let parser = Arc::new(Mutex::new(ReplyParser::new(spec.chat.reasoning, &plan.prompt)));
        let collected = Arc::new(Mutex::new((String::new(), String::new())));
        let emit_pieces = {
            let events = self.events.clone();
            let collected = collected.clone();
            let (cid, mid) = (conv.id.clone(), message_id.clone());
            move |pieces: Vec<Piece>| {
                for p in pieces {
                    let mut c = collected.lock().unwrap();
                    let (thinking, answer) = match p {
                        Piece::Thinking(t) => {
                            c.0.push_str(&t);
                            (Some(t), None)
                        }
                        Piece::Answer(a) => {
                            c.1.push_str(&a);
                            (None, Some(a))
                        }
                    };
                    events(Event::ChatDelta {
                        conversation_id: cid.clone(),
                        message_id: mid.clone(),
                        thinking,
                        answer,
                    });
                }
            }
        };
        let raw = Arc::new(Mutex::new(String::new()));
        let on_progress: Arc<dyn Fn(f32) + Send + Sync> = {
            let (events, cid) = (self.events.clone(), conv.id.clone());
            Arc::new(move |fraction| events(Event::ChatReading { conversation_id: cid.clone(), fraction }))
        };
        let run = |prompt: String, max_tokens: u32| {
            let (p2, e2, r2) = (parser.clone(), emit_pieces.clone(), raw.clone());
            self.runtime.generate(
                GenerateRequest {
                    prompt,
                    max_tokens,
                    sampling,
                    stop: stop.clone(),
                    on_progress: Some(on_progress.clone()),
                    images: images.clone(),
                },
                move |text| {
                    r2.lock().unwrap().push_str(text);
                    let pieces = p2.lock().unwrap().push(text);
                    e2(pieces);
                },
            )
        };
        // Thinking budget: small models can reason until they run out of room and never answer.
        // After 3/4 of the budget, close the reasoning in the model's own format and ask for the answer.
        let budget = ms.max_reply_tokens.unwrap_or(reserve);
        let may_think = conv.think_harder || spec.chat.thinking.thinks_by_default();
        let thinking_budget = if may_think { budget * 3 / 4 } else { budget };
        let first = run(plan.prompt.clone(), thinking_budget);
        let result = match first {
            Ok(s) if s.stop_reason == StopReason::MaxTokens
                && parser.lock().unwrap().is_thinking()
                && budget > thinking_budget =>
            {
                let close = parser.lock().unwrap().close_thinking().unwrap_or_default();
                let pieces = parser.lock().unwrap().push(close);
                emit_pieces(pieces);
                let prompt = format!("{}{}{close}", plan.prompt, raw.lock().unwrap());
                run(prompt, budget - thinking_budget).map(|second| sum_stats(s, second))
            }
            other => other,
        };
        *self.stop.lock().unwrap() = None;
        let stats = result?;
        emit_pieces(parser.lock().unwrap().finish());

        let (thinking, answer) = collected.lock().unwrap().clone();
        conv.messages.push(Message {
            id: message_id,
            role: Role::Assistant,
            content: answer.trim_end().to_string(),
            thinking: (!thinking.trim().is_empty()).then(|| thinking.trim().to_string()),
            attachments: vec![],
            created_at: store::now(),
            stats: Some(ReplyStats {
                tokens: stats.generated_tokens,
                seconds: (stats.generation_ms / 1000.0) as f32,
                words: chat::count_words(&answer),
            }),
            stopped: stats.stop_reason == StopReason::Stopped,
        });
        conv.updated_at = store::now();
        self.store.save_conversation(&conv)?;
        Ok(conv)
    }
}

/// Combines the two parts of a reply whose thinking was cut short (see `reply`).
fn sum_stats(a: crate::llm::GenerateStats, b: crate::llm::GenerateStats) -> crate::llm::GenerateStats {
    crate::llm::GenerateStats {
        generated_tokens: a.generated_tokens + b.generated_tokens,
        generation_ms: a.generation_ms + b.generation_ms,
        stop_reason: b.stop_reason,
        ..a
    }
}

fn default_system_prompt(use_case: UseCase) -> &'static str {
    match use_case {
        UseCase::Everyday => {
            "You are a helpful assistant running privately on the user's own device. Answer clearly and concisely, in the user's language."
        }
        UseCase::Coding => {
            "You are a careful programming assistant running on the user's own device. Give working code, explain briefly, and state your assumptions."
        }
        UseCase::Writing => {
            "You are a writing assistant running on the user's own device. Keep the user's meaning and voice, and write clearly."
        }
        UseCase::Documents => {
            "You help the user with their private documents, entirely on their own device. Base answers on the attached documents, quote them when useful, and say so when the answer isn't in them."
        }
    }
}

/// Context size the model gets when loaded: the best fit for this device and use case, grown
/// when the conversation needs `needed` tokens (see [`fit::context_for`]), unless the user set
/// one; kept within the model's own limit.
fn planned_context(
    spec: &ModelSpec,
    use_case: UseCase,
    settings: &Settings,
    device: &DeviceInfo,
    needed: u32,
    vision: bool,
) -> (u32, fit::Fit) {
    let ms = settings.per_model.get(&spec.id).cloned().unwrap_or_default();
    let pools = MemoryPools::for_device(device, settings.gpu_allowed());
    let desired = fit::desired_context(use_case, device.platform);
    let mut memory = spec.memory;
    if vision && let Some((_, bytes)) = spec.vision {
        // The encoder's weights plus its working buffers for one image.
        memory.fixed_bytes += bytes + VISION_SCRATCH_BYTES;
    }
    let fit = fit::context_for(&memory, &pools, desired, needed, fit::ubatch_for(device.platform));
    let context = ms.context.unwrap_or(fit.context).clamp(fit::MIN_CONTEXT, spec.memory.max_context.max(fit::MIN_CONTEXT));
    (context, fit)
}

/// Working memory of the image encoder for one (already shrunk) image.
const VISION_SCRATCH_BYTES: u64 = 512 * 1024 * 1024;
/// Context tokens one attached image takes, roughly (images are shrunk to at most
/// [`IMAGE_MAX_SIDE`] pixels; the exact number depends on the model).
const IMAGE_TOKENS: u32 = 1024;
/// Attached images are shrunk to this on their longer side before being saved.
const IMAGE_MAX_SIDE: u32 = 1024;

/// Context tokens kept free for the reply, so the prompt may use the rest.
fn reply_reserve(think_harder: bool, context: u32) -> u32 {
    reply_room(think_harder).min(context / 2)
}

/// Reply room asked for before the context size is known.
fn reply_room(think_harder: bool) -> u32 {
    if think_harder { 4096 } else { 1024 }
}

/// A short title from the first message (or the first attachment's name).
fn title_from(text: &str, attachments: &[Attachment]) -> String {
    let first_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let base = if first_line.is_empty() {
        attachments.first().map(|a| a.name.as_str()).unwrap_or("New chat")
    } else {
        first_line
    };
    if base.chars().count() <= 60 {
        return base.to_string();
    }
    let cut: String = base.chars().take(60).collect();
    match cut.rfind(' ') {
        Some(i) if i > 30 => format!("{}…", &cut[..i]),
        _ => format!("{cut}…"),
    }
}

/// Best-effort chat settings for an imported file, from its template.
pub fn guess_chat_profile(template: &str) -> ChatProfile {
    let reasoning = if template.contains("<|channel|>") {
        ReasoningFormat::Harmony
    } else if template.contains("<think>") {
        ReasoningFormat::ThinkTags
    } else {
        ReasoningFormat::None
    };
    let thinking = if template.contains("enable_thinking") {
        ThinkingControl::TemplateFlag { variable: "enable_thinking".into() }
    } else if template.contains("reasoning_effort") {
        ThinkingControl::AlwaysOn { variable: "reasoning_effort".into(), normal: "low".into(), harder: "high".into() }
    } else {
        ThinkingControl::Unsupported
    };
    ChatProfile {
        reasoning,
        thinking,
        sampling: Sampling { temperature: 0.7, top_p: 0.9, top_k: 40, min_p: 0.05 },
        thinking_sampling: None,
        template_vars: Default::default(),
    }
}

fn rand_seed() -> u32 {
    uuid::Uuid::new_v4().as_u128() as u32
}

fn gpu_free_total() -> u64 {
    crate::llm::devices()
        .iter()
        .filter(|d| d.is_gpu || d.is_integrated)
        .map(|d| d.memory_free)
        .sum()
}

fn process_memory() -> Option<u64> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = sysinfo::get_current_pid().ok()?;
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory());
    sys.process(pid).map(|p| p.memory())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_short_and_readable() {
        assert_eq!(title_from("  \nHow do I bake bread?", &[]), "How do I bake bread?");
        let long = "Please help me rewrite this cover letter so it sounds more confident but still polite";
        let t = title_from(long, &[]);
        assert!(t.ends_with('…') && t.chars().count() <= 61, "{t}");
        let a = Attachment { name: "report.pdf".into(), text: "x".into(), image: None };
        assert_eq!(title_from("", &[a]), "report.pdf");
    }

    /// The whole chat flow through the same API the app uses, with a real (tiny) model imported
    /// from a local file: import → chat (events streamed) → regenerate → too long → delete.
    #[test]
    fn import_chat_regenerate_and_delete_a_local_model() {
        let _serial = crate::llm::tests::serial();
        let Some(path) = crate::llm::tests::test_model() else {
            eprintln!("skipped: test model missing");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let seen = Arc::new(Mutex::new(Vec::<Event>::new()));
        let s2 = seen.clone();
        let engine = Engine::new(dir.path(), None, "0.1.0", Arc::new(move |e| s2.lock().unwrap().push(e))).unwrap();

        let size = std::fs::metadata(&path).unwrap().len();
        let model = engine
            .import_model("stories15M-q4_0.gguf", std::fs::File::open(&path).unwrap(), size)
            .unwrap();
        assert!(engine.state().installed.contains(&model.id));
        assert!(model.memory.kv_bytes_per_token > 0);
        engine
            .update_settings(|s| {
                s.active_model = Some(model.id.clone());
                s.per_model.insert(model.id.clone(), ModelSettings { max_reply_tokens: Some(24), ..Default::default() });
            })
            .unwrap();

        let conv = engine.send_message(None, "Once upon a time", vec![], false).unwrap();
        assert_eq!(conv.messages.len(), 2);
        let reply = &conv.messages[1];
        assert_eq!(reply.role, Role::Assistant);
        let stats = reply.stats.as_ref().unwrap();
        assert!(stats.tokens > 0 && stats.tokens <= 24);
        {
            let events = seen.lock().unwrap();
            assert!(events.iter().any(|e| matches!(e, Event::ChatStarted { .. })));
            let streamed: String = events
                .iter()
                .filter_map(|e| match e {
                    Event::ChatDelta { answer: Some(a), .. } => Some(a.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(streamed.trim_end(), reply.content, "the saved reply is what was streamed");
        }
        // Saved to disk and listed.
        assert_eq!(engine.conversations()[0].id, conv.id);

        let again = engine.regenerate(&conv.id, false).unwrap();
        assert_eq!(again.messages.len(), 2, "regenerate replaces the last reply");

        let huge = "word ".repeat(6000);
        match engine.send_message(Some(&conv.id), &huge, vec![], false) {
            Err(EngineError::MessageTooLong { times_too_long }) => assert!(times_too_long > 1.0),
            other => panic!("expected MessageTooLong, got {other:?}"),
        }
        // A document too long to read whole is cut down to the parts relevant to the question.
        let doc = |text: &str| Attachment { name: "notes.txt".into(), text: text.into(), image: None };
        assert_eq!(engine.check_attachments(&model.id, &[doc(&huge)]), Ok(()));
        let long_doc = format!("{huge} The deadline is 14 October. {huge}");
        let conv = engine.send_message(None, "When is the deadline?", vec![doc(&long_doc)], false).unwrap();
        assert_eq!(conv.messages.len(), 2);
        let started = seen.lock().unwrap().iter().rev().find_map(|e| match e {
            Event::ChatStarted { conversation_id, partial_documents, .. } if *conversation_id == conv.id => Some(*partial_documents),
            _ => None,
        });
        assert_eq!(started, Some(true), "the document was cut down to what fits");
        // An image with no readable text is useless to a model that can't see.
        let blank = Attachment { name: "photo.jpg".into(), text: "  ".into(), image: Some("x.jpg".into()) };
        assert!(matches!(engine.check_attachments(&model.id, &[blank]), Err(EngineError::Document { .. })));

        engine.delete_model(&model.id).unwrap();
        assert!(!engine.state().installed.contains(&model.id));
        assert!(engine.state().imported.is_empty());
        assert!(engine.state().loaded.is_none());
    }

    #[test]
    fn importing_a_file_that_is_not_a_model_is_refused_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::new(dir.path(), None, "0.1.0", Arc::new(|_| {})).unwrap();
        let r = engine.import_model("notes.gguf", &b"this is not a model"[..], 19);
        assert!(matches!(r, Err(EngineError::BadModelFile { .. })));
        assert!(engine.state().imported.is_empty());
        assert_eq!(std::fs::read_dir(engine.store().models_dir()).unwrap().count(), 0, "no leftovers");
    }

    #[test]
    fn gpu_crash_marker_from_a_previous_run_turns_gpu_off() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("gpu-load.marker"), "/models/qwen3.5-4b.gguf\nAMD Radeon RX 7800 XT").unwrap();
        let engine = Engine::new(dir.path(), None, "0.1.0", Arc::new(|_| {})).unwrap();
        let s = engine.state().settings;
        assert!(!s.gpu_allowed());
        let crash = s.gpu_crash.unwrap();
        assert_eq!((crash.model_id.as_str(), crash.device.as_str()), ("qwen3.5-4b", "AMD Radeon RX 7800 XT"));
        assert!(!dir.path().join("gpu-load.marker").exists());
    }

    #[test]
    fn license_must_be_accepted_before_download() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::new(dir.path(), None, "0.1.0", Arc::new(|_| {})).unwrap();
        let custom = engine.state().models.into_iter().find(|m| m.license.requires_acceptance).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let r = rt.block_on(engine.download(&custom.id));
        assert!(matches!(r, Err(EngineError::LicenseNotAccepted { .. })));
    }

    /// The published catalog verifies with the built-in key (run with `--ignored`; needs internet).
    #[test]
    #[ignore = "network: fetches the live catalog from GitHub"]
    fn live_catalog_verifies_with_the_built_in_key() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::new(dir.path(), None, "0.1.0", Arc::new(|_| {})).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let r = rt.block_on(engine.refresh_catalog(true)).unwrap();
        assert!(r.checked);
        assert!(r.version >= engine.state().catalog_version);
        assert!(engine.store().settings().last_catalog_check.is_some());
        // Automatic checks right after are skipped (at most once a day).
        assert!(!rt.block_on(engine.refresh_catalog(false)).unwrap().checked);
    }

    #[test]
    fn errors_serialize_for_the_ui_without_clashing_tags() {
        let e = EngineError::Download { error: DownloadError::Offline };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"kind":"download","error":{"kind":"offline"}}"#
        );
    }

    #[test]
    fn guesses_chat_format_of_imported_files() {
        let p = guess_chat_profile("{% if enable_thinking %}<think>{% endif %}");
        assert_eq!(p.reasoning, ReasoningFormat::ThinkTags);
        assert!(matches!(p.thinking, ThinkingControl::TemplateFlag { .. }));
        assert_eq!(guess_chat_profile("{{ messages }}").reasoning, ReasoningFormat::None);
        assert_eq!(guess_chat_profile("<|channel|>analysis").reasoning, ReasoningFormat::Harmony);
    }
}
