# Fontshare self-hosted woff2 files

D2-spike-only. The CSS lives at
`frontend/src/design/explore/midnight-gold/fontshare.css`; the woff2 files
live here under `files/` and are referenced by the CSS via absolute paths
(`/fonts/fontshare/files/<short>.woff2`).

## Why self-hosted

Fontshare's CDN sets a `_fontshare_key` cookie on its CSS responses, which
trips Chromium's Opaque Response Blocking (`ERR_BLOCKED_BY_ORB`) on
cross-origin `<link>` loads — even without `crossorigin`. Self-hosting
bypasses ORB and matches the production CSP (`font-src 'self'`).

## Fonts included

Regular weights only (italics aren't on Fontshare's public weight API):

- Clash Display: 400, 500, 600, 700
- Satoshi: 400, 500, 700
- Author: 400, 500, 700
- Synonym: 400, 500, 700

## Italics

Synthetic italic (browser slants the regular face) is used in the spike for
Author and Synonym. Author's true cursive italic isn't accessible via the
public Fontshare weight API. If the chosen font pair includes either Author
or Synonym, D3 task 20 will need to manually grab the italic woff2 files
from the full font pack download on Fontshare's website and add a
corresponding `font-style: italic` `@font-face` rule to the CSS.

## D3.1 cleanup

This entire directory and the picker UI get pruned in D3.1. D3 task 20
keeps only the chosen pair's woff2 files (renamed from short hashes to
human-readable names like `clash-display-500.woff2`) and writes a clean
`@font-face` block in `frontend/src/styles/themes/`.
