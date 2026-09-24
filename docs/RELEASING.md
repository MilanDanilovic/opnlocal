# Releasing

## Keys (never in the repo)

Kept in `C:\Users\mdani\opnlocal-keys\` (outside the repo). **Back these up somewhere safe**:
losing the catalog key means installed apps can never accept a new model list; losing the
Android key means Google Play updates need a key reset.

| File | Purpose |
|---|---|
| `catalog-signing.pem` | ed25519 private key that signs `catalog.json`. Public key: `IGd0duRE2zwpKNtOsPayxo69nPoM4pMIfcXLkZ+U3d8=` (in `crates/engine/src/catalog.rs` → `TRUSTED_KEYS`). |
| `android-upload.jks` + `android-keystore.properties` | Android release signing (upload key for Play App Signing). |

To rotate the catalog key: add the new public key to `TRUSTED_KEYS`, ship an app update, and
only then start signing with the new key.

## Checklist

1. Bump the version in `app/src-tauri/tauri.conf.json` and `Cargo.toml` (`workspace.package`).
2. Full test pass (see TESTING.md), including the real-app test and `catalog-tool verify`.
3. Windows: `npx tauri build --bundles nsis` (see BUILDING.md).
4. Android: `npx tauri android build --target aarch64` with `GGML_CPU_ARM_ARCH` set.
5. Run the `build` workflow for `linux,macos,ios` and download the artifacts.
6. If the catalog changed: sign it and publish to `MilanDanilovic/opnlocal-catalog`.

## Signing status (v0.1.0)

None of the builds are signed by a recognized publisher yet:

- **Windows:** unsigned. SmartScreen shows "Windows protected your PC" → *More info* → *Run anyway*.
  A code-signing certificate (OV/EV) or Microsoft Trusted Signing removes this.
- **macOS:** unsigned and not notarized. Gatekeeper blocks the first launch: right-click the app →
  *Open*, or System Settings → Privacy & Security → *Open Anyway*. Needs an Apple Developer ID
  certificate and notarization to fix.
- **Android:** signed with our own upload key; installs after allowing "unknown sources".
  Google Play needs a developer account ($25 one-time).
- **iOS:** unsigned `.ipa`. It can't be installed as-is. Options:
  - **Free Apple ID + Sideloadly (Windows/macOS):** open Sideloadly, plug in the iPhone, drag in
    `opnlocal_*.ipa`, sign in with the Apple ID, *Start*. On the iPhone: Settings → General → VPN &
    Device Management → trust the developer; enable Developer Mode (Settings → Privacy &
    Security). Apps signed this way stop opening after 7 days until re-signed, and a free
    account may not grant the extra-memory entitlements, so large models can be refused.
  - **Apple Developer Program ($99/year):** proper signing, TestFlight and the App Store.
