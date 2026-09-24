# Security

## Reporting a vulnerability

Please report privately: on GitHub, open the repository's **Security** tab and choose
**Report a vulnerability**. Don't open a public issue for security problems.

Fixes go into the next release, and reporters are credited in the release notes if they wish.

Only the latest release is supported.

## What matters most

opnlocal promises that chats stay on the device and that only verified files are installed.
Reports in these areas are especially welcome:

- **Catalog signing**: a way to make an official build accept a catalog not signed with the
  trusted key, or an older catalog than the one it has (see [docs/CATALOG.md](docs/CATALOG.md)).
- **Download verification**: a way to get a model file installed whose sha256 doesn't match
  the catalog.
- **Local attack surface**: the UI and engine talk only through Tauri's in-process IPC and
  there is no local server. Anything that lets another app, web page or a model's output reach
  that IPC, run code or read files.
- **Untrusted input**: model files (GGUF headers, chat templates), attached documents (PDF,
  DOCX) and model replies (rendered as sanitized Markdown).
- **Privacy**: any network request beyond those listed in [docs/PRIVACY.md](docs/PRIVACY.md).

Out of scope: the content or quality of model answers, and issues in upstream projects that
aren't reachable through opnlocal (report those upstream).
