//! Minimal GGUF header reader: the metadata key/values only, never tensor data.
//!
//! Used for imported model files (name, chat template, memory estimate) and by the catalog tool.
//! Format: https://github.com/ggml-org/ggml/blob/master/docs/gguf.md

use crate::catalog::MemoryProfile;
use std::collections::BTreeMap;
use std::io::{self, BufReader, Read};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    Str(String),
    /// Numeric arrays are kept (per-layer values); other arrays only keep their length.
    Array(Vec<Value>),
    ArrayLen(u64),
}

impl Value {
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::UInt(v) => Some(*v),
            Value::Int(v) if *v >= 0 => Some(*v as u64),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Header {
    pub version: u32,
    pub tensor_count: u64,
    pub metadata: BTreeMap<String, Value>,
}

#[derive(thiserror::Error, Debug)]
pub enum GgufError {
    #[error("this is not a GGUF model file")]
    NotGguf,
    #[error("unsupported GGUF version {0}")]
    Version(u32),
    #[error("the file header is damaged or incomplete")]
    Corrupt,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Numeric arrays longer than this are summarized as a length (vocab scores, merges...).
const MAX_KEPT_ARRAY: u64 = 4096;
/// Refuse absurd lengths from damaged files instead of allocating them.
const MAX_STRING: u64 = 64 * 1024 * 1024;

pub fn read_file(path: &Path) -> Result<Header, GgufError> {
    read(BufReader::new(std::fs::File::open(path)?))
}

pub fn read(mut r: impl Read) -> Result<Header, GgufError> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if &magic != b"GGUF" {
        return Err(GgufError::NotGguf);
    }
    let version = u32_le(&mut r)?;
    if !(2..=3).contains(&version) {
        return Err(GgufError::Version(version));
    }
    let tensor_count = u64_le(&mut r)?;
    let kv_count = u64_le(&mut r)?;
    let mut metadata = BTreeMap::new();
    for _ in 0..kv_count {
        let key = string(&mut r)?;
        let ty = u32_le(&mut r)?;
        metadata.insert(key, value(&mut r, ty)?);
    }
    Ok(Header {
        version,
        tensor_count,
        metadata,
    })
}

fn value(r: &mut impl Read, ty: u32) -> Result<Value, GgufError> {
    Ok(match ty {
        0 => Value::UInt(read_n::<1>(r)?[0] as u64),
        1 => Value::Int(read_n::<1>(r)?[0] as i8 as i64),
        2 => Value::UInt(u16::from_le_bytes(read_n(r)?) as u64),
        3 => Value::Int(i16::from_le_bytes(read_n(r)?) as i64),
        4 => Value::UInt(u32_le(r)? as u64),
        5 => Value::Int(i32::from_le_bytes(read_n(r)?) as i64),
        6 => Value::Float(f32::from_le_bytes(read_n(r)?) as f64),
        7 => Value::Bool(read_n::<1>(r)?[0] != 0),
        8 => Value::Str(string(r)?),
        9 => {
            let item_ty = u32_le(r)?;
            let len = u64_le(r)?;
            let numeric = item_ty != 8 && item_ty != 9;
            if numeric && len <= MAX_KEPT_ARRAY {
                let mut items = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    items.push(value(r, item_ty)?);
                }
                Value::Array(items)
            } else {
                for _ in 0..len {
                    skip_value(r, item_ty)?;
                }
                Value::ArrayLen(len)
            }
        }
        10 => Value::UInt(u64_le(r)?),
        11 => Value::Int(i64::from_le_bytes(read_n(r)?)),
        12 => Value::Float(f64::from_le_bytes(read_n(r)?)),
        _ => return Err(GgufError::Corrupt),
    })
}

fn skip_value(r: &mut impl Read, ty: u32) -> Result<(), GgufError> {
    let n = match ty {
        0 | 1 | 7 => 1,
        2 | 3 => 2,
        4 | 5 | 6 => 4,
        10..=12 => 8,
        8 => {
            let len = u64_le(r)?;
            if len > MAX_STRING {
                return Err(GgufError::Corrupt);
            }
            len
        }
        9 => {
            let item_ty = u32_le(r)?;
            let len = u64_le(r)?;
            for _ in 0..len {
                skip_value(r, item_ty)?;
            }
            return Ok(());
        }
        _ => return Err(GgufError::Corrupt),
    };
    io::copy(&mut r.take(n), &mut io::sink())?;
    Ok(())
}

