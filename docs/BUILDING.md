# Building opnlocal

All platforms build the same Rust engine (`crates/engine`) and Svelte UI (`app`), wrapped by
Tauri 2 (`app/src-tauri`). llama.cpp is compiled from source by the `llama-cpp-sys-2` crate
(pinned `=0.1.157`), so every build needs CMake and a C/C++ toolchain for the target.

| Target | Where it builds | Output |
|---|---|---|
| Windows x64 | Windows | `opnlocal_<v>_x64-setup.exe` (NSIS, per-user, no admin) |
| Android arm64 (+ x86_64 for the emulator) | Windows, macOS or Linux | `.apk`, `.aab` |
| Linux x64 | Ubuntu 22.04 (CI) or Docker | `.AppImage`, `.deb` |
| macOS arm64 | macOS (CI) | `.app`, `.dmg` |
| iOS arm64 | macOS (CI) | unsigned `.ipa`; simulator `.app` |

CI does Linux, macOS and iOS: `.github/workflows/build.yml` (manual trigger, choose platforms).

## Common prerequisites

- Rust stable (tested with 1.97) — `rustup`
- Node.js 22 and npm
- CMake ≥ 3.28 and Ninja (`py -m pip install cmake ninja` works without admin rights)
- libclang (for bindgen): LLVM, or `py -m pip install libclang` and set `LIBCLANG_PATH` to
  `.../site-packages/clang/native`
- In `app/`: `npm ci`

## Windows (installer)

Also needed: Visual Studio 2022 Build Tools (C++ workload, Windows 10/11 SDK) and the
[Vulkan SDK](https://vulkan.lunarg.com/) (`VULKAN_SDK` set by its installer).

```powershell
$env:CARGO_TARGET_DIR = 'C:\ot'   # REQUIRED: a short path. The Vulkan shader generator fails past
                                  # Windows' 260-character path limit under a normal target dir.
$env:CMAKE_GENERATOR = 'Ninja'
cd app
npx tauri build --bundles nsis
# → C:\ot\release\bundle\nsis\opnlocal_<version>_x64-setup.exe
```

How the llama.cpp libraries ship: on Windows and Linux llama.cpp is built as shared libraries
with runtime-loaded backends (`ggml-cpu-*.dll` variants for different processors, and
`ggml-vulkan.dll`). `app/src-tauri/build.rs` copies them next to the executable and into
`app/src-tauri/libs/`, which `tauri.windows.conf.json` bundles into the install folder. The C
runtime is linked statically (`.cargo/config.toml`), so no Visual C++ Redistributable is needed.

## Android

Also needed: JDK 17, Android SDK with platform 36, build-tools 36, NDK r29
(`sdkmanager "platforms;android-36" "build-tools;36.0.0" "ndk;29.0.14206865" "platform-tools"`),
and Rust targets `aarch64-linux-android` (phones) and `x86_64-linux-android` (emulator).
Set `ANDROID_HOME` and `NDK_HOME`.

```powershell
$env:CARGO_TARGET_DIR = 'C:\ot'; $env:CMAKE_GENERATOR = 'Ninja'
# Phones: the arm64 build targets armv8.2-a with dot-product + fp16 instructions (2019+ phones).
# The app refuses cleanly on older processors instead of crashing.
$env:GGML_CPU_ARM_ARCH = 'armv8.2-a+dotprod+fp16'
cd app
npx tauri android build --target aarch64          # release .apk + .aab (signed if the key exists)
npx tauri android build --debug --target x86_64 --apk   # for the emulator
```

Release signing reads `~/opnlocal-keys/android-keystore.properties` (or the path in
`OPNLOCAL_ANDROID_KEYSTORE_PROPERTIES`). Without it, release builds are unsigned. See RELEASING.md.

## Linux (in Docker, from any OS)

```sh
docker run --rm -v "$PWD":/src -w /src ubuntu:22.04 bash scripts/build-linux.sh
```

or push and run the `build` workflow with `platforms=linux`. Packages needed on Ubuntu 22.04:
`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf file`, plus the LunarG
Vulkan SDK tarball for headers and `glslc` (see the workflow). The app finds its llama.cpp
libraries through an rpath (`$ORIGIN:$ORIGIN/../lib/opnlocal`).

## macOS

Xcode (command line tools), then:

```sh
cd app && npx tauri build --bundles app,dmg
```

Metal is built in statically. `crates/engine/build.rs` links clang's runtime library, which
ggml's Metal code needs for its OS-version checks.

## iOS

Xcode, `brew install xcodegen`, Rust targets `aarch64-apple-ios aarch64-apple-ios-sim`:

```sh
cd app
npx tauri ios init --ci && bash ../scripts/ios-patch-project.sh   # adds memory entitlements
npx tauri ios build --debug --target aarch64-sim                  # simulator
bash ../scripts/ios-simulator-test.sh                             # run it + self-test
npx tauri ios build --target aarch64 --no-sign                    # unsigned .ipa
```

`gen/apple` is regenerated each time and not committed. Installing on an iPhone needs signing:
see RELEASING.md (Sideloadly with a free Apple ID, or an Apple Developer account).

## Updating the model catalog

1. Edit `catalog/source.json` (editorial fields; pin `revision` to a 40-character commit).
2. `cargo run -p catalog-tool --release -- build`: fetches sizes and sha256 from the Hugging Face
   API and computes memory profiles from each file's GGUF header.
3. `cargo run -p catalog-tool --release -- verify <models-dir> [ids…]`: downloads, benchmarks and
   chats with each model through the engine. Writes `catalog/VERIFICATION.md`.
4. Bump `version`, run `catalog-tool sign <key.pem>`, and commit `catalog.json` + `.sig` both
   here (built into the app) and to `MilanDanilovic/opnlocal-catalog` (fetched by installed apps).
