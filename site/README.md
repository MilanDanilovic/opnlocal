# Landing page

A static page: `index.html`, `style.css`, `main.js`, `og.png` (share image) and `icon.png`.
No build step, no backend, no analytics, no cookies. The demo recording is `docs/images/demo.gif`
in this repository and is copied next to the page when deploying, so it isn't stored twice.

The download button picks the visitor's platform in the browser (user agent only) and points at
the `releases/latest/download/<stable name>` links, which always resolve to the newest release.

Preview locally: open `index.html` in a browser after copying `docs/images/demo.gif` into this
folder (or run `python -m http.server` here).

## Deploy to GitHub Pages (current: https://milandanilovic.github.io/opnlocal/)

`.github/workflows/pages.yml` publishes this folder on every push to `main` that touches it.
One-time setup: repository *Settings → Pages → Build and deployment → Source: GitHub Actions*
(or `gh api -X POST repos/<owner>/<repo>/pages -f build_type=workflow`).

## Deploy to Cloudflare Pages

Create a Pages project connected to the repository with:

| Setting | Value |
|---|---|
| Build command | `cp docs/images/demo.gif site/` |
| Build output directory | `site` |

Then change `<link rel="canonical">` and the `og:image` / `twitter:image` URLs in `index.html`
to the new address (for example `https://opnlocal.pages.dev/`).
