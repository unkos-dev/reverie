---
status: accepted
date: 2026-06-13
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Rasterize SVG-declared EPUB covers to PNG via resvg

## Context and Problem Statement

Standard Ebooks — the canonical public-domain EPUB source — declare their cover
as `images/cover.svg` (`media-type="image/svg+xml"`, `properties="cover-image"`).
Two gaps combine to break these covers. **Detection:** covers are located by
`find_cover_href`, which matched only a few legacy manifest ids — but Standard
Ebooks (and EPUB 3 generally) declare the cover via the `properties="cover-image"`
attribute on an item with an arbitrary id (`cover.svg`/`cover.jpg`), which
`opf_layer` discarded. So no SE cover was detected at all → HTTP 404
(`NoCover`). A scan of the 80-book dev collection confirmed this is universal:
0/80 detected by the id heuristic; all 80 use `properties="cover-image"` (72 SVG,
8 JPEG). **Decode:** even once detected, `covers::extract` calls
`image::guess_format`, which has no SVG support (upstream wontfix), so the SVG
bytes fail to decode → HTTP 500 (`CoverError::Decode`). Either way the frontend
falls back to a typographic spine; on a library of Standard Ebooks, _no_ real
cover art renders.

This decision addresses the decode gap (rasterize SVG → PNG). The detection gap
ships in the same PR: `opf_layer` now captures `properties="cover-image"` and
`find_cover_href` prefers it over the id heuristic — without it, rasterization
would never be reached.

