---
type: ADR
profile-version: 1
id: "REV-ADR-0024"
title: "Rasterize SVG-declared EPUB covers to PNG via resvg"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-13"
decision-makers:
  - "John Unkovich"
---

# Rasterize SVG-declared EPUB covers to PNG via resvg

## Context and problem statement

Standard Ebooks (the canonical public-domain EPUB source) declare their cover as `images/cover.svg`
(`media-type="image/svg+xml"`, `properties="cover-image"`). Two gaps combine to break these covers. Detection: covers
are located by `find_cover_href`, which matched only a few legacy manifest ids, but Standard Ebooks (and EPUB 3
generally) declare the cover via the `properties="cover-image"` attribute on an item with an arbitrary id
(`cover.svg`/`cover.jpg`), which `opf_layer` discarded. So no Standard Ebooks cover was detected at all, producing HTTP
404 (`NoCover`). A scan of the 80-book dev collection confirmed this is universal: 0/80 detected by the id heuristic;
all 80 use `properties="cover-image"` (72 SVG, 8 JPEG). Decode: even once detected, `covers::extract` calls
`image::guess_format`, which has no SVG support (upstream wontfix), so the SVG bytes fail to decode, producing HTTP 500
(`CoverError::Decode`). Either way the frontend falls back to a typographic spine; on a library of Standard Ebooks, no
real cover art renders.

This decision addresses the decode gap (rasterize SVG to PNG). The detection gap ships alongside it: `opf_layer` now
captures `properties="cover-image"` and `find_cover_href` prefers it over the id heuristic, without which rasterization
would never be reached.

Rendering SVG means executing an SVG renderer over attacker-controlled XML pulled from an uploaded EPUB. Reverie's
threat model is a multi-user, internet-exposed instance, so the renderer choice and its hardening are a decision worth
recording, not a routine dependency bump. This is also the first SVG-rendering dependency in the tree, which the
project's "ADR before a new crate" policy requires capturing.

## Decision drivers

- SVG-declared covers from the canonical public-domain source must render as real artwork.
- No new stored-XSS surface: the cover route must not start serving SVG bytes (SVG can carry script), and response
  content types must stay raster-only so the existing `nosniff` posture and per-route CSP are unaffected.
- Untrusted-XML parsing must be hardened against XXE, local-file disclosure via `<image href>`, and decode/expansion
  bombs.
- Minimal blast radius: cache keying, the resize step, and the serving handler should be reused unchanged.
- Bounded build weight: the arm64 staging host's build OOMs the GNU linker under heavy parallel builds, so a fat
  transitive tree (for example bundled fonts) is a cost.

## Considered options

- Rasterize at extraction to PNG via `resvg`: sniff SVG at the `guess_format` failure point, render to PNG, hand the PNG
  to the existing pipeline.
- Serve SVG pass-through: store and serve the SVG bytes directly.
- Prefer the raster sibling only: ignore the SVG and hunt for a sibling `cover.jpg` heuristically.

## Decision outcome

Chosen option: **rasterize at extraction to PNG via `resvg`**, because it renders the canonical covers while keeping the
serving path raster-only (no new content type, no XSS surface, no CSP change) and reusing the cache key, resize, and
serve code unchanged. The two alternatives either open an XSS surface (pass-through) or are fragile and incomplete
(sibling-hunting): see Pros and cons of the options.

Rasterisation occurs at extraction, and the result is a PNG served through the existing raster path. The choice confines
resource access to the EPUB and bounds input and render work.

### Consequences

- Positive: the canonical Standard Ebooks covers render as real artwork across the REST and OPDS cover routes.
- Positive: zero new response content types and no SVG bytes ever leave the server; the cover route's XSS surface and
  CSP are unchanged.
- Negative: a new rendering dependency (around 23 transitive crates) and untrusted-XML parsing enter the tree, mitigated
  by the bounded, EPUB-contained rendering and the maintained crate.
- Negative: SVG covers relying on live `<text>` lose that text (no `text` feature); acceptable for the canonical source
  and documented as a known limitation.
- Negative: an SVG cover that legitimately uses filters renders via the spine fallback rather than as artwork (filters
  are rejected as a render-cost bomb); acceptable, since the canonical Standard Ebooks covers use none.
- Negative: a cover whose SVG carries a DOCTYPE is rejected (spine fallback). Parsing uses `allow_dtd: false` because
  DTD entity expansion otherwise inflates nesting past the byte-scan depth guard (a stack-overflow bypass) and reopens
  XXE and billion-laughs. Real covers (Standard Ebooks and modern toolchains) emit no DOCTYPE, so the cost is borne only
  by unusual inputs.

## Pros and cons of the options

### Rasterize at extraction to PNG via `resvg`

- Positive: serving stays raster-only, so there is no XSS surface and no CSP/content-type churn.
- Positive: reuses cache keying, resize, and serve paths unchanged.
- Positive: handles both Standard Ebooks cover shapes (in-ZIP sibling reference and base64 data URI) through one
  hardened resolver.
- Negative: introduces a new rendering dependency and an untrusted-XML attack surface to harden.

### Serve SVG pass-through

- Positive: no rendering dependency.
- Negative: stored-XSS surface (SVG can carry script); needs a sanitisation story and CSP changes on the cover route.
- Negative: collapses the thumbnail/full size tiers, since resizing an SVG means rasterizing it anyway, so this does not
  avoid a renderer.

### Prefer the raster sibling only

- Positive: no SVG rendering at all.
- Negative: the sibling `cover.jpg` is not manifest-declared as the cover; heuristic sibling-hunting is fragile and
  misses the base64 data-URI variant entirely.
- Negative: the in-ZIP href resolver in the chosen option subsumes this case cleanly, so the heuristic adds fragility
  for no coverage gain.

## More information

- Render hardening references: `usvg`'s `Options::image_href_resolver`,
  [resvg#647](https://github.com/linebender/resvg/issues/647) (no built-in dimension limits), and the pinned 0.47 parser
  source; neither `roxmltree` nor `usvg` guards element-nesting recursion, so parsing uses `allow_dtd: false` and
  nesting is bounded on the raw bytes.
- Security checklists consulted:
  [XML and serialization](../../docs/security/codeguard/codeguard-0-xml-and-serialization.md),
  [file handling and uploads](../../docs/security/codeguard/codeguard-0-file-handling-and-uploads.md); threat-model
  stance: the multi-user exposed instance.
- Sibling ADR:
  [Backend auxiliary crates: axum-extra, serde_with, and subtle](./0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md).
- Dependency declaration: [`backend/Cargo.toml`](../../backend/Cargo.toml).
