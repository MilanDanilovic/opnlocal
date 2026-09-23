//! opnlocal engine. Temporary bring-up probe; replaced by the real modules next.

use serde::Serialize;

#[derive(Serialize)]
pub struct ProbeDevice {
    pub name: String,
    pub description: String,
    pub backend: String,
    pub kind: String,
    pub memory_free: u64,
    pub memory_total: u64,
}

#[derive(Serialize)]
pub struct Probe {
    pub os: &'static str,
    pub arch: &'static str,
    pub devices: Vec<ProbeDevice>,
}

static BACKEND: std::sync::OnceLock<llama_cpp_2::llama_backend::LlamaBackend> =
    std::sync::OnceLock::new();

pub fn probe() -> Probe {
    BACKEND.get_or_init(|| {
        let b = llama_cpp_2::llama_backend::LlamaBackend::init().unwrap();
        if llama_cpp_2::list_llama_ggml_backend_devices().is_empty()
            && let Ok(exe) = std::env::current_exe()
        {
            llama_cpp_2::llama_backend::load_backends_from_path(exe.parent().unwrap());
        }
        b
    });
    let devices = llama_cpp_2::list_llama_ggml_backend_devices()
        .into_iter()
        .map(|d| ProbeDevice {
            name: d.name,
            description: d.description,
            backend: d.backend,
            kind: format!("{:?}", d.device_type),
            memory_free: d.memory_free as u64,
            memory_total: d.memory_total as u64,
        })
        .collect();
    Probe {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        devices,
    }
}