Rendering SVG means executing an SVG renderer over **attacker-controlled XML**
pulled from an uploaded EPUB. Reverie's threat model is a multi-user, internet-
exposed instance, so the renderer choice and its hardening are a decision worth
recording, not a routine dependency bump. This is also the first SVG-rendering
dependency in the tree, which CLAUDE.md's "ADR before a new crate" trigger
requires capturing. Tracked as [UNK-406](https://linear.app/unkos/issue/UNK-406).

## Decision Drivers

- SVG-declared covers from the canonical public-domain source must render as real
  artwork.
- No new stored-XSS surface: the cover route must not start serving SVG bytes
  (SVG can carry script), and response content-types must stay raster-only so the
  existing `nosniff` posture and per-route CSP are unaffected.
- Untrusted-XML parsing must be hardened against XXE, local-file disclosure via
  `<image href>`, and decode/expansion bombs.
- Minimal blast radius: cache keying, the resize step, and the serving handler
  should be reused unchanged.
- Bounded build weight — `oci-compute-1` OOMs the GNU linker under heavy parallel
  builds, so a fat transitive tree (e.g. bundled fonts) is a cost.

## Considered Options

- **Rasterize at extraction to PNG via `resvg`** — sniff SVG at the
  `guess_format` failure point, render to PNG, hand the PNG to the existing
  pipeline.
- **Serve SVG pass-through** — store and serve the SVG bytes directly.
- **Prefer the raster sibling only** — ignore the SVG and hunt for a sibling
  `cover.jpg` heuristically.

## Decision Outcome

Chosen option: **rasterize at extraction to PNG via `resvg`**, because it renders
the canonical covers while keeping the serving path raster-only (no new
content-type, no XSS surface, no CSP change) and reusing the cache key, resize,
and serve code unchanged. The two alternatives either open an XSS surface
(pass-through) or are fragile and incomplete (sibling-hunting) — see Pros and
Cons.

Concrete shape of the decision:

- **Crate:** `resvg = { version = "0.47", default-features = false, features = ["raster-images"] }`
  (pulls `usvg`/`tiny-skia`/`roxmltree` + low-level raster codecs; ~23 crates).
  `resvg` is the only maintained pure-Rust headless SVG rasterizer, Linebender-
  stewarded, MSRV 1.87, Apache-2.0/MIT, zero RUSTSEC advisories as of 2026-06.
- **`text` feature OFF.** Standard Ebooks convert cover text to vector paths at
  build time, so text rendering is unnecessary; omitting it avoids
  `fontdb`/`rustybuzz`/`ttf-parser` (~+400 KiB) and the bundled-font-in-Docker
  problem. Accepted degradation: an SVG relying on live `<text>` renders without
  that text.
- **Render target = 1200 px long edge** (= `CoverSize::Full`), so the downstream
  resize is a near-no-op. Output is PNG; `ext_for_format(Png)` and the `"png"`
  cache/content-type arms already exist.
- **Hardened, no-IO resolver.** `usvg`'s default `resolve_string` reads local
  files; it is replaced so `<image href>` resolves _only_ to path-checked sibling
  entries inside the same EPUB ZIP. `resolve_data` (base64 data-URIs) is _also_
  overridden so both raster ingress paths enforce the same caps.
- **Decode-bomb caps.** `resvg`'s raster decoders have no built-in size limits
  ([resvg#647](https://github.com/linebender/resvg/issues/647)). Every embedded
  raster is bounded by byte size and a header-sniffed megapixel cap before
  decode; raw SVG input is capped before parse; render output is capped at the
  long edge (`Pixmap::new` over/zero-size → error).
- **Parser stack overflow.** Both `roxmltree`'s parser **and** `usvg`'s tree
  conversion recurse on element nesting with no depth guard and _abort the
  process_ (stack overflow, uncatchable) on deeply nested SVG — observed at a few
  hundred levels on a 2 MiB stack, reachable by a few-KB cover. Because the parser
  itself overflows, the depth bound runs on the **raw bytes before any parse**: a
  flat byte scan (no parse, no recursion — so it cannot itself overflow) rejects
  nesting past a conservative cap that sits inside the empirically verified-safe
  range (depth-50 parses and renders; depth-1000 overflows without the guard).
  Only then is the SVG parsed (`roxmltree`) and converted (`from_xmltree`).
- **Render-cost cap.** `resvg` has no render-time limits, and two axes escape the
  output-size cap: filters (the filter buffer is allocated to the filter region,
  not the canvas — a crafted `userSpaceOnUse` region + large `feGaussianBlur` is
  a CPU/memory bomb) and vector tessellation (cost scales with segment count,
  independent of output size). Before `resvg::render`, a complexity gate walks the
  tree (`children`, the _full_ `clip-path`/`mask` chains, and every node's
  paint-server / layout sub-trees via `Node::subroots` — a filter can hide inside
  a `<pattern>` fill or a second-level chained `<mask>`) and rejects covers using
  any filter primitive, or exceeding the total path-segment or node-count budgets.
- **Blank-output guard.** `usvg` silently drops unresolvable `<image>` nodes and
  "succeeds" with a transparent pixmap; an all-transparent render is rejected so
  the `<img onError>` spine fallback is preserved.
- **Validator (Layer 5) parses + gates, does not rasterize.** A parseable SVG
  cover within the render-cost budget is not flagged; one that breaches it
  (filters, or geometry/node/depth over budget) is flagged `Degraded` at
  ingestion. Acceptance and serve-time apply the _same_ parse-and-render-cost
  gate, so a cover that clears ingestion will not later fail at serve _on those
  gates_. The serve path additionally rasterizes and rejects an all-transparent
  result, so an SVG that parses and is within budget but resolves to no visible
  pixels (an `<image>` whose sibling is absent, or an empty `<svg>`) is accepted
  at ingestion yet falls back to the spine at serve — both correctly avoid
  serving a blank cover, by different paths.

### Consequences

- Good — the canonical Standard Ebooks covers render as real artwork across the
  REST and OPDS cover routes.
- Good — zero new response content-types and no SVG bytes ever leave the server;
  the cover route's XSS surface and CSP are unchanged.
- Good — cache key, resize, and serve code are reused verbatim; the only new
  surface is one rasterization module behind the extraction boundary.
- Bad — a new rendering dependency (~23 transitive crates) and untrusted-XML
  parsing enter the tree. Mitigated by the hardening above and the maintained,
  RUSTSEC-clean crate.
- Bad — SVG covers relying on live `<text>` lose that text (no `text` feature).
  Acceptable for the canonical source; documented as a known limitation.
- Bad — an SVG cover that legitimately uses filters renders via the spine
  fallback rather than as artwork (filters are rejected as a render-cost bomb).
  Acceptable: the canonical Standard Ebooks covers use none.
- Bad — a cover whose SVG carries a DOCTYPE is rejected (spine fallback). We parse
  with `allow_dtd: false` because DTD entity expansion otherwise inflates nesting
  past the byte-scan depth guard (a stack-overflow bypass) and reopens XXE /
  billion-laughs. Real covers (Standard Ebooks and modern toolchains) emit no
  DOCTYPE, so the cost is borne only by unusual inputs.
- Neutral — existing rows ingested before this change keep their `degraded`
  status until re-ingested or revalidated; covers render regardless, since
  serving never gates on `validation_status`. Bulk revalidation is a follow-up.

### Confirmation

Load-bearing invariants, enforced by unit tests in `covers::svg`:

- **No filesystem access from SVG parsing** — the default file-reading
  `resolve_string` is overridden; `<image href>` resolves only to
  path-traversal-checked siblings (`blocks_external_path_href`,
  `blocks_traversal_href`). XXE is structurally impossible: `roxmltree` performs
  no IO, so external entities are never fetched (`dtd_entities_are_inert`).
- **Decode/expansion bombs are bounded before decode** — byte, megapixel, and
  raw-input caps (`rejects_oversized_sibling_image`, `rejects_huge_pixel_sibling`,
  `rejects_oversized_svg_input`); output dimensions are capped.
- **No silent transparent covers** — all-transparent renders return
  `Decode`, preserving the spine fallback (`blank_svg_yields_decode_error`).
- **Deep nesting cannot crash the process** — nesting depth is bounded by a
  raw-byte scan before any parse (both the parser and the converter recurse); a
  1000-deep cover is rejected, not aborted, and a moderately nested cover still
  renders (`rejects_deeply_nested_svg_without_crashing`,
  `accepts_moderately_nested_svg`).
- **Render cost is bounded before render** — filter primitives are rejected
  (including those hidden in a mask or a `<pattern>` paint server) and
  path-segment and node budgets enforced, at serve _and_ ingestion
  (`rejects_svg_with_filter`, `rejects_filter_hidden_in_mask`,
  `rejects_filter_in_pattern`, `rejects_filter_in_chained_mask`,
  `rejects_segments_in_chained_clip`, `rejects_excessive_path_segments`,
  `rejects_excessive_node_count`, `filtered_svg_cover_emits_degraded`).
- **No DTD / entity-expansion surface** — DTDs are disabled, so a DOCTYPE-bearing
  cover (the prerequisite for entity-inflated nesting and XXE) is rejected at
  parse (`rejects_svg_with_doctype`, `dtd_entities_are_inert`).
- **The canonical input renders** — a real Standard Ebooks `cover.svg` passes the
  full gate and rasterizes (`accepts_real_standard_ebooks_cover`).
- **Raster-only responses** — rasterization happens at extraction; only PNG flows
  to the cache and the serving handler.

## Pros and Cons of the Options

### Rasterize at extraction to PNG via `resvg`

- Good — serving stays raster-only: no XSS surface, no CSP/content-type churn.
- Good — reuses cache keying, resize, and serve paths unchanged.
- Good — handles both SE cover shapes (in-ZIP sibling ref and base64 data-URI)
  through one hardened resolver.
- Bad — new rendering dependency and untrusted-XML attack surface to harden.

### Serve SVG pass-through

- Good — no rendering dependency.
- Bad — stored-XSS surface (SVG carries script); needs a sanitization story and
  CSP changes on the cover route.
- Bad — collapses the thumb/full size tiers — resizing an SVG means rasterizing
  it anyway, so this does not actually avoid a renderer.

### Prefer the raster sibling only

- Good — no SVG rendering at all.
- Bad — the sibling `cover.jpg` is not manifest-declared as the cover; heuristic
  sibling-hunting is fragile and misses the base64 data-URI variant entirely.
- Bad — the in-ZIP href resolver in the chosen option subsumes this case cleanly,
  so the heuristic adds fragility for no coverage gain.

## More Information

- Render hardening references: `usvg` `Options::image_href_resolver`,
  [resvg#647](https://github.com/linebender/resvg/issues/647) (no built-in
  dimension limits), and the pinned 0.47 parser source — neither `roxmltree` nor
  `usvg` guards element-nesting recursion, so we parse with `allow_dtd: false` and
  bound nesting on the raw bytes ourselves.
- Security checklists consulted:
  [`docs/security/codeguard/codeguard-0-xml-and-serialization.md`](../docs/security/codeguard/codeguard-0-xml-and-serialization.md),
  [`docs/security/codeguard/codeguard-0-file-handling-and-uploads.md`](../docs/security/codeguard/codeguard-0-file-handling-and-uploads.md);
  threat-model stance in
  [`project_open_source_security_stance`](../.claude/projects/-home-coder-reverie/memory/project_open_source_security_stance.md).
- As-built note: the implementation hardens `resolve_data` (base64 data-URIs) in
  addition to `resolve_string`, going one step beyond the original plan, so the
  byte/megapixel caps cover both raster-ingress paths.
- Follow-ups (out of scope here, candidate Linear issues): the dashboard
  "Cover %" metric counts enrichment sidecars, not embedded covers, and will not
  move with this change; the ~existing SVG-cover rows stay `degraded` until a
  bulk revalidation ships.
- Sibling dependency-adoption ADR shape:
  [`2026-05-22-backend-aux-crates.md`](2026-05-22-backend-aux-crates.md).
- Linear: [UNK-406](https://linear.app/unkos/issue/UNK-406).
