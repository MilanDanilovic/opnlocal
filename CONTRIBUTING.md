# Contributing

Thanks for helping. opnlocal is for people who have never heard of VRAM or quantization, so
changes that make it simpler, clearer or more reliable are the most welcome.

## Build and run

Full per-platform setup: [docs/BUILDING.md](docs/BUILDING.md). Quick start:

```sh
cd app
npm ci
npx tauri dev          # desktop app with hot reload
npm run dev            # UI only, in a browser, with a built-in test backend
```

On Windows set `CARGO_TARGET_DIR` to a short path such as `C:\ot` first (the Vulkan shader
build fails past the 260-character path limit).

## Test

Run the tests for what you changed ([docs/TESTING.md](docs/TESTING.md) has the full list):

```sh
cargo test -p opnlocal-engine --release     # engine
cd app && npm run check && npm test         # UI types and unit tests
cd app && npx playwright test               # UI journeys and accessibility checks
```

## Pull requests

- One topic per pull request, with a short description of what and why.
- Add or update tests where behavior changes. Keep them focused.
- User-facing text goes in `app/src/lib/strings.ts`, in plain words.
- Never add network requests, analytics or telemetry. The list of allowed connections is in
  [docs/PRIVACY.md](docs/PRIVACY.md); changing it needs discussion first.
- New dependencies must have permissive licenses. `scripts/generate-notices.mjs` fails on
  anything outside `about.toml`'s allow-list.
- Model catalog changes: edit `catalog/source.json` and explain why; the maintainer builds,
  verifies and signs the catalog ([docs/CATALOG.md](docs/CATALOG.md)).

## License

By submitting a contribution, you agree it is licensed under the
[Apache License 2.0](LICENSE), the same as the project (Apache-2.0, section 5).
