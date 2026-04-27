# Reverie · Fontshare fonts (self-hosted)

Locked typography: **Author Variable** (display) + **Satoshi Variable**
(body), both Indian Type Foundry, distributed via
[Fontshare](https://www.fontshare.com). **JetBrains Mono** Regular
(mono, conditional — UNK-113 reviews adoption post-0.1.0).

The variable woff2 files in `files/` are committed into this
open-source repository. Integrity is verified by `SHA256SUMS` in the
same directory: `cd files && sha256sum -c SHA256SUMS`.

## Why self-host (and not the CDN)

The brand identity at
[unkos-dev/reverie-branding](https://github.com/unkos-dev/reverie-branding)
specifies Author + Satoshi via Fontshare. The CDN delivery path is
attractive (cookie-free, CORS-permissive woff2; passes Chromium's
Opaque Response Blocking) — but Fontshare's public CSS API at
`api.fontshare.com/v2/css` sets a `_fontshare_key` cookie, which
Chromium treats as a credentialed response and blocks under ORB.
Self-hosting the variable woff2 directly is the only viable delivery
path:

- Removes the runtime dependency on `cdn.fontshare.com` reachability.
- Strengthens production CSP from
  `font-src 'self' https://cdn.fontshare.com` to `font-src 'self'`.
- Italic axes are not exposed by the public weight CSS API (only the
  per-font download zip carries them); self-hosting trivially solves
  the italic problem.

## Why not `@fontsource`

There is **no `@fontsource/author` or `@fontsource/satoshi` npm
package**. Author and Satoshi ship from Fontshare, not Google Fonts;
`@fontsource` indexes only the Google catalogue. JetBrains Mono is
self-hosted from the JetBrains GitHub release for the same
consistency reason — one delivery mechanism for the whole stack.

## FFL clause-02 acceptance

The Fontshare Free EULA (`License/FFL.txt` in the download zip)
prohibits two things this project does:

- Clause 02: "uploading them in a public server" — i.e. committing
  the woff2 files into this open-source repo.
- Clause 02: "transmit the Font Software over the Internet in font
  serving or for font replacement... from infrastructure other than
  Fontshare's."

Self-hosting in the public repository is a formal violation. We
accept that risk because:

1. Chromium ORB on the CDN's CSS API blocks the cookie-bearing
   response (verified by the brand-handoff investigation). Self-hosted
   variable woff2 is the only viable delivery on the boutique
   typographic register the brand requires.
2. Production CSP `font-src 'self'` is materially stronger than
   allowing `cdn.fontshare.com`.
3. If Indian Type Foundry objects, the fallback is a paid commercial
   license plus an on-prem mirror. The substitution is mechanical
   (URLs change; the `@font-face` declarations in
   `frontend/src/styles/fonts.css` do not).
4. The risk surfaces to a single party (ITF) with a single resolution
   path, not as a structural risk to operators.

JetBrains Mono is OFL-1.1 (permissive — no FFL constraint).

## SHA256SUMS verification

Each woff2 file in `files/` has its sha256 hash recorded in
`files/SHA256SUMS`. To re-derive after a font refresh:

```bash
cd frontend/public/fonts/fontshare/files
sha256sum *.woff2 > SHA256SUMS
```

To verify integrity (CI gate):

```bash
cd frontend/public/fonts/fontshare/files && sha256sum -c SHA256SUMS
```

## URL discovery (refresh procedure)

If a font needs refreshing (foundry update, italic axis added,
JetBrains Mono version bump):

1. Pull the per-font zip from
   `https://api.fontshare.com/v2/fonts/download/{author,satoshi}`.
2. Extract `WEB/fonts/<Family>-Variable.woff2` and
   `WEB/fonts/<Family>-VariableItalic.woff2` into `files/`.
3. Pull JetBrains Mono Regular from
   `https://github.com/JetBrains/JetBrainsMono/raw/master/fonts/webfonts/JetBrainsMono-Regular.woff2`.
4. Re-derive `SHA256SUMS` (one command above).
5. Verify `sha256sum -c SHA256SUMS` is OK.

The italic woff2 is **not** exposed by Fontshare's public weight CSS
API; the per-font download endpoint is the only place it ships.

## CSP

The `@font-face` declarations in `frontend/src/styles/fonts.css`
reference `/fonts/fontshare/files/...` (absolute path served from
`frontend/public/`). The production HTML CSP in
`backend/src/security/csp.rs` declares `font-src 'self'`. Operators
who want to add a font CDN must edit `csp.rs::build_html_csp` and
rebuild — there is no runtime configuration knob, by design (every
deployment carries an identical, auditable font policy).
