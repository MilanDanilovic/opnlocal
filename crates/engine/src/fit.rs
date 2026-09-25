//! Will a model fit on this device? Memory math only; speed is never estimated here.
//!
//! need = weights + KV cache(context) + fixed state + compute scratch + overhead,
//! compared against per-platform budgets. Budgets are deliberately conservative: an app that
//! runs out of memory gets killed (phones) or makes the whole computer crawl (desktops).

use crate::catalog::{MemoryProfile, UseCase};
use crate::hardware::{DeviceInfo, GpuKind, Platform};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Runtime overhead not covered by the profile (code, small buffers, allocator slack).
pub const OVERHEAD_BYTES: u64 = 256 * 1024 * 1024;
/// Keep this much free on top of any download (the OS needs room too).
pub const STORAGE_MARGIN_BYTES: u64 = 512 * 1024 * 1024;
/// Never go below this context; smaller is useless for a conversation.
pub const MIN_CONTEXT: u32 = 2048;
/// The KV cache runs 8-bit (q8_0: 8.5 bits per value) while profiles measure the f16 cache.
/// When a backend can't do that, the runtime halves the context instead, so the estimate holds.
pub const KV_8BIT_FRACTION: f64 = 9.0 / 16.0;

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FitLabel {
    Comfortable,
    Tight,
    TooBig,
}

/// Where the model runs. Plain words in the UI: graphics chip / both / processor.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Placement {
    Gpu,
    Mixed,
    Cpu,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq)]
#[ts(export)]
pub struct Fit {
    pub label: FitLabel,
    pub placement: Placement,
    /// Context (tokens) the estimate was made for; used when loading.
    pub context: u32,
    #[ts(type = "number")]
    pub need_bytes: u64,
    /// How much more memory the device would need (TooBig only).
    #[ts(type = "number")]
    pub short_by_bytes: u64,
    /// It fits the device, but not the memory free right now: other apps should be closed.
    pub close_other_apps: bool,
}

/// Memory the model may use on this device.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq)]
#[ts(export)]
pub struct MemoryPools {
    /// Usable dedicated graphics memory (discrete GPUs only), 0 if none or disabled.
    #[ts(type = "number")]
    pub gpu_bytes: u64,
    /// GPU and CPU share system memory and the GPU does the work (Apple silicon, iPhone).
    pub unified_gpu: bool,
    /// System memory the model may use and still leave the device responsive.
    #[ts(type = "number")]
    pub comfortable_bytes: u64,
    /// Absolute ceiling before the OS starts killing apps or swapping heavily.
    #[ts(type = "number")]
    pub limit_bytes: u64,
    /// Free system memory right now.
    #[ts(type = "number")]
    pub available_now_bytes: u64,
}

impl MemoryPools {
    /// Budgets per platform. Phones keep far more headroom: the OS kills the foreground app
    /// on memory pressure, and other apps must survive in the background.
    pub fn for_device(device: &DeviceInfo, gpu_allowed: bool) -> MemoryPools {
        let ram = device.memory.total;
        let (comfortable, limit) = match device.platform {
            Platform::Android => (frac(ram, 0.35), frac(ram, 0.50)),
            // iOS reports the per-app limit directly (jetsam budget, raised by entitlements).
            Platform::Ios => {
                let app = device.memory.app_limit.unwrap_or(frac(ram, 0.5));
                (frac(app, 0.70), frac(app, 0.90))
            }
            Platform::Windows | Platform::Linux | Platform::MacOs => {
                (frac(ram, 0.50), frac(ram, 0.70))
            }
        };
        let discrete = device
            .gpus
            .iter()
            .filter(|g| g.kind == GpuKind::Discrete)
            .max_by_key(|g| g.memory_free);
        let unified = device.gpus.iter().any(|g| g.kind == GpuKind::Unified);
        MemoryPools {
            gpu_bytes: match (gpu_allowed, discrete) {
                (true, Some(g)) => frac(g.memory_free, 0.90),
                _ => 0,
            },
            unified_gpu: gpu_allowed && unified,
            comfortable_bytes: comfortable,
            limit_bytes: limit,
            available_now_bytes: device.memory.available,
        }
    }
}

fn frac(bytes: u64, f: f64) -> u64 {
    (bytes as f64 * f) as u64
}

/// Micro-batch size for prompt processing. Smaller on phones: the compute buffer scales with it.
pub fn ubatch_for(platform: Platform) -> u32 {
    match platform {
        Platform::Android | Platform::Ios => 256,
        _ => 512,
    }
}

