# Privacy

opnlocal runs AI models on the user's own device. Conversations, attached documents and images
(text is read from images on the device; a shrunk copy of each image is kept with the
conversation), device details and benchmark results stay on the device.

## Network connections (the complete list)

| When | To | What is sent | Can be turned off |
|---|---|---|---|
| The user taps *Download* for a model, or *Add image support* for one | `huggingface.co` (then a Hugging Face CDN it redirects to) | A standard HTTPS request for one pinned file, with `Range` headers when resuming. User agent `opnlocal/<version>`. | It only happens on request. |
| At most once a day, and *Check now* in Settings | `raw.githubusercontent.com` (the public repo `MilanDanilovic/opnlocal-catalog`) | Two HTTPS GET requests (`catalog.json`, `catalog.json.sig`). No parameters, no identifiers. | Settings → Privacy → "Check for new models automatically". |
| Opening a license or *Source code* link | The link's site, in the system browser | Whatever the browser sends | Only on tap. |

Never: analytics, telemetry, crash reports, accounts, advertising IDs, update pings, or sending
prompts, replies, documents, or device information anywhere.

Like any HTTPS client, these requests reveal the device's IP address to the server contacted.

## What's stored, and where

All in the app's data folder (Settings → Advanced → Storage shows the path):

- model files (`models/`), deleted with *Models → Delete*
- conversations (`conversations/*.json`), deleted individually or with *Delete all conversations*
- settings, accepted model licenses, benchmark results (`settings.json`, `benchmarks.json`)
- the newest verified model list (`catalog.json`, `catalog.json.sig`)

Uninstalling the app removes the data folder on Android and iOS. On desktop, the installer's
uninstaller removes the app; the data folder can be deleted by hand.

## Security notes

- There is no local server and no open port: the UI talks to the engine through Tauri's
  in-process IPC only. The web view loads only the app's bundled files (strict CSP, no remote
  content), and model output is sanitized before display.
- Model files are verified against the sha256 in the catalog; a mismatch deletes the file.
- The catalog is signed with ed25519; the app only accepts catalogs signed with the key built
  into it, and never an older version than the one it has.
- Web view platform components (Microsoft Edge WebView2 on Windows, WebKitGTK on Linux, the
  system WebView on Android and iOS) are provided by the operating system and follow the
  system's own settings.
