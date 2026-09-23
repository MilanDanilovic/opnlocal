//! The model catalog: a small, signed list of models opnlocal knows how to run.
//!
//! A copy ships inside the app, so it works offline from the first launch. A newer copy may be
//! fetched from the public catalog repo. It is only used if its ed25519 signature verifies
//! against a key in [`TRUSTED_KEYS`] and its `version` is higher than what we already have.

use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/MilanDanilovic/opnlocal-catalog/main/catalog.json";
pub const SIGNATURE_URL: &str =
    "https://raw.githubusercontent.com/MilanDanilovic/opnlocal-catalog/main/catalog.json.sig";

/// Raw ed25519 public keys (base64) allowed to sign catalogs. The private key lives outside the repo.
pub const TRUSTED_KEYS: &[&str] = &["IGd0duRE2zwpKNtOsPayxo69nPoM4pMIfcXLkZ+U3d8="];

/// Only catalogs with this schema number are understood by this app version.
pub const SCHEMA: u32 = 1;

#[derive(Serialize, Deserialize, TS, Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UseCase {
    Everyday,
    Coding,
    Writing,
    Documents,
}

impl UseCase {
    pub const ALL: [UseCase; 4] = [Self::Everyday, Self::Coding, Self::Writing, Self::Documents];
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Catalog {
    pub schema: u32,
    /// Monotonic; a fetched catalog replaces the current one only if this is higher.
    #[ts(type = "number")]
    pub version: u64,
    /// ISO date, informational.
    pub published: String,
    pub models: Vec<CatalogModel>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct CatalogModel {
    /// Stable id; also the model's file name on disk.
    pub id: String,
    pub name: String,
    pub publisher: String,
    /// One or two plain-language sentences for non-technical people.
    pub summary: String,
    /// Relative quality per use case within this catalog, 1 (basic) to 5 (best). 0 = not suited.
    pub scores: Scores,
    pub license: License,
    pub file: ModelFile,
    pub memory: MemoryProfile,
    pub chat: ChatProfile,
    /// Oldest app version that can run this model (e.g. a newer architecture needs a newer llama.cpp).
    pub min_app_version: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[ts(export)]
pub struct Scores {
    pub everyday: u8,
    pub coding: u8,
    pub writing: u8,
    pub documents: u8,
}

impl Scores {
    pub fn get(&self, use_case: UseCase) -> u8 {
        match use_case {
            UseCase::Everyday => self.everyday,
            UseCase::Coding => self.coding,
            UseCase::Writing => self.writing,
            UseCase::Documents => self.documents,
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct License {
    /// SPDX id where one exists (e.g. "Apache-2.0"), otherwise a short slug.
    pub id: String,
    pub name: String,
    pub url: String,
    /// Custom terms the user must read and accept once before downloading.
    pub requires_acceptance: bool,
    /// Plain-language summary of the conditions, shown before acceptance.
    pub summary: Option<String>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ModelFile {
    /// Hugging Face repo, e.g. "unsloth/Qwen3.5-4B-GGUF".
    pub repo: String,
    /// Pinned 40-char commit.
    pub revision: String,
    pub path: String,
    #[ts(type = "number")]
    pub size: u64,
    pub sha256: String,
}

impl ModelFile {
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.path
        )
    }
}

/// Memory model used to decide whether a model fits (see `fit`). For catalog models these numbers
/// are measured by `catalog-tool verify` from llama.cpp's own buffer allocations.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[ts(export)]
pub struct MemoryProfile {
    /// Model weights as loaded (≈ file size).
    #[ts(type = "number")]
    pub weights_bytes: u64,
    /// KV cache cost per context token for full-attention layers (f16 cache).
    #[ts(type = "number")]
    pub kv_bytes_per_token: u64,
    /// KV cache per token for sliding-window layers; those only keep `swa_window` tokens.
    #[ts(type = "number")]
    pub swa_kv_bytes_per_token: u64,
    pub swa_window: u32,
    /// Context-independent state (recurrent layers etc.).
    #[ts(type = "number")]
    pub fixed_bytes: u64,
    /// Compute scratch buffer per token of micro-batch (dominated by vocab size).
    #[ts(type = "number")]
    pub compute_bytes_per_ubatch_token: u64,
    /// Longest context the model was trained for.
    pub max_context: u32,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ChatProfile {
    /// How the model marks its private reasoning in the output.
    pub reasoning: ReasoningFormat,
    /// How thinking is switched on and off through the chat template.
    pub thinking: ThinkingControl,
    /// Recommended sampling for normal replies.
    pub sampling: Sampling,
    /// Recommended sampling when thinking is on, if different.
    pub thinking_sampling: Option<Sampling>,
    /// Extra variables for the chat template (e.g. gpt-oss's `model_identity`).
    #[serde(default)]
    pub template_vars: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ReasoningFormat {
    /// No private reasoning in the output.
    None,
    /// `<think>…</think>` before the answer (Qwen, SmolLM, many others).
    ThinkTags,
    /// OpenAI harmony channels: `<|channel|>analysis<|message|>…<|end|>` then `final`.
    Harmony,
    /// Gemma 4: `<|channel>thought …<channel|>` before the answer.
    GemmaChannel,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum ThinkingControl {
    /// The model never thinks.
    Unsupported,
    /// A boolean template variable (usually `enable_thinking`).
    TemplateFlag { variable: String },
    /// Always reasons; a template variable sets the effort (gpt-oss: `reasoning_effort`).
    AlwaysOn {
        variable: String,
        normal: String,
        harder: String,
    },
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq)]
#[ts(export)]
pub struct Sampling {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub min_p: f32,
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum CatalogError {
    #[error("the catalog signature is missing or malformed")]
    BadSignatureEncoding,
    #[error("the catalog signature does not match a trusted key")]
    UntrustedSignature,
    #[error("the catalog is not valid JSON: {0}")]
    Parse(String),
    #[error("catalog schema {0} is not supported by this app version")]
    UnsupportedSchema(u32),
}

impl Catalog {
    /// Parses catalog bytes after checking their detached signature (base64 ed25519).
    pub fn parse_verified(bytes: &[u8], signature_b64: &str) -> Result<Catalog, CatalogError> {
        verify_signature(bytes, signature_b64, TRUSTED_KEYS)?;
        Self::parse_unverified(bytes)
    }

    /// Parses without a signature check. Only for the catalog built into the app binary
    /// (its integrity is the app's own).
    pub fn parse_unverified(bytes: &[u8]) -> Result<Catalog, CatalogError> {
        let catalog: Catalog =
            serde_json::from_slice(bytes).map_err(|e| CatalogError::Parse(e.to_string()))?;
        if catalog.schema != SCHEMA {
            return Err(CatalogError::UnsupportedSchema(catalog.schema));
        }
        Ok(catalog)
    }

    /// Models this app version can run.
    pub fn runnable(&self, app_version: &str) -> Vec<&CatalogModel> {
        self.models
            .iter()
            .filter(|m| version_at_least(app_version, &m.min_app_version))
            .collect()
    }

    pub fn model(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Picks the newest of the built-in and cached catalogs.
    pub fn newest(builtin: Catalog, cached: Option<Catalog>) -> Catalog {
        match cached {
            Some(c) if c.version > builtin.version => c,
            _ => builtin,
        }
    }
}

pub fn verify_signature(
    bytes: &[u8],
    signature_b64: &str,
    trusted_keys: &[&str],
) -> Result<(), CatalogError> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let sig_bytes = b64
        .decode(signature_b64.trim())
        .map_err(|_| CatalogError::BadSignatureEncoding)?;
    let signature =
        Signature::from_slice(&sig_bytes).map_err(|_| CatalogError::BadSignatureEncoding)?;
    let trusted = trusted_keys.iter().any(|key| {
        let Ok(raw) = b64.decode(key) else {
            return false;
        };
        let Ok(raw) = <[u8; 32]>::try_from(raw.as_slice()) else {
            return false;
        };
        VerifyingKey::from_bytes(&raw)
            .map(|k| k.verify_strict(bytes, &signature).is_ok())
            .unwrap_or(false)
    });
    if trusted {
        Ok(())
    } else {
        Err(CatalogError::UntrustedSignature)
    }
}

/// `a >= b` for dotted numeric versions ("0.10.2" >= "0.9"). Non-numeric parts count as 0.
pub fn version_at_least(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    true
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    pub fn test_model(id: &str, weights_gb: f64, scores: Scores) -> CatalogModel {
        CatalogModel {
            id: id.into(),
            name: id.into(),
            publisher: "Test".into(),
            summary: "A test model.".into(),
            scores,
            license: License {
                id: "Apache-2.0".into(),
                name: "Apache License 2.0".into(),
                url: "https://www.apache.org/licenses/LICENSE-2.0".into(),
                requires_acceptance: false,
                summary: None,
            },
            file: ModelFile {
                repo: "org/repo".into(),
                revision: "0".repeat(40),
                path: format!("{id}.gguf"),
                size: (weights_gb * 1e9) as u64,
                sha256: "0".repeat(64),
            },
            memory: MemoryProfile {
                weights_bytes: (weights_gb * 1e9) as u64,
                kv_bytes_per_token: 32 * 1024,
                max_context: 262_144,
                compute_bytes_per_ubatch_token: 1_000_000,
                ..Default::default()
            },
            chat: ChatProfile {
                reasoning: ReasoningFormat::ThinkTags,
                thinking: ThinkingControl::TemplateFlag {
                    variable: "enable_thinking".into(),
                },
                sampling: Sampling {
                    temperature: 0.7,
                    top_p: 0.8,
                    top_k: 20,
                    min_p: 0.0,
                },
                thinking_sampling: None,
                template_vars: Default::default(),
            },
            min_app_version: "0.1.0".into(),
        }
    }

    fn catalog_bytes(version: u64) -> Vec<u8> {
        serde_json::to_vec(&Catalog {
            schema: SCHEMA,
            version,
            published: "2026-09-24".into(),
            models: vec![test_model("a", 1.0, Scores::default())],
        })
        .unwrap()
    }

    fn keypair() -> (SigningKey, String) {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let public =
            base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
        (key, public)
    }

    #[test]
    fn accepts_catalog_signed_by_trusted_key() {
        let (key, public) = keypair();
        let bytes = catalog_bytes(2);
        let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(&bytes).to_bytes());
        assert_eq!(verify_signature(&bytes, &sig, &[&public]), Ok(()));
    }

    #[test]
    fn rejects_tampered_catalog() {
        let (key, public) = keypair();
        let bytes = catalog_bytes(2);
        let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(&bytes).to_bytes());
        let mut tampered = bytes.clone();
        tampered[10] ^= 1;
        assert_eq!(
            verify_signature(&tampered, &sig, &[&public]),
            Err(CatalogError::UntrustedSignature)
        );
    }

    #[test]
    fn rejects_signature_from_unknown_key() {
        let (_, public) = keypair();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let bytes = catalog_bytes(2);
        let sig = base64::engine::general_purpose::STANDARD.encode(other.sign(&bytes).to_bytes());
        assert_eq!(
            verify_signature(&bytes, &sig, &[&public]),
            Err(CatalogError::UntrustedSignature)
        );
        assert_eq!(
            verify_signature(&bytes, "not base64!", &[&public]),
            Err(CatalogError::BadSignatureEncoding)
        );
    }

    #[test]
    fn newer_cached_catalog_wins_older_does_not() {
        let builtin = Catalog::parse_unverified(&catalog_bytes(5)).unwrap();
        let older = Catalog::parse_unverified(&catalog_bytes(4)).unwrap();
        let newer = Catalog::parse_unverified(&catalog_bytes(6)).unwrap();
        assert_eq!(Catalog::newest(builtin.clone(), Some(older)).version, 5);
        assert_eq!(Catalog::newest(builtin, Some(newer)).version, 6);
    }

    #[test]
    fn unknown_schema_is_refused() {
        let mut value: serde_json::Value = serde_json::from_slice(&catalog_bytes(1)).unwrap();
        value["schema"] = 99.into();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            Catalog::parse_unverified(&bytes),
            Err(CatalogError::UnsupportedSchema(99))
        );
    }

    #[test]
    fn filters_models_needing_newer_app() {
        let mut catalog = Catalog::parse_unverified(&catalog_bytes(1)).unwrap();
        let mut future = test_model("future", 1.0, Scores::default());
        future.min_app_version = "0.3.0".into();
        catalog.models.push(future);
        let ids: Vec<_> = catalog.runnable("0.2.9").iter().map(|m| m.id.clone()).collect();
        assert_eq!(ids, vec!["a"]);
        assert_eq!(catalog.runnable("0.3.0").len(), 2);
    }

    #[test]
    fn version_comparison() {
        assert!(version_at_least("0.10.0", "0.9.9"));
        assert!(version_at_least("1.0", "1.0.0"));
        assert!(!version_at_least("0.1.0", "0.1.1"));
    }

    #[test]
    fn builtin_catalog_parses() {
        let catalog = Catalog::parse_unverified(crate::BUILTIN_CATALOG).unwrap();
        assert!(!catalog.models.is_empty());
        for m in &catalog.models {
            assert_eq!(m.file.revision.len(), 40, "{} revision must be pinned", m.id);
            assert_eq!(m.file.sha256.len(), 64, "{} needs a sha256", m.id);
            assert!(m.memory.weights_bytes > 0, "{} needs a memory profile", m.id);
            assert!(
                UseCase::ALL.iter().any(|u| m.scores.get(*u) > 0),
                "{} must suit at least one use",
                m.id
            );
        }
    }
}
