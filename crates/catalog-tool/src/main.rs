//! Maintainer tool for the model catalog (never shipped in the app).
//!
//!   catalog-tool inspect <repo> <revision> <file>   print a remote GGUF's metadata
//!   catalog-tool build                              catalog/source.json → catalog/catalog.json
//!   catalog-tool sign <private-key.pem>             write catalog/catalog.json.sig
//!   catalog-tool verify <models-dir> [ids…]         download, load and chat with each model
//!
//! `build` fills in what must not be typed by hand: file sizes and sha256 (from the Hugging Face
//! API at the pinned commit) and memory profiles (from each file's GGUF header).

mod verify;

use base64::Engine as _;
use opnlocal_engine::catalog::{Catalog, CatalogModel, MemoryProfile, ModelFile};
use opnlocal_engine::gguf;
use serde::Deserialize;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("inspect") if args.len() == 4 => inspect(&args[1], &args[2], &args[3]).await,
        Some("build") => build().await,
        Some("sign") if args.len() == 2 => sign(Path::new(&args[1])),
        Some("verify") if args.len() >= 2 => verify::run(Path::new(&args[1]), &args[2..]).await,
        _ => Err("usage: catalog-tool inspect <repo> <rev> <file> | build | sign <key.pem> | verify <dir> [ids…]".into()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Reads a GGUF header over HTTP with growing Range requests (headers are ~2–16 MB).
pub async fn remote_header(client: &reqwest::Client, url: &str) -> Res<gguf::Header> {
    let mut len: u64 = 8 * 1024 * 1024;
    loop {
        let bytes = client
            .get(url)
            .header("Range", format!("bytes=0-{}", len - 1))
            .send()
            .await
            .map_err(err)?
            .error_for_status()
            .map_err(err)?
            .bytes()
            .await
            .map_err(err)?;
        match gguf::read(bytes.as_ref()) {
            Ok(h) => return Ok(h),
            Err(gguf::GgufError::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof && len < 128 << 20 => {
                len *= 2;
            }
            Err(e) => return Err(format!("{url}: {e}")),
        }
    }
}

async fn inspect(repo: &str, rev: &str, file: &str) -> Res<()> {
    let client = reqwest::Client::new();
    let url = format!("https://huggingface.co/{repo}/resolve/{rev}/{file}");
    let h = remote_header(&client, &url).await?;
    for (k, v) in &h.metadata {
        if k == "tokenizer.chat_template" {
            continue;
        }
        println!("{k} = {v:?}");
    }
    println!("--- chat template ---\n{}", h.str("tokenizer.chat_template").unwrap_or("(none)"));
    Ok(())
}

#[derive(Deserialize)]
struct Source {
    schema: u32,
    version: u64,
    published: String,
    models: Vec<SourceModel>,
}

#[derive(Deserialize)]
struct SourceModel {
    #[serde(flatten)]
    model: serde_json::Value,
    file: SourceFile,
    /// Hand-checked corrections for layouts the GGUF metadata doesn't describe.
    #[serde(default)]
    memory_overrides: Option<MemoryOverrides>,
}

#[derive(Deserialize)]
struct SourceFile {
    repo: String,
    revision: String,
    path: String,
}

#[derive(Deserialize, Default)]
struct MemoryOverrides {
    kv_bytes_per_token: Option<u64>,
    swa_kv_bytes_per_token: Option<u64>,
    swa_window: Option<u32>,
    fixed_bytes: Option<u64>,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    size: u64,
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    oid: String,
}

async fn build() -> Res<()> {
    let src_path = root().join("catalog/source.json");
    let source: Source = serde_json::from_slice(&std::fs::read(&src_path).map_err(err)?).map_err(err)?;
    let client = reqwest::Client::new();
    let mut models = Vec::new();
    for sm in source.models {
        let f = &sm.file;
        let dir = Path::new(&f.path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let tree_url = format!("https://huggingface.co/api/models/{}/tree/{}/{}", f.repo, f.revision, dir);
        let tree: Vec<TreeEntry> = client.get(&tree_url).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
        let entry = tree.into_iter().find(|e| e.path == f.path).ok_or(format!("{} not found in {}", f.path, f.repo))?;
        let sha256 = entry.lfs.ok_or(format!("{} is not an LFS file", f.path))?.oid;
        let file = ModelFile { repo: f.repo.clone(), revision: f.revision.clone(), path: f.path.clone(), size: entry.size, sha256 };
        let header = remote_header(&client, &file.url()).await?;
        let mut memory: MemoryProfile = header.memory_profile(entry.size);
        if let Some(o) = &sm.memory_overrides {
            memory.kv_bytes_per_token = o.kv_bytes_per_token.unwrap_or(memory.kv_bytes_per_token);
            memory.swa_kv_bytes_per_token = o.swa_kv_bytes_per_token.unwrap_or(memory.swa_kv_bytes_per_token);
            memory.swa_window = o.swa_window.unwrap_or(memory.swa_window);
            memory.fixed_bytes = o.fixed_bytes.unwrap_or(memory.fixed_bytes);
        }
        let mut value = sm.model;
        value["file"] = serde_json::to_value(&file).map_err(err)?;
        value["memory"] = serde_json::to_value(memory).map_err(err)?;
        let model: CatalogModel = serde_json::from_value(value).map_err(|e| format!("{}: {e}", f.path))?;
        println!(
            "{:<20} {:>6.2} GB  kv {:>4} KiB/tok  swa {:>4} KiB/tok (window {})  arch {}",
            model.id,
            model.file.size as f64 / 1e9,
            model.memory.kv_bytes_per_token / 1024,
            model.memory.swa_kv_bytes_per_token / 1024,
            model.memory.swa_window,
            header.architecture().unwrap_or("?")
        );
        models.push(model);
    }
    let catalog = Catalog { schema: source.schema, version: source.version, published: source.published, models };
    let out = root().join("catalog/catalog.json");
    std::fs::write(&out, serde_json::to_vec_pretty(&catalog).map_err(err)?).map_err(err)?;
    println!("wrote {} (version {}); now run `sign`", out.display(), catalog.version);
    Ok(())
}

fn sign(key: &Path) -> Res<()> {
    use ed25519_dalek::Signer;
    use ed25519_dalek::pkcs8::DecodePrivateKey;
    let pem = std::fs::read_to_string(key).map_err(err)?;
    let signing = ed25519_dalek::SigningKey::from_pkcs8_pem(&pem).map_err(err)?;
    let public = base64::engine::general_purpose::STANDARD.encode(signing.verifying_key().to_bytes());
    if !opnlocal_engine::catalog::TRUSTED_KEYS.contains(&public.as_str()) {
        return Err(format!("key {public} is not in TRUSTED_KEYS; the app would reject this signature"));
    }
    let path = root().join("catalog/catalog.json");
    let bytes = std::fs::read(&path).map_err(err)?;
    let sig = base64::engine::general_purpose::STANDARD.encode(signing.sign(&bytes).to_bytes());
    std::fs::write(root().join("catalog/catalog.json.sig"), &sig).map_err(err)?;
    Catalog::parse_verified(&bytes, &sig).map_err(err)?;
    println!("signed {} with {public}", path.display());
    Ok(())
}
