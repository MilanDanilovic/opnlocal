# Testing

Cheap tests run often; the expensive ones (real downloads, real devices) run before a release.

| Layer | Command | What it covers | Needs |
|---|---|---|---|
| Engine unit + integration | `cargo test -p opnlocal-engine --release` | fit math, recommendations, catalog signatures, GGUF parsing, downloads (local test server: resume, drop, redirect, wrong checksum, cancel, offline), storage, chat templates and thinking parsing, document text, **real llama.cpp** load/generate/stop/prefix reuse, benchmark | `.test-models/stories15M-q4_0.gguf` (19 MB) and `Qwen3.5-0.8B-Q4_K_M.gguf` for the real-model tests; they skip if absent |
| App crate | `cargo test -p opnlocal` | small helpers (content-URI names) | |
| UI unit | `cd app && npm test` | formatting, sanitized Markdown | |
| UI journeys | `cd app && npx playwright test` | every screen and state (first run, license acceptance, old CPU, too little memory, low disk, offline download, cancel, returning user, stop, settings) on desktop and phone viewports, **axe WCAG 2.2 AA** checks, screenshots in `test-results/shots/` | in-browser test backend (`src/lib/mock.ts`) |
| Real Windows app | `cd app && npx playwright test -c playwright.real.config.ts` | the built app (`C:\ot\release\opnlocal.exe`) driven over the WebView2 DevTools protocol: real detection, real download + sha256, real benchmark, three chat turns incl. stop | built app, internet |
| Real Android app | same command with `OPNLOCAL_ANDROID=1` | same journey on an emulator or phone via adb (Playwright Android) | debug APK installed and running, `adb` on PATH |
| iOS simulator | `scripts/ios-simulator-test.sh` (CI) | app launches; self-test prints device detection and a real generation | macOS + Xcode |
| Catalog | `catalog-tool verify <dir>` | every catalog model: download, benchmark, chat with thinking off and on | ~55 GB disk, internet |

Get the test models once:

```sh
mkdir -p .test-models && cd .test-models
curl -LO https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf
curl -L -o Qwen3.5-0.8B-Q4_K_M.gguf https://huggingface.co/unsloth/Qwen3.5-0.8B-GGUF/resolve/6ab461498e2023f6e3c1baea90a8f0fe38ab64d0/Qwen3.5-0.8B-Q4_K_M.gguf
```
