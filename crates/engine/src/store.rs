//! Everything opnlocal keeps on disk, in one folder:
//!
//! ```text
//! <root>/settings.json          user choices, accepted licenses
//! <root>/benchmarks.json        measured results per model
//! <root>/catalog.json(.sig)     newest verified catalog fetched online
//! <root>/imported.json          models the user imported from a file
//! <root>/models/<id>.gguf       model files (+ .part while downloading)
//! <root>/conversations/<id>.json
//! <root>/gpu-load.marker        exists only while a model loads on the GPU (crash guard)
//! ```
//!
//! Writes go to a temp file first and are renamed into place, so a crash never leaves a
//! half-written JSON file.

use crate::bench::BenchmarkResult;
use crate::catalog::{MemoryProfile, UseCase};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default)]
#[ts(export)]
pub struct Settings {
    pub onboarding_done: bool,
    pub use_case: Option<UseCase>,
    /// Model used for new chats.
    pub active_model: Option<String>,
    /// Refresh the model list at most once a day. Off = never goes online on its own.
    pub auto_refresh_catalog: bool,
    /// Unix seconds of the last catalog check.
    #[ts(type = "number | null")]
    pub last_catalog_check: Option<u64>,
    /// License id → unix seconds when accepted.
    #[ts(type = "Record<string, number>")]
    pub accepted_licenses: BTreeMap<String, u64>,
    /// User turned graphics acceleration off in Advanced.
    pub gpu_disabled: bool,
    /// Set when the app died while loading a model on the GPU; GPU stays off until re-enabled.
    pub gpu_crash: Option<GpuCrash>,
    pub per_model: BTreeMap<String, ModelSettings>,
}

impl Settings {
    pub fn new() -> Self {
        Settings {
            auto_refresh_catalog: true,
            ..Default::default()
        }
    }

