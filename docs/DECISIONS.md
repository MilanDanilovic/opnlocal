# opnlocal: architecture decision (v1)

Status: accepted, 2026-09-24. Research notes behind each choice are summarized at the end.

## What we're building

One app that tells people which AI models their device can run, downloads one, measures it, and lets
them chat with it. Everything runs on the device. The user never needs to know what VRAM,
quantization or an inference engine is.

## Decision

| Concern | Choice | Why |
|---|---|---|
| Inference runtime | **llama.cpp** (GGUF), pinned via `llama-cpp-2 =0.1.157` | The only runtime that runs the *same model file* on Windows, macOS, Linux, Android and iOS. It has device and memory enumeration, load progress and perf counters built in. MIT license. |
| App shell | **Tauri 2** | One web UI on all 5 platforms, a Rust core in the same process, and small native plugins where needed. MIT/Apache license. |
| UI | **Svelte 5 + TypeScript** (Vite) | Least code for a streaming chat UI. Plain semantic HTML makes it accessible. |
| Core logic | One Rust crate, `crates/engine`, with no UI dependency | Detection, catalog, fit/recommendation, downloads, benchmark, chat, storage. Unit-testable without a window. |
| Process model | **Model runs inside the app on every platform** | Phones can't spawn processes. No local server means no local API to secure. |
| GPU backend | Windows/Linux: CPU + Vulkan, loaded at runtime. macOS/iOS: Metal. Android: CPU only. | Vulkan covers NVIDIA, AMD and Intel with one build. Phone GPUs rarely beat the CPU for replies and crash often. |
| CPU code | Windows/Linux: `GGML_CPU_ALL_VARIANTS` (best variant picked at runtime). Android: `armv8.2-a+dotprod+fp16` build, and the app refuses cleanly on older CPUs. | Fast on modern hardware, and never crashes with "illegal instruction" on older hardware. |
| Model catalog | Signed JSON (ed25519), bundled in the app, refreshed at most daily from the public repo `MilanDanilovic/opnlocal-catalog` | Works offline from the first launch. Can be updated without an app release. Can't be tampered with. |
| Model files | Downloaded from Hugging Face at a pinned commit and verified by sha256 | No login needed; nothing is redistributed by us. |
| Storage | Models in the app data folder. Conversations and settings as JSON files. | Nothing to migrate, nothing to corrupt at scale. |
| Network | Only (1) catalog refresh, (2) model downloads the user starts | No analytics, crash reports or update pings. |
| Code license | Apache-2.0 (from 2026-09-24; earlier commits were all rights reserved). Permissive dependencies only (plus MPL-2.0, copyleft per file). | Free and open source with a patent grant; paid store builds remain possible. |

## Platform support matrix (v1)

| | Windows | macOS | Linux | Android | iOS |
|---|---|---|---|---|---|
| Minimum | 10 (x64) | 13, Apple Silicon | x64, glibc 2.35 (Ubuntu 22.04+) | 9 (API 28), arm64-v8a, dotprod CPU | 17 |
| GPU | Vulkan (NVIDIA/AMD/Intel) | Metal | Vulkan | none (CPU) | Metal |
| Package | NSIS `.exe` (per-user, no admin) | `.dmg` (unsigned) | AppImage + `.deb` | `.apk` + `.aab` | unsigned `.ipa` (sideload) |
| Built on | this PC | GitHub Actions (macOS) | Docker / GitHub Actions | this PC | GitHub Actions (macOS) |
| Tested on real hardware | yes (Ryzen 7700X + RX 7800 XT) | no (CI only) | Docker/CI only | emulator + owner's phone | simulator (CI) + owner's iPhone via Sideloadly |
| Signing | none (SmartScreen warning) | none (Gatekeeper warning) | n/a | self-generated key | none; re-signed by Sideloadly with a free Apple ID (7 days) |

## Product rules that shape the code

- **Never invent performance numbers.** Before measuring, show only *fit* ("Runs comfortably",
  "Runs, but tight", "Too big for this device") and where it runs (graphics chip or processor).
  Speed appears only after the benchmark and is labeled "Measured on this device". Words per second
  are counted from the text actually produced.
- **Always set the context size explicitly.** llama.cpp's default (the model's full trained length)
  can allocate gigabytes.
- **Thinking off by default.** A per-chat "Think harder" switch turns it on; "Show thinking" reveals it.
- **One download at a time.** It resumes after interruption (HTTP Range) and is verified by sha256 at
  the end. Cancel deletes the partial file.
