//! Picks up to three models for a use case: the best comfortable fit, a lighter/faster one,
//! and a stronger one that is tight. "Faster" only ever means "smaller file": no speed is claimed
//! before the model is measured on this device.

use crate::catalog::{CatalogModel, UseCase};
use crate::fit::{self, Fit, FitLabel, MemoryPools, StorageFit};
use crate::hardware::DeviceInfo;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Slot {
    Recommended,
    Lighter,
    Stronger,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ModelOption {
    pub model_id: String,
    pub fit: Fit,
    pub storage: StorageFit,
    /// Quality score for the chosen use case (0 = not made for it).
    pub score: u8,
    pub installed: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Pick {
    pub slot: Slot,
    pub option: ModelOption,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Unsupported {
    /// The processor can't run our build (see `DeviceIssue::CpuTooOld`).
    CpuTooOld,
    /// Even the smallest suitable model needs more memory than the device can give.
    NotEnoughMemory {
        #[ts(type = "number")]
        smallest_need_bytes: u64,
    },
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Recommendations {
    pub use_case: UseCase,
    pub picks: Vec<Pick>,
    /// Every model, best fits first; non-fitting ones carry the reason in `fit`.
    pub all: Vec<ModelOption>,
    pub unsupported: Option<Unsupported>,
}

/// `installed(id)` tells whether a model file is already on disk (or partly: `partial_bytes`).
pub fn recommend(
    models: &[&CatalogModel],
    device: &DeviceInfo,
    pools: &MemoryPools,
    use_case: UseCase,
    installed: impl Fn(&str) -> bool,
    partial_bytes: impl Fn(&str) -> u64,
) -> Recommendations {
    let ubatch = fit::ubatch_for(device.platform);
    let desired = fit::desired_context(use_case, device.platform);
    let mut all: Vec<ModelOption> = models
        .iter()
        .map(|m| {
            let is_installed = installed(&m.id);
            let already = if is_installed {
                m.file.size
            } else {
                partial_bytes(&m.id)
            };
            ModelOption {
                model_id: m.id.clone(),
                fit: fit::best_fit(&m.memory, pools, desired, ubatch),
                storage: fit::storage_fit(m.file.size, already, device.storage.free),
                score: m.scores.get(use_case),
                installed: is_installed,
            }
        })
        .collect();
    let weight = |id: &str| {
        models
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.memory.weights_bytes)
            .unwrap_or(u64::MAX)
    };
    // Best fits first, then higher score, then smaller.
    all.sort_by(|a, b| {
        (a.fit.label, b.score, weight(&a.model_id)).cmp(&(b.fit.label, a.score, weight(&b.model_id)))
    });

    if !device.cpu_supported() {
        return Recommendations {
            use_case,
            picks: vec![],
            all,
            unsupported: Some(Unsupported::CpuTooOld),
        };
    }

    let suitable = |label: FitLabel| {
        all.iter()
            .filter(move |o| o.score > 0 && o.fit.label == label)
            .collect::<Vec<_>>()
    };
    let comfortable = suitable(FitLabel::Comfortable);
    let tight = suitable(FitLabel::Tight);

    // Highest score; among equals the smaller (faster, lighter) model.
    let best = |opts: &[&ModelOption]| -> Option<ModelOption> {
        opts.iter()
            .max_by(|a, b| {
                (a.score, std::cmp::Reverse(weight(&a.model_id)))
                    .cmp(&(b.score, std::cmp::Reverse(weight(&b.model_id))))
            })
            .map(|o| (*o).clone())
    };

    let mut picks = Vec::new();
    let recommended = best(&comfortable).or_else(|| best(&tight));
    if let Some(rec) = recommended {
        let rec_weight = weight(&rec.model_id);
        let lighter: Vec<&ModelOption> = comfortable
            .iter()
            .copied()
            .filter(|o| weight(&o.model_id) * 4 <= rec_weight * 3)
            .collect();
        let stronger: Vec<&ModelOption> = tight
            .iter()
            .copied()
            .filter(|o| o.model_id != rec.model_id && o.score > rec.score)
            .collect();
        let rec_is_tight = rec.fit.label == FitLabel::Tight;
        picks.push(Pick {
            slot: Slot::Recommended,
            option: rec,
        });
        if let Some(l) = best(&lighter) {
            picks.push(Pick {
                slot: Slot::Lighter,
                option: l,
            });
        }
        if !rec_is_tight && let Some(s) = best(&stronger) {
            picks.push(Pick {
                slot: Slot::Stronger,
                option: s,
            });
        }
    }

    let unsupported = picks.is_empty().then(|| Unsupported::NotEnoughMemory {
        smallest_need_bytes: all
            .iter()
            .filter(|o| o.score > 0)
            .map(|o| o.fit.need_bytes)
            .min()
            .unwrap_or(0),
    });

    Recommendations {
        use_case,
        picks,
        all,
        unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Scores;
    use crate::catalog::tests::test_model;
    use crate::hardware::tests::device;
    use crate::hardware::{DeviceIssue, GpuKind, Platform};

    fn s(everyday: u8, coding: u8) -> Scores {
        Scores {
            everyday,
            coding,
            writing: everyday,
            documents: everyday,
        }
    }

    fn catalog() -> Vec<CatalogModel> {
        vec![
            test_model("tiny", 0.5, s(1, 1)),
            test_model("small", 1.3, s(2, 1)),
            test_model("mid", 2.7, s(3, 3)),
            test_model("big", 5.7, s(4, 3)),
            test_model("huge", 9.0, s(5, 5)),
            test_model("coder-only", 2.0, s(0, 4)),
        ]
    }

    fn run(d: &DeviceInfo, use_case: UseCase) -> Recommendations {
        let models = catalog();
        let refs: Vec<&CatalogModel> = models.iter().collect();
        let pools = MemoryPools::for_device(d, true);
        recommend(&refs, d, &pools, use_case, |_| false, |_| 0)
    }

    fn slots(r: &Recommendations) -> Vec<(Slot, &str)> {
        r.picks
            .iter()
            .map(|p| (p.slot, p.option.model_id.as_str()))
            .collect()
    }

    #[test]
    fn laptop_16gb_gets_best_comfortable_plus_lighter_plus_stronger() {
        let r = run(&device(Platform::Linux, 16, None), UseCase::Everyday);
        assert_eq!(
            slots(&r),
            vec![
                (Slot::Recommended, "big"),
                (Slot::Lighter, "mid"),
                (Slot::Stronger, "huge")
            ]
        );
        assert!(r.unsupported.is_none());
    }

    #[test]
    fn scores_zero_models_are_never_picked_for_that_use() {
        let r = run(&device(Platform::Linux, 16, None), UseCase::Everyday);
        assert!(!r.picks.iter().any(|p| p.option.model_id == "coder-only"));
        let r = run(&device(Platform::Linux, 8, None), UseCase::Coding);
        assert_eq!(r.picks[0].option.model_id, "coder-only");
    }

    #[test]
    fn small_phone_gets_small_models_only() {
        let r = run(&device(Platform::Android, 6, None), UseCase::Everyday);
        let ids: Vec<_> = slots(&r).into_iter().map(|(_, id)| id).collect();
        assert!(!ids.contains(&"big") && !ids.contains(&"huge"));
        assert_eq!(r.picks[0].slot, Slot::Recommended);
    }

    #[test]
    fn gpu_machine_can_run_the_biggest_comfortably() {
        let d = device(Platform::Windows, 32, Some((GpuKind::Discrete, 16)));
        let r = run(&d, UseCase::Coding);
        assert_eq!(r.picks[0].option.model_id, "huge");
        assert_eq!(r.picks[0].option.fit.label, FitLabel::Comfortable);
    }

    #[test]
    fn only_tight_fits_recommends_tight_and_no_stronger() {
        // 3 GB phone: comfortable ≤ 1.05 GB, limit 1.5 GB → only "tiny" (0.5 GB + buffers) fits, tightly.
        let r = run(&device(Platform::Android, 3, None), UseCase::Everyday);
        assert_eq!(slots(&r), vec![(Slot::Recommended, "tiny")]);
        assert_eq!(r.picks[0].option.fit.label, FitLabel::Tight);
    }

    #[test]
    fn nothing_fits_is_unsupported_with_smallest_need() {
        let r = run(&device(Platform::Android, 1, None), UseCase::Everyday);
        assert!(r.picks.is_empty());
        match r.unsupported {
            Some(Unsupported::NotEnoughMemory { smallest_need_bytes }) => {
                assert!(smallest_need_bytes > 500_000_000)
            }
            other => panic!("expected NotEnoughMemory, got {other:?}"),
        }
    }

    #[test]
    fn old_cpu_is_unsupported_regardless_of_memory() {
        let mut d = device(Platform::Android, 12, None);
        d.issues.push(DeviceIssue::CpuTooOld {
            missing: vec!["dotprod".into()],
        });
        let r = run(&d, UseCase::Everyday);
        assert!(r.picks.is_empty());
        assert_eq!(r.unsupported, Some(Unsupported::CpuTooOld));
    }

    #[test]
    fn all_models_listed_best_fit_first() {
        let r = run(&device(Platform::Android, 8, None), UseCase::Everyday);
        assert_eq!(r.all.len(), 6);
        let labels: Vec<_> = r.all.iter().map(|o| o.fit.label).collect();
        let mut sorted = labels.clone();
        sorted.sort();
        assert_eq!(labels, sorted);
    }

    #[test]
    fn low_disk_space_is_reported_per_model() {
        let mut d = device(Platform::Linux, 32, None);
        d.storage.free = 3_000_000_000;
        let r = run(&d, UseCase::Everyday);
        let huge = r.all.iter().find(|o| o.model_id == "huge").unwrap();
        assert!(!huge.storage.ok);
        let tiny = r.all.iter().find(|o| o.model_id == "tiny").unwrap();
        assert!(tiny.storage.ok);
    }
}
