//! Tauri adapter: each command forwards to one `Engine` method; engine events are emitted to the
//! web view as `engine` events. There is no local HTTP server: the UI talks to the engine only
//! through Tauri's in-process IPC.

use opnlocal_engine::bench::BenchmarkResult;
use opnlocal_engine::catalog::UseCase;
use opnlocal_engine::engine::{AppState, CatalogRefresh};
use opnlocal_engine::hardware::DeviceInfo;
use opnlocal_engine::recommend::Recommendations;
use opnlocal_engine::store::{Attachment, Conversation, ConversationSummary, ImportedModel, Settings};
use opnlocal_engine::{Engine, EngineError};
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

mod keep_awake;

type Eng<'a> = State<'a, Arc<Engine>>;
type Res<T> = Result<T, EngineError>;

/// Runs blocking engine work (loading, generating, reading files) off the UI thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.unwrap_or_else(|e| {
        Err(EngineError::Io {
            message: format!("background task failed: {e}"),
        })
    })
}

#[tauri::command]
async fn get_state(engine: Eng<'_>) -> Res<AppState> {
    Ok(engine.state())
}

#[tauri::command]
async fn detect_device(engine: Eng<'_>) -> Res<DeviceInfo> {
    let e = engine.inner().clone();
    blocking(move || Ok(e.detect())).await
}

#[tauri::command]
async fn recommend(engine: Eng<'_>, use_case: UseCase) -> Res<Recommendations> {
    let e = engine.inner().clone();
    blocking(move || Ok(e.recommend(use_case))).await
}

#[tauri::command]
async fn save_settings(engine: Eng<'_>, settings: Settings) -> Res<Settings> {
    engine.update_settings(|s| *s = settings)
}

#[tauri::command]
async fn accept_license(engine: Eng<'_>, license_id: String) -> Res<Settings> {
    engine.accept_license(&license_id)
}

#[tauri::command]
async fn refresh_catalog(engine: Eng<'_>, force: bool) -> Res<CatalogRefresh> {
    engine.refresh_catalog(force).await
}

#[tauri::command]
async fn download_model(app: tauri::AppHandle, engine: Eng<'_>, model_id: String) -> Res<()> {
    let _awake = keep_awake::Awake::new(&app);
    engine.download(&model_id).await
}

#[tauri::command]
async fn cancel_download(engine: Eng<'_>) -> Res<()> {
    engine.cancel_download();
    Ok(())
}

#[tauri::command]
async fn delete_model(engine: Eng<'_>, model_id: String) -> Res<()> {
    let e = engine.inner().clone();
    blocking(move || e.delete_model(&model_id)).await
}

#[tauri::command]
async fn run_benchmark(app: tauri::AppHandle, engine: Eng<'_>, model_id: String) -> Res<BenchmarkResult> {
    let _awake = keep_awake::Awake::new(&app);
    let e = engine.inner().clone();
    blocking(move || e.benchmark(&model_id)).await
}

#[tauri::command]
async fn send_message(
    app: tauri::AppHandle,
    engine: Eng<'_>,
    conversation_id: Option<String>,
    text: String,
    attachments: Vec<Attachment>,
    think_harder: bool,
) -> Res<Conversation> {
    let _awake = keep_awake::Awake::new(&app);
    let e = engine.inner().clone();
    blocking(move || e.send_message(conversation_id.as_deref(), &text, attachments, think_harder)).await
}

#[tauri::command]
async fn regenerate(app: tauri::AppHandle, engine: Eng<'_>, conversation_id: String, think_harder: bool) -> Res<Conversation> {
    let _awake = keep_awake::Awake::new(&app);
    let e = engine.inner().clone();
    blocking(move || e.regenerate(&conversation_id, think_harder)).await
}

#[tauri::command]
async fn stop_generation(engine: Eng<'_>) -> Res<()> {
    engine.stop_generation();
    Ok(())
}

#[tauri::command]
async fn list_conversations(engine: Eng<'_>) -> Res<Vec<ConversationSummary>> {
    Ok(engine.conversations())
}

#[tauri::command]
async fn get_conversation(engine: Eng<'_>, id: String) -> Res<Conversation> {
    engine.conversation(&id)
}

#[tauri::command]
async fn delete_conversation(engine: Eng<'_>, id: String) -> Res<()> {
    engine.delete_conversation(&id)
}

#[tauri::command]
async fn delete_all_conversations(engine: Eng<'_>) -> Res<()> {
    engine.delete_all_conversations()
}

#[tauri::command]
async fn set_conversation_model(engine: Eng<'_>, id: String, model_id: String) -> Res<Conversation> {
    engine.set_conversation_model(&id, &model_id)
}

#[tauri::command]
async fn unload_model(engine: Eng<'_>) -> Res<()> {
    let e = engine.inner().clone();
    blocking(move || {
        e.unload();
        Ok(())
    })
    .await
}

/// Opens a user-picked file. Paths on desktop, `content://` URIs on Android, security-scoped
/// URLs on iOS: the fs plugin handles all three.
fn open_picked(app: &tauri::AppHandle, path: &str) -> Res<(String, std::fs::File)> {
    use tauri_plugin_fs::FsExt;
    let file_path: tauri_plugin_fs::FilePath = path.parse().expect("infallible");
    let name = match &file_path {
        tauri_plugin_fs::FilePath::Path(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()),
        tauri_plugin_fs::FilePath::Url(u) => u
            .path_segments()
            .and_then(|mut s| s.next_back())
            .map(percent_decode),
    }
    .unwrap_or_else(|| "document".into());
    // OpenOptions' fields are private; its serde defaults give read-only access.
    let opts: tauri_plugin_fs::OpenOptions = serde_json::from_str("{}").expect("static options");
    let file = app.fs().open(file_path, opts).map_err(|e| EngineError::Io { message: e.to_string() })?;
    Ok((name, file))
}