fn read_n<const N: usize>(r: &mut impl Read) -> Result<[u8; N], GgufError> {
    let mut b = [0u8; N];
    r.read_exact(&mut b)?;
    Ok(b)
}
fn u32_le(r: &mut impl Read) -> Result<u32, GgufError> {
    Ok(u32::from_le_bytes(read_n(r)?))
}
fn u64_le(r: &mut impl Read) -> Result<u64, GgufError> {
    Ok(u64::from_le_bytes(read_n(r)?))
}
fn string(r: &mut impl Read) -> Result<String, GgufError> {
    let len = u64_le(r)?;
    if len > MAX_STRING {
        return Err(GgufError::Corrupt);
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

impl Header {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn u64(&self, key: &str) -> Option<u64> {
        self.get(key)?.as_u64()
    }

    pub fn architecture(&self) -> Option<&str> {
        self.str("general.architecture")
    }

    fn arch_u64(&self, key: &str) -> Option<u64> {
        self.u64(&format!("{}.{key}", self.architecture()?))
    }

    /// A per-layer value that may be stored as a scalar or an array.
    fn per_layer(&self, key: &str, n_layers: u64) -> Vec<u64> {
        let Some(arch) = self.architecture() else {
            return vec![];
        };
        match self.get(&format!("{arch}.{key}")) {
            Some(Value::Array(items)) => items.iter().map(|v| v.as_u64().unwrap_or(0)).collect(),
            Some(v) => vec![v.as_u64().unwrap_or(0); n_layers as usize],
            None => vec![],
        }
    }

    /// Conservative memory estimate from metadata alone (imported models).
    ///
    /// Assumes an f16 KV cache on every attention layer; hybrid/recurrent layouts are
    /// recognized where the metadata says so (`full_attention_interval`, zero KV heads).
    /// Sliding-window layers are counted as full layers when their pattern is unknown, which
    /// overestimates rather than underestimates.
    pub fn memory_profile(&self, file_size: u64) -> MemoryProfile {
        let n_layers = self.arch_u64("block_count").unwrap_or(0);
        let n_embd = self.arch_u64("embedding_length").unwrap_or(0);
        let heads = self.per_layer("attention.head_count", n_layers);
        let n_head = heads.iter().copied().max().unwrap_or(1).max(1);
        let head_dim = n_embd / n_head;
        let k_len = self.arch_u64("attention.key_length").unwrap_or(head_dim);
        let v_len = self.arch_u64("attention.value_length").unwrap_or(head_dim);
        let mut kv_heads = self.per_layer("attention.head_count_kv", n_layers);
        if kv_heads.is_empty() {
            kv_heads = vec![n_head; n_layers as usize];
        }
        // Hybrid models (e.g. Qwen3.5): only every Nth layer has attention; the rest are recurrent.
        if let Some(interval) = self.arch_u64("full_attention_interval").filter(|i| *i > 1) {
            for (i, h) in kv_heads.iter_mut().enumerate() {
                if (i as u64 + 1) % interval != 0 {
                    *h = 0;
                }
            }
        }
        let kv_per_token: u64 = kv_heads.iter().map(|h| h * (k_len + v_len) * 2).sum();
        let n_vocab = match self.get("tokenizer.ggml.tokens") {
            Some(Value::ArrayLen(n)) => *n,
            Some(Value::Array(a)) => a.len() as u64,
            _ => self.arch_u64("vocab_size").unwrap_or(150_000),
        };
        MemoryProfile {
            weights_bytes: file_size,
            kv_bytes_per_token: kv_per_token,
            swa_kv_bytes_per_token: 0,
            swa_window: 0,
            // Recurrent state is small but unknown here; budget 64 MiB to be safe.
            fixed_bytes: 64 * 1024 * 1024,
            // Logits dominate: vocab × 4 bytes per micro-batch token, plus activations.
            compute_bytes_per_ubatch_token: n_vocab * 4 + n_embd * 16,
            max_context: self.arch_u64("context_length").unwrap_or(4096) as u32,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a GGUF header in memory: (key, type, encoded value bytes).
    pub fn header_bytes(kvs: &[(&str, u32, Vec<u8>)]) -> Vec<u8> {
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        b.extend((kvs.len() as u64).to_le_bytes());
        for (k, ty, v) in kvs {
            b.extend((k.len() as u64).to_le_bytes());
            b.extend(k.as_bytes());
            b.extend(ty.to_le_bytes());
            b.extend(v);
        }
        b
    }
    pub fn s(v: &str) -> Vec<u8> {
        let mut b = (v.len() as u64).to_le_bytes().to_vec();
        b.extend(v.as_bytes());
        b
    }
    pub fn u32v(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }
    fn str_array(items: &[&str]) -> Vec<u8> {
        let mut b = 8u32.to_le_bytes().to_vec();
        b.extend((items.len() as u64).to_le_bytes());
        for i in items {
            b.extend(s(i));
        }
        b
    }
    fn u32_array(items: &[u32]) -> Vec<u8> {
        let mut b = 4u32.to_le_bytes().to_vec();
        b.extend((items.len() as u64).to_le_bytes());
        for i in items {
            b.extend(i.to_le_bytes());
        }
        b
    }

    #[test]
    fn reads_llama_style_header_and_estimates_kv() {
        // Llama-3.2-1B-like: 16 layers, 2048 embd, 32 heads, 8 kv heads, head dim 64.
        let bytes = header_bytes(&[
            ("general.architecture", 8, s("llama")),
            ("general.name", 8, s("Tiny Llama")),
            ("llama.block_count", 4, u32v(16)),
            ("llama.embedding_length", 4, u32v(2048)),
            ("llama.attention.head_count", 4, u32v(32)),
            ("llama.attention.head_count_kv", 4, u32v(8)),
            ("llama.context_length", 4, u32v(131_072)),
            ("tokenizer.ggml.tokens", 9, str_array(&["a", "b", "c"])),
            ("tokenizer.chat_template", 8, s("{{ messages }}")),
        ]);
        let h = read(bytes.as_slice()).unwrap();
        assert_eq!(h.str("general.name"), Some("Tiny Llama"));
        assert_eq!(h.str("tokenizer.chat_template"), Some("{{ messages }}"));
        let p = h.memory_profile(800_000_000);
        // 16 layers × 8 heads × (64 + 64) × 2 bytes = 32 KiB per token (matches llama.cpp's log).
        assert_eq!(p.kv_bytes_per_token, 32 * 1024);
        assert_eq!(p.max_context, 131_072);
        assert_eq!(p.weights_bytes, 800_000_000);
    }

    #[test]
    fn hybrid_models_only_count_attention_layers() {
        // Qwen3.5-0.8B-like: 24 layers, every 4th has attention, 2 kv heads × 256 dims.
        let bytes = header_bytes(&[
            ("general.architecture", 8, s("qwen35")),
            ("qwen35.block_count", 4, u32v(24)),
            ("qwen35.embedding_length", 4, u32v(1024)),
            ("qwen35.attention.head_count", 4, u32v(8)),
            ("qwen35.attention.head_count_kv", 4, u32v(2)),
            ("qwen35.attention.key_length", 4, u32v(256)),
            ("qwen35.attention.value_length", 4, u32v(256)),
            ("qwen35.full_attention_interval", 4, u32v(4)),
        ]);
        let p = read(bytes.as_slice()).unwrap().memory_profile(1);
        // 6 attention layers × 2 × 512 × 2 bytes = 12 KiB per token.
        assert_eq!(p.kv_bytes_per_token, 12 * 1024);
    }

    #[test]
    fn per_layer_kv_heads_with_zeros() {
        let bytes = header_bytes(&[
            ("general.architecture", 8, s("lfm2")),
            ("lfm2.block_count", 4, u32v(4)),
            ("lfm2.embedding_length", 4, u32v(1024)),
            ("lfm2.attention.head_count", 4, u32v(16)),
            ("lfm2.attention.head_count_kv", 9, u32_array(&[0, 8, 0, 8])),
        ]);
        let p = read(bytes.as_slice()).unwrap().memory_profile(1);
        assert_eq!(p.kv_bytes_per_token, 2 * 8 * (64 + 64) * 2);
    }

    #[test]
    fn rejects_non_gguf_and_truncated_files() {
        assert!(matches!(read(&b"PK\x03\x04rest"[..]), Err(GgufError::NotGguf)));
        let bytes = header_bytes(&[("general.architecture", 8, s("llama"))]);
        assert!(read(&bytes[..bytes.len() - 3]).is_err());
    }

    #[test]
    fn reads_real_model_header_if_available() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.test-models/Qwen3.5-0.8B-Q4_K_M.gguf");
        if !path.exists() {
            eprintln!("skipped: {} not present", path.display());
            return;
        }
        let h = read_file(&path).unwrap();
        assert_eq!(h.architecture(), Some("qwen35"));
        assert!(h.str("tokenizer.chat_template").unwrap().contains("im_start"));
        let p = h.memory_profile(std::fs::metadata(&path).unwrap().len());
        // llama.cpp reports 384 MiB of KV at 32k context for this model = 12 KiB/token.
        assert_eq!(p.kv_bytes_per_token, 12 * 1024);
    }
}
