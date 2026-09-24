//! What is this device? Memory, processor, graphics, storage, and what our runtime can use.
//!
//! Graphics devices come from llama.cpp's own device list, so we report exactly what the engine
//! can use (not what the OS says exists). Everything else comes from standard OS APIs.
//! Nothing here is ever sent anywhere.

use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
    Android,
    Ios,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "android") {
            Platform::Android
        } else if cfg!(target_os = "ios") {
            Platform::Ios
        } else {
            Platform::Linux
        }
    }

    pub fn is_mobile(self) -> bool {
        matches!(self, Platform::Android | Platform::Ios)
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct DeviceInfo {
    pub platform: Platform,
    pub os_version: String,
    /// Phone model when the OS tells us ("Pixel 8"), otherwise None.
    pub device_name: Option<String>,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub gpus: Vec<GpuInfo>,
    pub storage: StorageInfo,
    pub issues: Vec<DeviceIssue>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct CpuInfo {
    pub name: String,
    pub cores: u32,
    pub threads: u32,
    pub arch: String,
    /// Instruction set extensions relevant to inference speed (for the Advanced report).
    pub features: Vec<String>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct MemoryInfo {
    #[ts(type = "number")]
    pub total: u64,
    #[ts(type = "number")]
    pub available: u64,
    /// iOS only: memory this app may still use before the OS stops it.
    #[ts(type = "number | null")]
    pub app_limit: Option<u64>,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GpuKind {
    /// Separate graphics card with its own memory.
    Discrete,
    /// Built into the processor, shares system memory; usually not faster than the CPU path.
    Integrated,
    /// Apple silicon / iPhone: shares memory, and is the fast path.
    Unified,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct GpuInfo {
    pub name: String,
    /// llama.cpp backend ("Vulkan", "Metal").
    pub backend: String,
    pub kind: GpuKind,
    #[ts(type = "number")]
    pub memory_total: u64,
    #[ts(type = "number")]
    pub memory_free: u64,
    /// llama.cpp device index (for `LlamaModelParams::with_devices`).
    pub device_index: u32,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct StorageInfo {
    #[ts(type = "number")]
    pub free: u64,
    #[ts(type = "number")]
    pub total: u64,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum DeviceIssue {
    /// The processor lacks instructions our build needs (older phones).
    CpuTooOld { missing: Vec<String> },
    /// No usable graphics acceleration was found; the processor will do the work.
    NoGpuAcceleration,
}

impl DeviceInfo {
    /// Whether this device can run anything at all.
    pub fn cpu_supported(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|i| matches!(i, DeviceIssue::CpuTooOld { .. }))
    }
}

/// Detects the device. `storage_dir` is where models are stored (for free space).
/// Requires the runtime to be initialized (see [`crate::llm::init_runtime`]).
pub fn detect(storage_dir: &Path) -> DeviceInfo {
    use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
    let platform = Platform::current();
    let sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_memory(MemoryRefreshKind::everything())
            .with_cpu(CpuRefreshKind::nothing()),
    );

    let cpu = CpuInfo {
        name: sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(platform_cpu_name)
            .unwrap_or_else(|| "Unknown processor".into()),
        cores: System::physical_core_count().unwrap_or(sys.cpus().len()) as u32,
        threads: sys.cpus().len().max(1) as u32,
        arch: std::env::consts::ARCH.to_string(),
        features: cpu_features(),
    };

    let memory = MemoryInfo {
        total: sys.total_memory(),
        available: sys.available_memory(),
        app_limit: ios_app_memory_limit(),
    };

    // The iOS simulator's Metal is an emulation that produces wrong results for llama.cpp
    // kernels (verified in CI: garbage output), so the simulator build runs on the processor.
    let gpus = if cfg!(target_abi = "sim") {
        Vec::new()
    } else {
        gpus_from_devices(platform, crate::llm::devices())
    };

    let mut issues = Vec::new();
    let missing = missing_cpu_features(platform, &cpu.arch, &cpu.features);
    if !missing.is_empty() {
        issues.push(DeviceIssue::CpuTooOld { missing });
    }
    if gpus.is_empty() && platform != Platform::Android && !cfg!(target_abi = "sim") {
        issues.push(DeviceIssue::NoGpuAcceleration);
    }

    DeviceInfo {
        platform,
        os_version: platform_os_version()
            .or_else(System::long_os_version)
            .unwrap_or_else(|| std::env::consts::OS.into()),
        device_name: device_name(),
        cpu,
        memory,
        gpus,
        storage: StorageInfo {
            free: fs4::available_space(storage_dir).unwrap_or(0),
            total: fs4::total_space(storage_dir).unwrap_or(0),
        },
        issues,
    }
}

/// A llama.cpp device as reported by the runtime.
#[derive(Clone, Debug)]
pub struct RuntimeDevice {
    pub index: u32,
    pub description: String,
    pub backend: String,
    pub is_gpu: bool,
    pub is_integrated: bool,
    pub memory_total: u64,
    pub memory_free: u64,
}

/// Maps runtime devices to what we show and plan with. Duplicates (the same card registered
/// twice) are dropped; Apple GPUs are always unified memory.
pub fn gpus_from_devices(platform: Platform, devices: Vec<RuntimeDevice>) -> Vec<GpuInfo> {
    let mut gpus: Vec<GpuInfo> = Vec::new();
    for d in devices.into_iter().filter(|d| d.is_gpu || d.is_integrated) {
        if gpus
            .iter()
            .any(|g| g.name == d.description && g.backend == d.backend)
        {
            continue;
        }
        let kind = if matches!(platform, Platform::MacOs | Platform::Ios) {
            GpuKind::Unified
        } else if d.is_integrated {
            GpuKind::Integrated
        } else {
            GpuKind::Discrete
        };
        gpus.push(GpuInfo {
            name: d.description.trim().to_string(),
            backend: d.backend,
            kind,
            memory_total: d.memory_total,
            memory_free: d.memory_free,
            device_index: d.index,
        });
    }
    gpus
}

fn cpu_features() -> Vec<String> {
    #[allow(unused_mut)]
    let mut f: Vec<&str> = Vec::new();
    #[cfg(target_arch = "x86_64")]
    {
        for (name, on) in [
            ("avx", std::arch::is_x86_feature_detected!("avx")),
            ("avx2", std::arch::is_x86_feature_detected!("avx2")),
            ("fma", std::arch::is_x86_feature_detected!("fma")),
            ("f16c", std::arch::is_x86_feature_detected!("f16c")),
            ("avx512f", std::arch::is_x86_feature_detected!("avx512f")),
        ] {
            if on {
                f.push(name);
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        for (name, on) in [
            ("neon", std::arch::is_aarch64_feature_detected!("neon")),
            ("dotprod", std::arch::is_aarch64_feature_detected!("dotprod")),
            ("fp16", std::arch::is_aarch64_feature_detected!("fp16")),
            ("i8mm", std::arch::is_aarch64_feature_detected!("i8mm")),
            ("sve", std::arch::is_aarch64_feature_detected!("sve")),
        ] {
            if on {
                f.push(name);
            }
        }
    }
    f.into_iter().map(String::from).collect()
}

/// The Android arm64 build targets armv8.2-a+dotprod+fp16 (see docs/DECISIONS.md). Running it on
/// an older core would crash with an illegal instruction, so we refuse cleanly instead.
/// (x86_64 Android builds exist only for the emulator and use a baseline x86-64 target.)
pub fn missing_cpu_features(platform: Platform, arch: &str, features: &[String]) -> Vec<String> {
    if platform != Platform::Android || arch != "aarch64" {
        return Vec::new();
    }
    ["dotprod", "fp16"]
        .into_iter()
        .filter(|req| !features.iter().any(|f| f == req))
        .map(String::from)
        .collect()
}

#[cfg(target_os = "ios")]
fn ios_app_memory_limit() -> Option<u64> {
    unsafe extern "C" {
        fn os_proc_available_memory() -> usize;
    }
    // SAFETY: plain libSystem query (iOS 13+), no arguments.
    let available = unsafe { os_proc_available_memory() } as u64;
    // 0 means "not available" (e.g. the simulator); fall back to the total-memory budget.
    (available > 0).then_some(available)
}

#[cfg(not(target_os = "ios"))]
fn ios_app_memory_limit() -> Option<u64> {
    None
}

#[cfg(target_os = "android")]
fn prop(name: &str) -> Option<String> {
    let key = std::ffi::CString::new(name).ok()?;
    let mut buf = [0u8; 92]; // PROP_VALUE_MAX
    // SAFETY: buf is PROP_VALUE_MAX bytes as required by the API.
    let len = unsafe { libc::__system_property_get(key.as_ptr(), buf.as_mut_ptr().cast()) };
    (len > 0)
        .then(|| String::from_utf8_lossy(&buf[..len as usize]).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Phones don't report a processor brand to apps; the chip name is in system properties
/// (Android 12+: "Qualcomm SM8650"), else the board platform ("kalama").
#[cfg(target_os = "android")]
fn platform_cpu_name() -> Option<String> {
    match (prop("ro.soc.manufacturer"), prop("ro.soc.model")) {
        (Some(m), Some(model)) => Some(format!("{m} {model}")),
        (None, Some(model)) => Some(model),
        _ => prop("ro.board.platform").or_else(|| prop("ro.hardware")),
    }
}

#[cfg(not(target_os = "android"))]
fn platform_cpu_name() -> Option<String> {
    None
}

#[cfg(target_os = "android")]
fn platform_os_version() -> Option<String> {
    prop("ro.build.version.release").map(|v| format!("Android {v}"))
}

#[cfg(not(target_os = "android"))]
fn platform_os_version() -> Option<String> {
    None
}

#[cfg(target_os = "android")]
fn device_name() -> Option<String> {
    let model = prop("ro.product.model")?;
    match prop("ro.product.manufacturer") {
        Some(m) if !model.to_lowercase().starts_with(&m.to_lowercase()) => {
            Some(format!("{} {}", capitalize(&m), model))
        }
        _ => Some(model),
    }
}

#[cfg(target_os = "android")]
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

#[cfg(not(target_os = "android"))]
fn device_name() -> Option<String> {
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    /// A synthetic device for tests of fit/recommend logic.
    pub fn device(platform: Platform, ram_gb: u64, gpu: Option<(GpuKind, u64)>) -> DeviceInfo {
        DeviceInfo {
            platform,
            os_version: "test".into(),
            device_name: None,
            cpu: CpuInfo {
                name: "Test CPU".into(),
                cores: 8,
                threads: 16,
                arch: "x86_64".into(),
                features: vec![],
            },
            memory: MemoryInfo {
                total: ram_gb * GB,
                available: ram_gb * GB * 3 / 4,
                app_limit: None,
            },
            gpus: gpu
                .map(|(kind, free_gb)| GpuInfo {
                    name: "Test GPU".into(),
                    backend: "Vulkan".into(),
                    kind,
                    memory_total: free_gb * GB,
                    memory_free: free_gb * GB,
                    device_index: 0,
                })
                .into_iter()
                .collect(),
            storage: StorageInfo {
                free: 100 * GB,
                total: 500 * GB,
            },
            issues: vec![],
        }
    }

    fn rd(desc: &str, gpu: bool, igpu: bool) -> RuntimeDevice {
        RuntimeDevice {
            index: 0,
            description: desc.into(),
            backend: if gpu || igpu { "Vulkan" } else { "CPU" }.into(),
            is_gpu: gpu,
            is_integrated: igpu,
            memory_total: 16 * GB,
            memory_free: 15 * GB,
        }
    }

    #[test]
    fn maps_runtime_devices_and_drops_duplicates_and_cpu() {
        let gpus = gpus_from_devices(
            Platform::Windows,
            vec![
                rd("AMD Radeon RX 7800 XT", true, false),
                rd("AMD Radeon(TM) Graphics", false, true),
                rd("AMD Ryzen 7 7700X", false, false),
                rd("AMD Radeon RX 7800 XT", true, false),
            ],
        );
        let kinds: Vec<_> = gpus.iter().map(|g| (g.name.as_str(), g.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                ("AMD Radeon RX 7800 XT", GpuKind::Discrete),
                ("AMD Radeon(TM) Graphics", GpuKind::Integrated)
            ]
        );
    }

    #[test]
    fn apple_gpus_are_unified_memory() {
        let gpus = gpus_from_devices(Platform::MacOs, vec![rd("Apple M3", true, false)]);
        assert_eq!(gpus[0].kind, GpuKind::Unified);
    }

    #[test]
    fn old_android_cpu_is_flagged() {
        let old = vec!["neon".to_string()];
        assert_eq!(
            missing_cpu_features(Platform::Android, "aarch64", &old),
            vec!["dotprod", "fp16"]
        );
        let modern: Vec<String> = ["neon", "dotprod", "fp16"].map(String::from).to_vec();
        assert!(missing_cpu_features(Platform::Android, "aarch64", &modern).is_empty());
        assert!(missing_cpu_features(Platform::Windows, "aarch64", &old).is_empty());
        // The x86_64 emulator build has no ARM requirements.
        assert!(missing_cpu_features(Platform::Android, "x86_64", &[]).is_empty());
    }

    #[test]
    fn detects_this_machine() {
        let _serial = crate::llm::tests::serial();
        crate::llm::init_runtime(None);
        let d = detect(&std::env::temp_dir());
        assert_eq!(d.platform, Platform::current());
        assert!(d.memory.total > 0 && d.memory.available <= d.memory.total);
        assert!(d.cpu.threads >= 1);
        assert!(d.storage.total >= d.storage.free && d.storage.free > 0);
    }
}
