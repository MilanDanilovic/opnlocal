# opnlocal

Find out which AI models your device can run, set one up, measure it, and chat with it —
entirely on your own device. Built for people who have never heard of VRAM, quantization or
inference engines.

**Status:** v0.1.0. Proprietary (all rights reserved, see [LICENSE](LICENSE)).

## What it does

1. **Find what my device can run.** Detects memory, graphics, processor and free space.
2. **What would you like to do?** Everyday help, coding, writing, or private documents.
3. **Up to three picks** in plain words: *Recommended*, *Lighter & faster*, *Stronger, but tight*.
   Before a model is measured, only fit is shown (never a guessed speed).
4. **Download** with progress, cancel, resume after interruption, and sha256 verification.
5. **Measure** a short benchmark on this device ("Replies at about 11 words per second").
6. **Chat** locally: saved conversations, formatted replies, code highlighting, attach
   .txt/.md/.pdf/.docx, stop, try again, and an optional "Think harder" mode.

Nothing is sent anywhere except (1) model downloads you start, from huggingface.co, and (2) a
signed model list checked at most once a day from github.com (can be turned off). See
[docs/PRIVACY.md](docs/PRIVACY.md).

## How it's built

| Part | Where | What |
|---|---|---|
| Engine | [`crates/engine`](crates/engine) | Rust. Hardware detection, signed catalog, fit and recommendations, downloads, llama.cpp runtime, chat templates, benchmark, storage. No UI code. |
| App | [`app`](app) | Tauri 2 shell ([`app/src-tauri`](app/src-tauri)) + Svelte 5 UI ([`app/src`](app/src)). Commands forward to the engine; no local server. |
| Catalog | [`catalog`](catalog) | `source.json` (hand-written) → `catalog.json` (built from Hugging Face metadata and GGUF headers) + `catalog.json.sig`. Published to the public repo `MilanDanilovic/opnlocal-catalog`. |
| Catalog tool | [`crates/catalog-tool`](crates/catalog-tool) | Builds, signs and verifies the catalog (downloads and chats with every model). |

Architecture decision, platform matrix and risks: [docs/DECISIONS.md](docs/DECISIONS.md).

## Build

See [docs/BUILDING.md](docs/BUILDING.md) for every platform. Short version (Windows):

```powershell
# Once: Rust, Node 22, VS 2022 Build Tools, Vulkan SDK, CMake + Ninja, libclang
$env:CARGO_TARGET_DIR = 'C:\ot'      # short path: Windows' 260-char limit breaks the Vulkan shader build
cd app
npm ci
npx tauri build --bundles nsis        # → C:\ot\release\bundle\nsis\opnlocal_0.1.0_x64-setup.exe
```

## Status of v0.1.0 (2026-09-24)

| Platform | Built | Ran the real app | Not verified yet |
|---|---|---|---|
| Windows x64 | here (NSIS installer) | installed build, full journey with a real download, benchmark and chat on RX 7800 XT (Vulkan) | other GPUs (NVIDIA, Intel), Windows 10 |
| Android arm64 | here (signed APK + AAB) | x86_64 emulator: full journey (CPU). The arm64 release build launches under ARM translation and correctly refuses (the translation layer lacks dot-product instructions). | any physical phone |
| Linux x64 | CI (AppImage + .deb) | CI: installed .deb, detection + real generation (CPU), screenshot | a desktop session with a GPU |
| macOS arm64 | CI (.dmg, unsigned) | CI: detection + real generation on Metal, screenshot | a physical Mac, notarization |
| iOS arm64 | CI (unsigned .ipa + simulator app) | CI simulator: detection + real generation (CPU; the simulator's Metal is unreliable), screenshot | any physical iPhone (Metal, memory limits, signing) |

All 9 catalog models were downloaded, benchmarked and chatted with through the engine on the
Windows machine: [catalog/VERIFICATION.md](catalog/VERIFICATION.md).

## Test

See [docs/TESTING.md](docs/TESTING.md).

```powershell
cargo test -p opnlocal-engine --release     # engine (real llama.cpp tests use .test-models/)
cd app; npm test; npx playwright test       # UI unit tests + UI journeys with accessibility checks
npx playwright test -c playwright.real.config.ts   # the real Windows app, real download and chat
```