    pub fn gpu_allowed(&self) -> bool {
        !self.gpu_disabled && self.gpu_crash.is_none()
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct GpuCrash {
    pub model_id: String,
    pub device: String,
    #[ts(type = "number")]
    pub at: u64,
}

/// Advanced per-model overrides. None = use the recommended value.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default)]
#[ts(export)]
pub struct ModelSettings {
    pub context: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_reply_tokens: Option<u32>,
    pub system_prompt: Option<String>,
    /// Layers on the GPU; None = automatic, 0 = processor only.
    pub gpu_layers: Option<u32>,
    pub threads: Option<u32>,
    /// Set by the benchmark on machines whose only GPU is integrated: it measured faster.
    pub use_integrated_gpu: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ImportedModel {
    pub id: String,
    pub name: String,
    /// File name inside models/.
    pub file: String,
    #[ts(type = "number")]
    pub size: u64,
    pub memory: MemoryProfile,
    #[ts(type = "number")]
    pub imported_at: u64,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Attachment {
    pub name: String,
    /// Extracted text; sent to the model with the message.
    pub text: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ReplyStats {
    pub tokens: u32,
    pub seconds: f32,
    pub words: u32,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: String,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[ts(type = "number")]
    pub created_at: u64,
    #[serde(default)]
    pub stats: Option<ReplyStats>,
    /// The reply was stopped by the user before it finished.
    #[serde(default)]
    pub stopped: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub model_id: String,
    pub use_case: UseCase,
    #[serde(default)]
    pub think_harder: bool,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number")]
    pub updated_at: u64,
    pub messages: Vec<Message>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub model_id: String,
    #[ts(type = "number")]
    pub updated_at: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Ids become file names; allow only safe characters.
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.starts_with('.')
}

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<Store> {
        let store = Store { root: root.into() };
        std::fs::create_dir_all(store.models_dir())?;
        std::fs::create_dir_all(store.root.join("conversations"))?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    pub fn model_path(&self, id: &str) -> PathBuf {
        self.models_dir().join(format!("{id}.gguf"))
    }

    pub fn is_installed(&self, id: &str) -> bool {
        safe_id(id) && self.model_path(id).is_file()
    }

    pub fn gpu_marker(&self) -> PathBuf {
        self.root.join("gpu-load.marker")
    }

    pub fn catalog_cache(&self) -> (PathBuf, PathBuf) {
        (
            self.root.join("catalog.json"),
            self.root.join("catalog.json.sig"),
        )
    }

    pub fn settings(&self) -> Settings {
        read_json(&self.root.join("settings.json")).unwrap_or_else(Settings::new)
    }

    pub fn save_settings(&self, s: &Settings) -> std::io::Result<()> {
        write_json(&self.root.join("settings.json"), s)
    }

    pub fn benchmarks(&self) -> BTreeMap<String, BenchmarkResult> {
        read_json(&self.root.join("benchmarks.json")).unwrap_or_default()
    }

    pub fn save_benchmark(&self, model_id: &str, r: &BenchmarkResult) -> std::io::Result<()> {
        let mut all = self.benchmarks();
        all.insert(model_id.to_string(), r.clone());
        write_json(&self.root.join("benchmarks.json"), &all)
    }

    pub fn imported(&self) -> Vec<ImportedModel> {
        read_json(&self.root.join("imported.json")).unwrap_or_default()
    }

    pub fn save_imported(&self, list: &[ImportedModel]) -> std::io::Result<()> {
        write_json(&self.root.join("imported.json"), &list)
    }

    /// Deletes a model file (and any partial download, benchmark and settings for it).
    pub fn delete_model(&self, id: &str) -> std::io::Result<()> {
        if !safe_id(id) {
            return Ok(());
        }
        let path = self.model_path(id);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        crate::download::discard_partial(&path);
        let mut b = self.benchmarks();
        if b.remove(id).is_some() {
            write_json(&self.root.join("benchmarks.json"), &b)?;
        }
        let list: Vec<_> = self.imported().into_iter().filter(|m| m.id != id).collect();
        self.save_imported(&list)?;
        let mut s = self.settings();
        s.per_model.remove(id);
        if s.active_model.as_deref() == Some(id) {
            s.active_model = None;
        }
        self.save_settings(&s)
    }

    fn conversation_path(&self, id: &str) -> Option<PathBuf> {
        safe_id(id).then(|| self.root.join("conversations").join(format!("{id}.json")))
    }

    pub fn conversation(&self, id: &str) -> Option<Conversation> {
        read_json(&self.conversation_path(id)?)
    }

    pub fn save_conversation(&self, c: &Conversation) -> std::io::Result<()> {
        let path = self
            .conversation_path(&c.id)
            .ok_or_else(|| std::io::Error::other("invalid conversation id"))?;
        write_json(&path, c)
    }

    pub fn delete_conversation(&self, id: &str) -> std::io::Result<()> {
        match self.conversation_path(id) {
            Some(p) if p.exists() => std::fs::remove_file(p),
            _ => Ok(()),
        }
    }

    pub fn delete_all_conversations(&self) -> std::io::Result<()> {
        for s in self.conversations() {
            self.delete_conversation(&s.id)?;
        }
        Ok(())
    }

    /// Newest first.
    pub fn conversations(&self) -> Vec<ConversationSummary> {
        let Ok(entries) = std::fs::read_dir(self.root.join("conversations")) else {
            return vec![];
        };
        let mut list: Vec<ConversationSummary> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| read_json::<Conversation>(&e.path()))
            .map(|c| ConversationSummary {
                id: c.id,
                title: c.title,
                model_id: c.model_id,
                updated_at: c.updated_at,
            })
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
        list
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub(crate) fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    fn conversation(id: &str, updated_at: u64) -> Conversation {
        Conversation {
            id: id.into(),
            title: format!("Chat {id}"),
            model_id: "m".into(),
            use_case: UseCase::Everyday,
            think_harder: false,
            created_at: 1,
            updated_at,
            messages: vec![Message {
                id: new_id(),
                role: Role::User,
                content: "Hello".into(),
                thinking: None,
                attachments: vec![],
                created_at: 1,
                stats: None,
                stopped: false,
            }],
        }
    }

    #[test]
    fn settings_default_and_roundtrip() {
        let (_d, s) = store();
        let mut settings = s.settings();
        assert!(settings.auto_refresh_catalog, "refresh defaults on");
        assert!(settings.gpu_allowed());
        settings.use_case = Some(UseCase::Coding);
        settings.accepted_licenses.insert("lfm1.0".into(), 42);
        s.save_settings(&settings).unwrap();
        assert_eq!(s.settings(), settings);
    }

    #[test]
    fn conversations_are_listed_newest_first_and_deletable() {
        let (_d, s) = store();
        s.save_conversation(&conversation("a", 10)).unwrap();
        s.save_conversation(&conversation("b", 20)).unwrap();
        let ids: Vec<_> = s.conversations().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["b", "a"]);
        assert_eq!(s.conversation("a").unwrap().messages[0].content, "Hello");
        s.delete_conversation("a").unwrap();
        assert_eq!(s.conversations().len(), 1);
        s.delete_all_conversations().unwrap();
        assert!(s.conversations().is_empty());
    }

    #[test]
    fn unsafe_ids_never_touch_the_filesystem() {
        let (_d, s) = store();
        assert!(s.conversation("../settings").is_none());
        assert!(s.save_conversation(&conversation("../../evil", 1)).is_err());
        assert!(!s.is_installed("../x"));
    }

    #[test]
    fn deleting_a_model_clears_its_traces() {
        let (_d, s) = store();
        std::fs::write(s.model_path("m1"), b"gguf").unwrap();
        let mut settings = s.settings();
        settings.active_model = Some("m1".into());
        settings.per_model.insert("m1".into(), ModelSettings::default());
        s.save_settings(&settings).unwrap();
        assert!(s.is_installed("m1"));
        s.delete_model("m1").unwrap();
        assert!(!s.is_installed("m1"));
        let settings = s.settings();
        assert_eq!(settings.active_model, None);
        assert!(settings.per_model.is_empty());
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults() {
        let (d, s) = store();
        std::fs::write(d.path().join("settings.json"), b"{not json").unwrap();
        assert!(s.settings().auto_refresh_catalog);
    }
}