/// Context to aim for. Documents need room; phones can't afford much.
pub fn desired_context(use_case: UseCase, platform: Platform) -> u32 {
    let mobile = matches!(platform, Platform::Android | Platform::Ios);
    match (use_case, mobile) {
        (UseCase::Documents, false) => 32_768,
        (_, false) => 16_384,
        (UseCase::Documents, true) => 8_192,
        (_, true) => 4_096,
    }
}

pub fn estimate_bytes(m: &MemoryProfile, context: u32, ubatch: u32) -> u64 {
    let ctx = context as u64;
    let swa_tokens = (m.swa_window as u64 + ubatch as u64).min(ctx);
    let kv = (m.kv_bytes_per_token * ctx + m.swa_kv_bytes_per_token * swa_tokens) as f64 * KV_8BIT_FRACTION;
    m.weights_bytes
        + kv as u64
        + m.fixed_bytes
        + m.compute_bytes_per_ubatch_token * ubatch as u64
        + OVERHEAD_BYTES
}

/// Fit at exactly this context.
pub fn fit_at(m: &MemoryProfile, pools: &MemoryPools, context: u32, ubatch: u32) -> Fit {
    let need = estimate_bytes(m, context, ubatch);
    let (label, placement, on_cpu) = if pools.gpu_bytes > 0 {
        if need <= pools.gpu_bytes {
            (FitLabel::Comfortable, Placement::Gpu, 0)
        } else {
            // Layers that don't fit on the graphics card stay in system memory.
            let spill = need - pools.gpu_bytes;
            (cpu_label(spill, pools), Placement::Mixed, spill)
        }
    } else {
        let placement = if pools.unified_gpu {
            Placement::Gpu
        } else {
            Placement::Cpu
        };
        (cpu_label(need, pools), placement, need)
    };
    let limit = pools.gpu_bytes + pools.limit_bytes;
    Fit {
        label,
        placement,
        context,
        need_bytes: need,
        short_by_bytes: if label == FitLabel::TooBig {
            need.saturating_sub(limit)
        } else {
            0
        },
        close_other_apps: label != FitLabel::TooBig
            && on_cpu > 0
            && on_cpu > pools.available_now_bytes,
    }
}

fn cpu_label(bytes: u64, pools: &MemoryPools) -> FitLabel {
    if bytes <= pools.comfortable_bytes {
        FitLabel::Comfortable
    } else if bytes <= pools.limit_bytes {
        FitLabel::Tight
    } else {
        FitLabel::TooBig
    }
}

/// Best fit for this use: the largest context (from `desired` down to [`MIN_CONTEXT`], halving)
/// that achieves the best label.
pub fn best_fit(m: &MemoryProfile, pools: &MemoryPools, desired: u32, ubatch: u32) -> Fit {
    let mut candidates = Vec::new();
    let mut ctx = desired.min(m.max_context.max(MIN_CONTEXT));
    loop {
        candidates.push(fit_at(m, pools, ctx, ubatch));
        if ctx <= MIN_CONTEXT {
            break;
        }
        ctx = (ctx / 2).max(MIN_CONTEXT);
    }
    // Candidates are in descending context order, so the first with the best label wins.
    let best = candidates.iter().map(|f| f.label).min().unwrap();
    *candidates.iter().find(|f| f.label == best).unwrap()
}

/// Context to load for a conversation that needs `needed` tokens (prompt plus reply room).
/// Normally the best fit from `desired` down; when the conversation is longer than that (a
/// long document), the smallest context that holds it and still fits the device, tight or
/// not. If nothing holds it, the largest context that fits, so the caller can say by how much
/// the conversation is too long.
pub fn context_for(m: &MemoryProfile, pools: &MemoryPools, desired: u32, needed: u32, ubatch: u32) -> Fit {
    let top = desired.max(needed.next_power_of_two()).min(m.max_context.max(MIN_CONTEXT));
    let normal = best_fit(m, pools, desired, ubatch);
    if normal.context >= needed {
        return normal;
    }
    let mut ctx = top;
    let mut fits = Vec::new();
    while ctx > normal.context {
        let f = fit_at(m, pools, ctx, ubatch);
        if f.label != FitLabel::TooBig {
            fits.push(f);
        }
        ctx /= 2;
    }
    fits.into_iter()
        .filter(|f| f.context >= needed)
        .min_by_key(|f| f.context)
        .or_else(|| fits_largest(m, pools, top, ubatch))
        .unwrap_or(normal)
}

