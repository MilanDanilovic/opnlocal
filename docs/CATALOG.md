# Catalog trust

The code is open, but the model list that official builds use is not open to anyone to change.

- The list (`catalog.json`) says which models exist, where to download each one (a pinned
  Hugging Face commit) and its sha256. The app downloads only files that match those hashes.
- Official builds trust exactly one ed25519 public key, compiled into the app
  (`TRUSTED_KEYS` in `crates/engine/src/catalog.rs`). A catalog is accepted only if
  `catalog.json.sig` is a valid signature from that key, and only if its `version` is newer
  than the one the app already has, so an old list can't be replayed.
- The private key is held by the maintainer and never enters the repository. Changing the
  public repo `MilanDanilovic/opnlocal-catalog` without the key does nothing: installed apps
  reject the unsigned change and keep the list they have.
- A copy of the catalog is built into the app, so it works offline and without trusting the
  network at all.

## Using your own catalog in a fork

1. Make a key: `openssl genpkey -algorithm ed25519 -out my-catalog.pem`
   (keep it out of your repo).
2. Get its public key in the form the app expects (raw 32 bytes, base64):
   `openssl pkey -in my-catalog.pem -pubout -outform DER | tail -c 32 | base64`
3. In `crates/engine/src/catalog.rs`, replace `TRUSTED_KEYS` with your public key and point
   `CATALOG_URL` / `SIGNATURE_URL` at where you will publish your files.
4. Edit `catalog/source.json`, then build and sign:
   `cargo run -p catalog-tool --release -- build` and
   `cargo run -p catalog-tool --release -- sign my-catalog.pem`
   (`sign` refuses a key that isn't in `TRUSTED_KEYS`).
5. Publish `catalog/catalog.json` and `catalog/catalog.json.sig` at those URLs.

Also change the app identifier (`identifier` in `app/src-tauri/tauri.conf.json`) so your build
installs next to official builds instead of replacing them.

Found a way around these checks? Please report it privately; see [SECURITY.md](../SECURITY.md).