- **GPU crash guard.** A marker file is written before any GPU load. If the app dies during the load,
  the next launch uses the CPU and says why.
- **Context grows with the conversation, not the use case.** A normal chat gets 16k tokens on
  desktop (4k on phones); a long document makes the engine reload the model with the smallest
  power-of-two context that holds it and still fits the device (`fit::context_for`). The KV
  cache is 8-bit (q8_0) so that costs about half; where a backend can't do that, the runtime
  halves the context instead, so the memory estimate holds either way.
- **Documents longer than the largest context are not refused.** They are cut down to the chunks
  most relevant to the question with plain word matching (BM25, `engine::retrieval`), and the
  chat says so. No embedding model: multilingual ones are 300 MB to 600 MB and would have to be
  loaded next to the chat model on every turn; word matching costs nothing and covers most
  "what does it say about X" questions. An embedding model can slot in behind the same function.
- **Images: text read on the device, pictures for models that can see.** OCR models (ocrs, 12 MB)
  are compiled into the engine, so it works offline everywhere. Models with an image encoder in
  the catalog (`vision`) can look at the picture itself after that extra download; the
  encoder is loaded only for conversations that contain images.

## Known risks

1. Tauri's iOS toolchain breaks with new Xcode releases (tauri#15066, tauri#16121). Mitigation: iOS
   build in CI first, with Xcode pinned. If the binding fails, fall back to bindgen against the
   official llama.cpp xcframework.
2. `llama-cpp-2` has little iOS evidence. Same mitigation.
3. Out-of-memory kills on phones. Mitigation: our own memory budget per platform (see
   `engine::fit`), small context sizes, and models that don't fit are never offered.
4. Tauri 3 is in alpha; we stay on 2.x.

## Learned while building (2026-09-24)

- **Thinking budget.** Small reasoning models (Qwen3.5 0.8B/2B) asked to "think harder" reasoned
  until they hit the reply limit and never answered. The engine now closes the reasoning in the
  model's own format after 3/4 of the budget and asks for the answer with the rest. Verified on
  all catalog models (`catalog/VERIFICATION.md`).
- **Chat templates are real Jinja.** LFM2.5 uses `{% generation %}` (a Hugging Face training-only
  tag); it's stripped before rendering. gpt-oss introduces itself as "ChatGPT" unless
  `model_identity` is set; the catalog sets it (`template_vars`).
- **Some models always think** (LFM2.5, gpt-oss). The catalog says so (`always` / `always_on`), so
  the reasoning stays behind "Show thinking" and "Think harder" is only offered where it changes
  something.
- **Windows path length.** llama.cpp's Vulkan shader generator breaks under Windows' 260-character
  path limit; builds use a short target directory (`C:\ot`).
- **Apple linking.** macOS needs clang's runtime library linked explicitly (ggml-metal's
  `@available` checks); iOS needs the Accelerate framework declared in the Xcode project.
- **Licenses** (full review of 598 crates, npm, Android libraries, llama.cpp vendored code): no
  strong copyleft ships (four MPL-2.0 crates are copyleft per file only, and are unmodified).
  Notices are generated (`scripts/generate-notices.mjs`) and shown in the app. The Linux
  **AppImage** bundles Ubuntu's GTK/WebKitGTK and more (many LGPL, some parts GPL); the `.deb`
  doesn't. It ships anyway, with each library's license file inside and their source packages
  on the release page (`scripts/appimage-licenses.sh`).

## Research summary (2026-09-23)

- llama.cpp v0.5.0 / b11149. Stable semver tags since 2026-08. Now stewarded by Hugging Face (ggml.ai
  joined HF in 2026-02). MIT.
- Tauri 2.11.x stable. Mobile has no process spawning, so inference must be in-process there.
- Alternatives considered: Flutter + llamadart (young binding with one maintainer), Flutter + Rust
  via flutter_rust_bridge (more moving parts), React Native + llama.rn (no Linux desktop), LiteRT-LM
  (curated model formats only), MLC LLM (per-target compilation).
- Catalog models (all ungated on Hugging Face): Qwen3.5 0.8B/2B/4B/9B, Gemma 4 E2B/12B (Apache-2.0
  since 2026-04), gpt-oss 20B (Apache-2.0), Qwen3.6 35B-A3B (Apache-2.0), LFM2.5 2.6B (LFM Open
  License 1.0: free under US$10M annual revenue; accepted in-app).
