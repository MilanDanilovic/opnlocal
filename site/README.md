# Landing page

A static page: `index.html`, `style.css`, `main.js`, `demo.gif` (the recording the README shows
too), `og.png` (share image) and `icon.png`. No build step, no backend, no analytics, no cookies.

The download button picks the visitor's platform in the browser (user agent only) and points at
the `releases/latest/download/<stable name>` links, which always resolve to the newest release.

Preview locally: open `index.html` in a browser (or run `python -m http.server` here).

## Deploy to Cloudflare Pages (current: https://opnlocal.pages.dev/)

A Pages project connected to the repository, deploying on every push to `main`, with:

| Setting | Value |
|---|---|
| Build command | (none) |
| Build output directory | `site` |

The canonical URL and the `og:image` / `twitter:image` URLs in `index.html` point at this address;
change them if the site moves.

## Deploy to GitHub Pages instead

Repository *Settings → Pages → Source: GitHub Actions*, then a workflow that uploads this folder
with `actions/upload-pages-artifact` and `actions/deploy-pages`.
