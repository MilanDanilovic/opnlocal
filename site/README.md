# Landing page

A static page: `index.html`, `style.css`, `main.js`, `og.png` (share image) and `icon.png`.
No build step, no backend, no analytics, no cookies. The demo recording is `docs/images/demo.gif`
in this repository and is copied next to the page when deploying, so it isn't stored twice.

The download button picks the visitor's platform in the browser (user agent only) and points at
the `releases/latest/download/<stable name>` links, which always resolve to the newest release.

Preview locally: open `index.html` in a browser after copying `docs/images/demo.gif` into this
folder (or run `python -m http.server` here).

## Deploy to Cloudflare Pages (current: https://opnlocal.pages.dev/)

A Pages project connected to the repository, deploying on every push to `main`, with:

| Setting | Value |
|---|---|
| Build command | `cp docs/images/demo.gif site/` |
| Build output directory | `site` |

The canonical URL and the `og:image` / `twitter:image` URLs in `index.html` point at this address;
change them if the site moves.

## Deploy to GitHub Pages instead

Repository *Settings → Pages → Source: GitHub Actions*, then a workflow that copies this folder
plus `docs/images/demo.gif` into one directory and uploads it with `actions/upload-pages-artifact`
and `actions/deploy-pages`.
