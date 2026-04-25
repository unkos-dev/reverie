# Reverie · self-hosted Fontshare fonts

Locked typography: **Author** (display) + **Satoshi** (body). Variable woff2
files for both faces, plus their italic counterparts, live in `files/`. The
CSS at `frontend/src/design/explore/midnight-gold/fontshare.css` references
them via absolute paths (`/fonts/fontshare/files/<name>.woff2`).

## Why self-hosted

Fontshare's CDN sets a `_fontshare_key` cookie on its CSS responses, which
trips Chromium's Opaque Response Blocking (`ERR_BLOCKED_BY_ORB`) on
cross-origin `<link>` loads — even without `crossorigin`. Self-hosting
bypasses ORB and matches the production CSP (`font-src 'self'`).

## Files

- `Author-Variable.woff2` — weights 400–700, normal style
- `Author-VariableItalic.woff2` — weights 400–700, italic style (real
  cursive italic; this is the gesture that the gold-italic accents at the
  hero, title, and stat-numerals depend on)
- `Satoshi-Variable.woff2` — weights 400–700, normal style
- `Satoshi-VariableItalic.woff2` — weights 400–700, italic style

Black weights (900) are deliberately not loaded — they fight the boutique
register per the project lead's handoff notes.

## How they were obtained

```sh
curl -sL https://api.fontshare.com/v2/fonts/download/author -o /tmp/author.zip
curl -sL https://api.fontshare.com/v2/fonts/download/satoshi -o /tmp/satoshi.zip
unzip /tmp/author.zip -d /tmp/author
unzip /tmp/satoshi.zip -d /tmp/satoshi
cp /tmp/author/Author_Complete/Fonts/WEB/fonts/Author-Variable.woff2 files/
cp /tmp/author/Author_Complete/Fonts/WEB/fonts/Author-VariableItalic.woff2 files/
cp /tmp/satoshi/Satoshi_Complete/Fonts/WEB/fonts/Satoshi-Variable.woff2 files/
cp /tmp/satoshi/Satoshi_Complete/Fonts/WEB/fonts/Satoshi-VariableItalic.woff2 files/
```

The italic woff2 files **cannot** be obtained from Fontshare's public
weight API (`f[]=author@400i` returns 500; `author-italic` slug returns
empty). The `download/<font>` endpoint is the only public source for the
italic variable axis.

## D3 follow-up

D3 task 20 will move these files to the canonical asset path (likely
`frontend/src/assets/fonts/`) and inline the `@font-face` block into the
canonical theme CSS at `frontend/src/styles/themes/index.css`. The picker
UI is already gone — only the two locked faces remain.
