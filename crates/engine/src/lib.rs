//! opnlocal engine: everything except the UI.
//!
//! - [`hardware`]: what this device has (memory, processor, graphics, storage)
//! - [`catalog`]: the signed list of models we know how to run
//! - [`fit`] and [`recommend`]: which of them fit, and which to suggest
//! - [`download`]: resumable, verified downloads
//! - [`llm`]: llama.cpp runtime (in-process, one worker thread)
//! - [`chat`], [`bench`], [`docs`], [`ocr`], [`retrieval`]: prompts and replies, measuring, attached
//!   documents and images, the relevant parts of documents too long to read whole
//! - [`store`]: files on disk
//! - [`engine`]: the facade the app talks to

pub mod bench;
pub mod catalog;
pub mod chat;
pub mod docs;
pub mod download;
pub mod engine;
pub mod fit;
pub mod gguf;
pub mod hardware;
pub mod llm;
pub mod ocr;
pub mod recommend;
pub mod retrieval;
pub mod store;

pub use engine::{Engine, EngineError, Event};

/// The catalog compiled into the app, so it works offline from the first launch.
pub const BUILTIN_CATALOG: &[u8] = include_bytes!("../../../catalog/catalog.json");