/// Largest context from `top` down that fits the device at all.
fn fits_largest(m: &MemoryProfile, pools: &MemoryPools, top: u32, ubatch: u32) -> Option<Fit> {
    let mut ctx = top;
    while ctx >= MIN_CONTEXT {
        let f = fit_at(m, pools, ctx, ubatch);
        if f.label != FitLabel::TooBig {
            return Some(f);
        }
        ctx /= 2;
    }
    None
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq)]
#[ts(export)]
pub struct StorageFit {
    pub ok: bool,
    /// Space needed on disk for the download (plus margin).
    #[ts(type = "number")]
    pub need_bytes: u64,
    /// How much must be freed (0 if ok).
    #[ts(type = "number")]
    pub short_by_bytes: u64,
}

/// `already` = bytes of this file already on disk (a partial download).
pub fn storage_fit(file_size: u64, already: u64, free: u64) -> StorageFit {
    let need = file_size.saturating_sub(already) + STORAGE_MARGIN_BYTES;
    StorageFit {
        ok: need <= free,
        need_bytes: need,
        short_by_bytes: need.saturating_sub(free),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::tests::device;

    const GB: u64 = 1_000_000_000;

    fn profile(weights_gb: f64) -> MemoryProfile {
        MemoryProfile {
            weights_bytes: (weights_gb * GB as f64) as u64,
            kv_bytes_per_token: 32 * 1024,
            compute_bytes_per_ubatch_token: 1_000_000,
            max_context: 262_144,
            ..Default::default()
        }
    }

    #[test]
    fn estimate_adds_kv_for_context() {
        let m = profile(2.0);
        let small = estimate_bytes(&m, 4096, 512);
        let big = estimate_bytes(&m, 8192, 512);
        assert_eq!(big - small, (4096.0 * 32.0 * 1024.0 * KV_8BIT_FRACTION) as u64);
    }

    #[test]
    fn context_grows_for_long_conversations_and_stops_at_what_fits() {
        // 16 GB laptop, 2.7 GB model at 32 KiB/token: a normal chat gets the usual context, a
        // 100k-token document gets the smallest power of two that holds it.
        let d = device(Platform::Linux, 16, None);
        let pools = MemoryPools::for_device(&d, true);
        let m = profile(2.7);
        assert_eq!(context_for(&m, &pools, 16_384, 3_000, 512).context, 16_384);
        let long = context_for(&m, &pools, 16_384, 100_000, 512);
        assert_eq!((long.context, long.label), (131_072, FitLabel::Comfortable));
        // An 8 GB phone can't hold 100k tokens of a 2 GB model: the largest context that fits.
        let phone = MemoryPools::for_device(&device(Platform::Android, 8, None), true);
        let capped = context_for(&profile(2.0), &phone, 4096, 100_000, 256);
        assert!(capped.context < 100_000 && capped.context > 4096);
        assert_ne!(capped.label, FitLabel::TooBig);
    }

    #[test]
    fn sliding_window_layers_stop_growing_at_window() {
        let m = MemoryProfile {
            swa_kv_bytes_per_token: 1000,
            swa_window: 1024,
            ..profile(1.0)
        };
        let a = estimate_bytes(&MemoryProfile { kv_bytes_per_token: 0, ..m }, 16_384, 512);
        let b = estimate_bytes(&MemoryProfile { kv_bytes_per_token: 0, ..m }, 65_536, 512);
        assert_eq!(a, b, "SWA cost is capped by the window");
    }

    #[test]
    fn discrete_gpu_holds_model_fully() {
        let d = device(Platform::Windows, 32, Some((GpuKind::Discrete, 16)));
        let pools = MemoryPools::for_device(&d, true);
        let f = fit_at(&profile(8.0), &pools, 16_384, 512);
        assert_eq!((f.label, f.placement), (FitLabel::Comfortable, Placement::Gpu));
    }

    #[test]
    fn model_bigger_than_vram_spills_to_system_memory() {
        let d = device(Platform::Windows, 32, Some((GpuKind::Discrete, 16)));
        let pools = MemoryPools::for_device(&d, true);
        let f = fit_at(&profile(20.0), &pools, 16_384, 512);
        assert_eq!(f.placement, Placement::Mixed);
        assert_eq!(f.label, FitLabel::Comfortable);
    }

    #[test]
    fn gpu_disabled_means_processor_only() {
        let d = device(Platform::Windows, 32, Some((GpuKind::Discrete, 16)));
        let pools = MemoryPools::for_device(&d, false);
        assert_eq!(fit_at(&profile(8.0), &pools, 16_384, 512).placement, Placement::Cpu);
    }

    #[test]
    fn integrated_gpu_does_not_count_as_extra_memory() {
        let d = device(Platform::Windows, 16, Some((GpuKind::Integrated, 8)));
        let pools = MemoryPools::for_device(&d, true);
        assert_eq!(pools.gpu_bytes, 0);
        assert_eq!(fit_at(&profile(4.0), &pools, 8192, 512).placement, Placement::Cpu);
    }

    #[test]
    fn apple_silicon_runs_on_gpu_from_shared_memory() {
        let d = device(Platform::MacOs, 16, Some((GpuKind::Unified, 12)));
        let pools = MemoryPools::for_device(&d, true);
        let f = fit_at(&profile(5.0), &pools, 16_384, 512);
        assert_eq!((f.label, f.placement), (FitLabel::Comfortable, Placement::Gpu));
    }

    #[test]
    fn labels_follow_budget_thresholds() {
        // 16 GB laptop, no GPU: comfortable ≤ 8 GB, tight ≤ 11.2 GB.
        let d = device(Platform::Linux, 16, None);
        let pools = MemoryPools::for_device(&d, true);
        assert_eq!(fit_at(&profile(6.0), &pools, 4096, 512).label, FitLabel::Comfortable);
        assert_eq!(fit_at(&profile(9.5), &pools, 4096, 512).label, FitLabel::Tight);
        let too_big = fit_at(&profile(14.0), &pools, 4096, 512);
        assert_eq!(too_big.label, FitLabel::TooBig);
        assert!(too_big.short_by_bytes > 2 * GB);
    }

    #[test]
    fn phones_keep_more_headroom_than_desktops() {
        let phone = MemoryPools::for_device(&device(Platform::Android, 8, None), true);
        let laptop = MemoryPools::for_device(&device(Platform::Linux, 8, None), true);
        assert!(phone.comfortable_bytes < laptop.comfortable_bytes);
        // A 2.7 GB model is fine on an 8 GB laptop but tight on an 8 GB phone.
        assert_eq!(fit_at(&profile(2.7), &laptop, 4096, 256).label, FitLabel::Comfortable);
        assert_eq!(fit_at(&profile(2.7), &phone, 4096, 256).label, FitLabel::Tight);
    }

    #[test]
    fn iphone_uses_reported_app_limit() {
        let mut d = device(Platform::Ios, 8, Some((GpuKind::Unified, 5)));
        d.memory.app_limit = Some(5 * GB);
        let pools = MemoryPools::for_device(&d, true);
        assert_eq!(pools.limit_bytes, 4_500_000_000);
    }

    #[test]
    fn best_fit_shrinks_context_before_giving_up_comfort() {
        // 16 GB machine: 5.5 GB weights + 16k ctx of a 320 KiB/token model doesn't fit comfortably,
        // but a smaller context does.
        let d = device(Platform::Linux, 16, None);
        let pools = MemoryPools::for_device(&d, true);
        let m = MemoryProfile {
            kv_bytes_per_token: 320 * 1024,
            ..profile(5.5)
        };
        let f = best_fit(&m, &pools, 16_384, 512);
        assert_eq!(f.label, FitLabel::Comfortable);
        assert!(f.context < 16_384 && f.context >= MIN_CONTEXT);
    }

    #[test]
    fn best_fit_never_exceeds_trained_context() {
        let d = device(Platform::Linux, 64, None);
        let pools = MemoryPools::for_device(&d, true);
        let m = MemoryProfile {
            max_context: 8192,
            ..profile(1.0)
        };
        assert_eq!(best_fit(&m, &pools, 32_768, 512).context, 8192);
    }

    #[test]
    fn warns_to_close_apps_when_free_memory_is_low_right_now() {
        let mut d = device(Platform::Windows, 16, None);
        d.memory.available = 3 * GB;
        let pools = MemoryPools::for_device(&d, true);
        let f = fit_at(&profile(4.0), &pools, 4096, 512);
        assert_eq!(f.label, FitLabel::Comfortable);
        assert!(f.close_other_apps);
    }

    #[test]
    fn storage_accounts_for_partial_download_and_margin() {
        let s = storage_fit(10 * GB, 4 * GB, 7 * GB);
        assert!(s.ok);
        let s = storage_fit(10 * GB, 0, 7 * GB);
        assert!(!s.ok);
        assert_eq!(s.short_by_bytes, 3 * GB + STORAGE_MARGIN_BYTES);
    }
}
