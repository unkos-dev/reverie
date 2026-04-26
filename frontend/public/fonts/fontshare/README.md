# Reverie · Fontshare fonts

Locked typography: **Author** (display) + **Satoshi** (body), both Indian Type
Foundry, distributed via [Fontshare](https://www.fontshare.com).

## Why this directory exists

Reverie's `@font-face` block (at
`frontend/src/design/explore/midnight-gold/fontshare.css`) points `src` at
Fontshare's CDN (`cdn.fontshare.com`). Fontshare-hosted woff2 responses
are cookie-free, CORS-permissive (`access-control-allow-origin: *`), and
served with the correct `font/woff2` MIME — they pass Chromium's Opaque
Response Blocking on cross-origin loads.

The Fontshare *CSS API* at `api.fontshare.com/v2/css` does **not** pass
ORB — its CSS responses set a `_fontshare_key` cookie, which Chromium
treats as evidence of a credentialed response and blocks. That's why we
self-author the `@font-face` block and skip the Fontshare CSS API
entirely.

## Why we don't self-host the woff2

The Fontshare Free EULA (FFL — `License/FFL.txt` in any Fontshare
download zip) prohibits two things relevant to this project:

- Clause 02: "uploading them in a public server" — committing the woff2
  files into this open-source repo.
- Clause 02: "transmit the Font Software over the Internet in font
  serving or for font replacement... without the prior written consent
  of the Licensor" — serving the woff2 files from Reverie's own
  infrastructure.

Fontshare's CDN is the EULA's intended delivery path for free use.
Self-hosting required either a paid commercial license or written
consent from ITF. Using their CDN avoids both.

## URL discovery

Fontshare's public CSS API (`api.fontshare.com/v2/css?f[]=author@400`)
exposes static-weight woff2 URLs but **not** variable-axis URLs. The
variable-axis URLs in `fontshare.css` were extracted from Fontshare's
own marketing pages.

If the URLs stop resolving (Fontshare rotates the CDN paths), re-run the
discovery:

1. Load `https://www.fontshare.com/fonts/{author,satoshi}` in a browser.
2. Capture the `cdn.fontshare.com/wf/.../*.woff2` requests via DevTools
   or Playwright `browser_network_requests`.
3. Identify the variable-axis files by content-length match against the
   `WEB/fonts/*.woff2` files in the Fontshare distribution zip
   (`api.fontshare.com/v2/fonts/download/{author,satoshi}`):
   - Author-Variable.woff2 — 37,080 bytes
   - Author-VariableItalic.woff2 — 40,900 bytes
   - Satoshi-Variable.woff2 — 42,588 bytes
   - Satoshi-VariableItalic.woff2 — 43,844 bytes
4. Verify by sha256 against the same zip files.
5. Update `fontshare.css` with the new URLs.

## CSP

Reverie's production HTML CSP (built in `backend/src/security/csp.rs`)
must include `https://cdn.fontshare.com` in `font-src`. The dev CSP in
`frontend/vite.config.ts` carries the same allowance. Any change to the
font origin requires updating both.

## Operational notes

- Runtime delivery depends on `cdn.fontshare.com` being reachable.
  Acceptable for Reverie's intended deployment (online self-host); a
  fully-offline deployment would need a paid license + on-prem mirror.
- Variable axes (one woff2 per face × normal/italic = 4 total) instead
  of static-weight stacks (8+ files per face).
- Black weights (900) are deliberately not loaded — they fight the
  boutique register per the project lead's handoff notes.
- D3 task 20 will inline the `@font-face` block into the canonical
  theme CSS at `frontend/src/styles/themes/`.