/// Android content URIs end in an encoded display name ("primary%3ADocuments%2Freport.pdf").
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8_lossy(&out).into_owned();
    decoded.rsplit(['/', ':']).next().unwrap_or(&decoded).to_string()
}

#[tauri::command]
async fn attach_file(app: tauri::AppHandle, engine: Eng<'_>, path: String) -> Res<Attachment> {
    let e = engine.inner().clone();
    blocking(move || {
        use std::io::Read;
        let (name, mut file) = open_picked(&app, &path)?;
        let mut bytes = Vec::new();
        file.by_ref()
            .take(opnlocal_engine::docs::MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        e.extract_document(&name, &bytes)
    })
    .await
}

#[tauri::command]
async fn import_model(app: tauri::AppHandle, engine: Eng<'_>, path: String) -> Res<ImportedModel> {
    let e = engine.inner().clone();
    blocking(move || {
        let (name, file) = open_picked(&app, &path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        e.import_model(&name, std::io::BufReader::new(file), size)
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(keep_awake::plugin())
        .setup(|app| {
            // OPNLOCAL_DATA_DIR: used by automated tests of the real app to start from a clean folder.
            let data_dir = match std::env::var_os("OPNLOCAL_DATA_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            // Desktop bundles ship the llama.cpp backends as resources next to the app.
            let lib_dir = app.path().resource_dir().ok();
            let handle = app.handle().clone();
            let events: opnlocal_engine::engine::EventSink = Arc::new(move |event| {
                let _ = handle.emit("engine", event);
            });
            let engine = Engine::new(
                &data_dir,
                lib_dir.as_deref(),
                &app.package_info().version.to_string(),
                events,
            )
            .map_err(|e| e.to_string())?;
            let engine = Arc::new(engine);
            app.manage(engine.clone());
            if let Ok(model) = std::env::var("OPNLOCAL_SELFTEST") {
                self_test(engine.clone(), model);
            }
            // Daily model-list check, only if the user allows it (the engine decides).
            tauri::async_runtime::spawn(async move {
                let _ = engine.refresh_catalog(false).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            detect_device,
            recommend,
            save_settings,
            accept_license,
            refresh_catalog,
            download_model,
            cancel_download,
            delete_model,
            run_benchmark,
            send_message,
            regenerate,
            stop_generation,
            list_conversations,
            get_conversation,
            delete_conversation,
            delete_all_conversations,
            set_conversation_model,
            unload_model,
            attach_file,
            import_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running opnlocal");
}

/// CI smoke test for platforms we can only run in CI (iOS simulator, macOS, Linux): reports the
/// detected device and, if `model` is a GGUF path, a short real generation. Lines start with
/// `OPNLOCAL_SELFTEST`; they go to stdout and to `selftest.log` in the data folder (iOS apps'
/// stdout isn't always captured).
fn self_test(engine: Arc<Engine>, model: String) {
    use opnlocal_engine::llm::{GenerateRequest, GpuChoice, LoadOptions, Runtime, SamplingParams};
    let log_path = engine.store().root().join("selftest.log");
    let log = move |line: String| {
        use std::io::Write;
        println!("{line}");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = writeln!(f, "{line}");
        }
    };
    std::thread::spawn(move || {
        let device = engine.detect();
        log(format!("OPNLOCAL_SELFTEST device {}", serde_json::to_string(&device).unwrap_or_default()));
        let path = std::path::PathBuf::from(&model);
        if !path.is_file() {
            log("OPNLOCAL_SELFTEST no-model".into());
            return;
        }
        let gpu = if device.gpus.is_empty() { GpuChoice::None } else { GpuChoice::All };
        let rt = Runtime::start(None);
        let options = LoadOptions { path, context: 512, ubatch: 128, gpu, threads: None, crash_marker: None };
        match rt.load(options, |_| {}) {
            Ok((info, _)) => log(format!("OPNLOCAL_SELFTEST loaded gpu_layers={} load_ms={}", info.gpu_layers, info.load_ms)),
            Err(e) => return log(format!("OPNLOCAL_SELFTEST FAILED load: {e}")),
        }
        let text = Arc::new(std::sync::Mutex::new(String::new()));
        let t2 = text.clone();
        let req = GenerateRequest {
            prompt: "<s>Once upon a time".into(),
            max_tokens: 32,
            sampling: SamplingParams { temperature: 0.0, top_p: 1.0, top_k: 1, min_p: 0.0, seed: 1 },
            stop: Default::default(),
        };
        match rt.generate(req, move |p| t2.lock().unwrap().push_str(p)) {
            Ok(s) => log(format!(
                "OPNLOCAL_SELFTEST OK tokens={} tokens_per_second={:.1} text={:?}",
                s.generated_tokens,
                s.generation_tokens_per_second().unwrap_or(0.0),
                text.lock().unwrap()
            )),
            Err(e) => log(format!("OPNLOCAL_SELFTEST FAILED generate: {e}")),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn android_content_uri_names() {
        assert_eq!(percent_decode("primary%3ADocuments%2Freport.pdf"), "report.pdf");
        assert_eq!(percent_decode("notes.txt"), "notes.txt");
        assert_eq!(percent_decode("My%20Letter.docx"), "My Letter.docx");
    }
}
