---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0015"
title: "Covers"
satisfies:
  - "REV-REQ-0048"
  - "REV-REQ-0049"
  - "REV-REQ-0050"
governed-by:
  - "REV-ADR-0024"
  - "REV-ADR-0025"
---

# Covers

This Design covers the pipeline that serves a book's cover art: detecting and extracting the cover image embedded in an
EPUB archive, rasterising an SVG-declared cover to PNG under a set of hardening guards, resizing into two size tiers
with tier-dependent encoding, caching the result on disk under a content-addressed key, serving it from two HTTP mounts
with a strong validator and cache headers, and the client's fallback to a generated cloth-bound spine when a cover fails
to load.

## Purpose and boundaries

This subject owns: cover-byte extraction from an EPUB archive, including the fallback into SVG rasterisation when the
raster decoder cannot read the declared cover (`backend/src/services/covers/extract.rs::extract_cover_bytes`); the
hardened SVG-to-PNG rasteriser and every guard around it: the input-byte cap, the raw-byte nesting-depth bound, the
DTD-disabled parse, the hardened image-href resolver bounding per-image and cumulative decode cost and
sibling-resolution count, the render-cost gate rejecting filters and over-budget geometry, and the blank-render
rejection (`backend/src/services/covers/svg.rs`); the two size tiers and their tier-dependent encoding, JPEG for
thumbnails and the source format for the full tier (`backend/src/services/covers/resize.rs`); the content-addressed
on-disk cache and its key derivation (`backend/src/services/covers/cache.rs`); the request-path generate-or-serve flow
and the shared extract-resize-write pipeline both writers call (`backend/src/services/covers/mod.rs::get_or_create`,
`generate_into_cache`); the background pre-warm mechanism (`spawn_warm_thumb`, `warm_one`, the `WARM_LIMIT` semaphore);
the two HTTP mounts and the cache headers, strong `ETag`, and conditional-request handling they share
(`backend/src/routes/opds/covers.rs`); and, on the client, the generated cloth-bound spine `CoverArtwork` renders for a
missing or failed cover image (`frontend/src/components/CoverArtwork.tsx` and its two call sites, the library grid and
the book detail page).

It does not own: the archive structural validation, container and OPF parsing, and cover-href detection this subject
calls into on every extraction, which belongs to the Design "EPUB validation and repair"; this subject only reuses their
output and maps an irrecoverable structural failure to the same outcome as an archive with no cover. It does not own the
reverse call: the Design "EPUB validation and repair" describes a cover-usability check that calls back into this
subject's SVG rasteriser and its resolver-free parse-and-gate check to decide, at ingestion, whether a manifestation's
cover renders. This subject supplies the routine, not the ingestion-time verdict built on it. It does not own the
`has_embedded_cover` column, its ingestion-time write, or the predicate that decides whether an ingest invokes this
subject's pre-warm mechanism: those belong to the Design "Ingestion pipeline"; this subject owns only the mechanism the
predicate calls. It does not own the writeback rewrite of `manifestations.current_file_hash`, which re-keys this
subject's cache by changing the hash prefix every cache-key derivation reads, or the corresponding writeback-time
refresh of `has_embedded_cover`: those belong to the Design "Writeback pipeline". It does not own the
enrichment-downloaded sidecar cover at `manifestations.cover_path`, the SSRF-guarded remote-cover HTTP client, its
download staging under `_covers/pending` and `_covers/accepted`, or its configuration surface (`REVERIE_COVER_MAX_BYTES`
and the sibling settings in `backend/src/config/cover.rs`): those belong to the Design "Enrichment pipeline"; that cover
is a distinct artefact from the EPUB-embedded cover this subject serves, a boundary the module doc at the top of
`covers/mod.rs` states directly. It does not own row-level security or the `acquire_with_rls` GUC contract this
subject's manifestation lookup runs inside: that belongs to the Design "Row-level security and database context". It
does not own `BasicOnly` or `CurrentUser` credential resolution, which the two HTTP mounts gate behind: that belongs to
the Design "Request authentication". It does not own the uniform security headers or the API Content-Security-Policy
every cover response also carries: those belong to the Design "Response security headers and CSP". It does not own the
strong-`ETag`/`If-Match` contract for mutable API resources; this subject's cover `ETag` is a separate, read-only,
content-addressed validator that the Design "Conditional requests and optimistic concurrency" does not mint or consume.
It does not own the `manifestations.file_path` and `current_file_hash` columns this subject reads: the Design "Works and
manifestations data model".

Depends on: `manifestations.file_path` and `current_file_hash`, read inside an RLS-scoped transaction for every
request-path lookup; `config.library_path`, which roots the cache directory; `config.opds.enabled`, which gates only the
OPDS mount's runtime mount (the API mount is unconditional).

Depended on by: the library grid, the library table view, the book detail page, the book detail drawer, and the series
page on the client, all of which request only the thumbnail tier; OPDS reader apps, which reach both tiers through the
acquisition feed's image and thumbnail links; the Layer 5 cover check the Design "EPUB validation and repair" describes,
which calls this subject's rasteriser to decide cover usability at ingestion; and the Design "Ingestion pipeline", which
calls this subject's pre-warm mechanism after a successful ingest.

## Structure

### State-writer census

The shared mutable state is the on-disk cache file at
`{library_path}/_covers/cache/{manifestation_id}-{hash16}- {tier}.{ext}`. Two independent call paths can write the same
path:

| State item | Where it lives | Writers |
| ---------- | -------------- | ------- |
| Cached cover file | `{library_path}/_covers/cache/…` | `get_or_create` (request-path miss) and `warm_one` (background pre-warm) |

Both funnel through the one write primitive, `CoverCache::write_atomic`: a temporary file in the cache directory,
written and flushed, then renamed into place, so a partial write is never visible at the final path. Neither writer
takes a lock over the destination path, and nothing coordinates the two: a request-path miss racing a pre-warm task for
the same `(manifestation_id, file_hash, size)`, or two concurrent request-path misses for the same cover, can both run
the extract-resize-write pipeline and both call `write_atomic` for the same destination. This is safe only because the
destination path is itself content-addressed: every writer computing the same key from the same source bytes produces
the same output bytes, so the redundant write is a no-op in effect, and `write_atomic`'s own documentation records
last-writer-wins on identical content as benign.

`cached_hit`, the read side both writers probe before generating, narrows this: the thumbnail tier probes only the `jpg`
extension, so a `png` or `webp` file at a thumbnail cache path is never read back by either writer. Such a file is
neither served nor removed; a fresh `jpg` copy is generated and cached alongside it. The full tier probes `jpg`, `png`,
and `webp` in that order, so it does not have this gap.

### Component relationships

- `extract.rs::extract_cover_bytes` re-validates the archive's Layer 1 structure (`zip_layer::validate`), then reuses
  `container_layer::validate` and `opf_layer::validate` to locate the manifest, and `cover_layer::find_cover_href` to
  resolve the declared cover entry, before reading its bytes. When the raster decoder (`image::guess_format`) cannot
  read the bytes and they look like SVG (`svg::looks_like_svg`), it hands them to `svg::rasterize_svg` with a sibling
  resolver scoped to the cover's directory inside the archive (`join_sibling_path`); the Layer 5 check the Design "EPUB
  validation and repair" describes calls `join_sibling_path` the same way, so both routes to a rasterised sibling
  resolve identically.
- `svg.rs::parse_and_gate` is the one hardened parse-and-gate routine shared by serve-time `rasterize_svg` and the
  resolver-free `parses_as_svg`, which the Layer 5 check in the Design "EPUB validation and repair" calls: input-byte
  cap, then a raw-byte nesting-depth scan (`check_nesting_depth`, before any parser touches the bytes), then a UTF-8
  check, then a `roxmltree` parse with `allow_dtd: false` (which rejects any DOCTYPE outright), then
  `usvg::Tree::from_xmltree` with the hardened image-href resolver (`hardened_resolver`, enforcing the per-image byte
  and megapixel caps and the cumulative sibling-resolution, sibling-byte, and decoded-pixel budgets during tree
  construction), then `check_render_complexity` (rejecting any filter primitive and any path-segment, node-count, or
  nesting-depth budget breach, walking `children`, the full `clip-path`/`mask` chains, and every node's paint-server
  sub-trees via `Node::subroots`). Only `rasterize_svg` goes on to render, check the output for an all-transparent
  pixmap, and encode PNG.
- `resize.rs::resize_cover` rejects any input format other than JPEG, PNG, or WebP, resizes with Lanczos3 only when the
  image exceeds the tier's long-edge cap, and encodes per tier: the thumbnail tier composites over white and always
  encodes JPEG at quality 82; the full tier re-encodes in the input format.
- `mod.rs::generate_into_cache` is the one synchronous extract-resize-write pipeline both writers call from inside
  `tokio::task::spawn_blocking`. `get_or_create` runs it on a cache miss reached from a request; it drops its RLS-scoped
  transaction before entering `spawn_blocking`, so no database connection is held during extraction or rasterisation.
  `warm_one` runs it from a background task, bounded by the `WARM_LIMIT` semaphore (three concurrent permits), which
  `spawn_warm_thumb` acquires before generating; nothing in the tree closes this semaphore.
- `routes/opds/covers.rs::serve_cover` is the one handler body both HTTP mounts share. It calls `get_or_create`,
  resolves the response by `CoverError` variant, checks a matching `If-None-Match` against the artifact's `ETag` before
  opening the file, and attaches the cache headers on every response that carries a body.
- The two mounts differ only in extractor wrapping and runtime gating: `covers::opds_router()` builds
  `/opds/books/{id}/cover{,/thumb}` behind `BasicOnly`, mounted only when `config.opds.enabled` (though always
  documented in the OpenAPI spec); `covers::api_router()`, exposed as `routes::opds::covers_router()`, builds
  `/api/v1/books/{id}/cover{,/thumb}` behind `CurrentUser`, merged into the pilot router unconditionally, so it is
  mounted regardless of `opds.enabled`.
- On the client, `CoverArtwork` is a pure decorative component with no data dependency on the cover pipeline; each of
  its two call sites (`LibraryPage.tsx`'s `BookCard`, `BookPage.tsx`'s `DetailCover`) pairs it with its own `<img>`
  element and its own `useState` boolean, set by that `<img>`'s `onError` handler. Two further `cover_url` consumers
  (`BookDetailDrawer.tsx`, `LibraryTableView.tsx`'s `TitleCellMark`) each implement the same pattern with their own,
  independent title-initials fallback instead of calling `CoverArtwork`; a fifth consumer, `SeriesPage.tsx`, attaches no
  failure handler to its `<img>` at all. Runtime behaviour below states each consumer's fallback fact. There is no
  shared state across the four consumers that track a load failure, no failure tracking in the fifth, and no cache layer
  of the client's own over cover bytes; the browser's native HTTP cache, partitioned by the response's `Vary` header, is
  the only client-side cache.

## Interfaces and dependencies

- `extract_cover_bytes(epub_path: &Path) -> Result<(Vec<u8>, ImageFormat), CoverError>`;
  `svg::rasterize_svg(svg_bytes, resolve_sibling) -> Result<Vec<u8>, CoverError>`;
  `svg::parses_as_svg(svg_bytes) -> bool`, called by the Design "EPUB validation and repair";
  `resize_cover(bytes, fmt, size) -> Result<(Vec<u8>, ImageFormat), CoverError>`.
- `CoverCache::new(root)`, `::ensure_dir()`, `::cached_path(manifestation_id, file_hash_prefix, size, ext) -> PathBuf`,
  `::write_atomic(dest, bytes)`.
- `get_or_create(state, manifestation_id, user_id, size) -> Result<CoverArtifact, CoverError>`, the request-path entry
  point; `spawn_warm_thumb(library_path, manifestation_id, file_hash, epub_path)`, the pre-warm entry point the Design
  "Ingestion pipeline" calls.
- Four `GET` endpoints, documented in the OpenAPI spec generated from `routes/opds/covers.rs`: `/opds/books/{id}/cover`,
  `/opds/books/{id}/cover/thumb` (Basic auth), `/api/v1/books/{id}/cover`, `/api/v1/books/{id}/cover/thumb` (session
  cookie, device-token bearer, OIDC bearer, or Basic). Each returns the image bytes with `Cache-Control`, a strong
  `ETag`, and `Vary`; a matching `If-None-Match` returns `304`; a missing manifestation, an RLS-hidden one, or one with
  no cover returns `404`.
- The acquisition feed (a component of the OPDS catalogue subject) links each entry to both tiers via the
  `http://opds-spec.org/image` and `http://opds-spec.org/image/thumbnail` link relations, pointing at the OPDS mount.
- `CoverArtwork({ bookId, title, authors, className }) => ReactElement`, the client's spine-fallback component; it takes
  no cover URL and renders a deterministic generated binding keyed on `bookId`.

## Data and state

- **`CoverArtifact`**: `{ path: PathBuf, etag: String }`, the request-path result `get_or_create` returns; `etag` is
  unquoted, `"{file_hash[..16]}-{full|thumb}"`.
- **The cache key.** `cache.rs::cached_path` and `mod.rs::etag_for` derive the same sixteen-character
  `current_file_hash` prefix and size tag independently but identically, so the validator and the cache filename change
  together, exactly when a writeback changes the file's content hash.
- **Size tiers.** `Full`: long edge capped at 1200 px, source format preserved. `Thumb`: long edge capped at 300 px,
  always JPEG at quality 82 (white-composited first, since JPEG carries no alpha channel).
- **SVG hardening budgets** (`svg.rs`): input bytes capped at 4 MiB; a single embedded raster (sibling or data URI)
  capped at 8 MiB and 16 megapixels; the cumulative decoded-pixel budget across one rasterisation capped at 64
  megapixels, shared by both resolver paths; sibling resolutions capped at 8 per rasterisation, with a cumulative 32 MiB
  sibling-byte budget; render output capped at a 1200 px long edge; path segments and node count each capped at 50,000;
  element-nesting depth capped at 48.
- **Cache headers.** A cover the endpoints can serve: `Cache-Control: private, max-age=86400`, a strong `ETag`,
  `Vary: Authorization, Cookie`. A negative response for a cover artifact resolved but missing from disk:
  `Cache-Control: private, max-age=60`, the same `Vary`, no `ETag`. A manifestation with no cover, an archive Layer 1
  rejects, or a manifestation RLS hides: the bare `AppError::NotFound` body, with neither `Cache-Control` nor `Vary`.
  The same shape applies in every one of those three cases.
- **`WARM_LIMIT`**: a process-static semaphore fixed at three permits, not exposed as configuration.
- **The `has_embedded_cover` column** this subject depends on but does not write: it gates only whether the Design
  "Ingestion pipeline" invokes `spawn_warm_thumb` after an ingest; it plays no part in the request-path `get_or_create`
  flow, which always attempts extraction on a cache miss regardless of that column's value.

## Runtime behaviour

**Serving a cover on the request path** (either mount, either tier):

1. `serve_cover` calls `get_or_create`, which opens an RLS-scoped transaction (`db::acquire_with_rls`), looks up the
   manifestation's `file_path` and `current_file_hash`, and drops the transaction immediately after.
2. A row RLS hides, or that does not exist, yields the same `CoverError::NoCover` as a manifestation with no cover;
   `get_or_create` cannot tell the two apart, by construction.
3. `cached_hit` probes the cache directory for an already-encoded file at this tier (`jpg` only for `Thumb`; `jpg`,
   `png`, `webp` in order for `Full`). A hit returns immediately with no filesystem write and no archive access.
4. A miss enters `spawn_blocking` and runs `generate_into_cache`: `extract_cover_bytes` re-validates the archive and
   locates the cover (rasterising it first if it is SVG-declared), `resize_cover` resizes and encodes for the tier, and
   `CoverCache::write_atomic` writes the result under its content-addressed name.
5. `serve_cover` compares a request's `If-None-Match` against the artifact's quoted `ETag`; a match returns `304` with
   the cache headers and no body. Otherwise it opens the cached file, derives `Content-Type` from its extension, and
   streams it with the cache headers and a `200`.

**Rasterising an SVG-declared cover**, inside step 4 above, entirely before any byte reaches the resize step:

1. The raw bytes are capped at 4 MiB, then scanned for element-nesting depth with a flat byte loop that never itself
   parses or descends into the structure, before any parser touches them; nesting past 48 levels is rejected here.
2. The bytes are checked for valid UTF-8, then parsed by `roxmltree` with `allow_dtd: false`; a DOCTYPE of any kind is
   rejected at this step, since disabling DTD processing means no entity is ever expanded.
3. `usvg::Tree::from_xmltree` builds the render tree, consulting the hardened image-href resolver for every `<image>`
   element it encounters: each sibling ZIP entry or embedded data URI is checked against a per-image byte and megapixel
   cap and a cumulative decoded-pixel budget shared across both resolver paths, and sibling resolutions additionally
   against a per-rasterisation count and cumulative-byte cap; a reference over any of these budgets is dropped rather
   than resolved, so the image node renders as absent rather than the rasterisation failing outright.
4. `check_render_complexity` walks the built tree: every group's children, its full clip-path and mask chains, and every
   node's paint-server and layout sub-trees. It rejects the whole rasterisation if any filter primitive is reachable, or
   if the total path-segment count, node count, or descent depth exceeds its budget.
5. `resvg::render` draws into a pixmap capped at a 1200 px long edge; an all-transparent result is rejected so the case
   renders identically to an unresolvable cover, and the pixmap is otherwise PNG-encoded and returned.

**Pre-warming a thumbnail at ingestion**, off the request path: the Design "Ingestion pipeline", after a successful EPUB
ingest, evaluates a gate predicate over the manifestation's format and its ingestion-time `has_embedded_cover` value
and, when it passes, calls `spawn_warm_thumb`. The call is detached (`tokio::spawn`) and best-effort: it acquires one of
`WARM_LIMIT`'s three permits, runs `warm_one` (the same `cached_hit`-then-`generate_into_cache` sequence as the request
path, but for the thumbnail tier only), and logs any outcome without returning it to the caller. Full-size covers are
never pre-warmed; a full-size cover is generated on its first request instead.

**Reacting to a cover load on the client**: five surfaces render a server-supplied `cover_url` through their own `<img>`
element; four of the five also hold their own local failure state. `cover_url` is always a non-empty, server-constructed
`/api/v1/books/{id}/cover/thumb` path for every book row the API returns; the four failure-tracking surfaces' own
empty-string check on it is not exercised by any response shape this API's book-row endpoints return, since every such
row's `cover_url` is unconditionally populated regardless of whether the manifestation has a cover the endpoints can
serve. Four surfaces attach an `onError` handler to the `<img>` that fires on any failed load, whether a `404`, a `500`,
or a network failure, and sets a `useState` boolean on first failure: `LibraryPage.tsx`'s `BookCard` and
`BookPage.tsx`'s `DetailCover` fall back to the generated `CoverArtwork` spine; `BookDetailDrawer.tsx` and
`LibraryTableView.tsx`'s `TitleCellMark` each fall back to their own title-initials `<span>` instead of `CoverArtwork`.
`SeriesPage.tsx` attaches no `onError` handler and holds no failure state: its only conditional is whether the
manifestation returned a `cover_url` at all, so a present `cover_url` whose image request fails renders the browser's
own broken-image glyph rather than a fallback this subject or the page supplies. Each of the four surfaces that tracks a
load failure does so independently, so a failure on one does not affect another.

**A Layer 5 cover-usability check calling back into this subject**, at ingestion, in the opposite direction from every
path above: the Design "EPUB validation and repair" describes that check, which calls this subject's `rasterize_svg`
with the same sibling resolution serving uses, so the ingestion-time verdict and the serve-time result cannot disagree
for the same bytes; when `rasterize_svg` fails for a reason `parses_as_svg`'s resolver-free check would also reject (a
genuine parse or render-cost failure), that Design records a `Degraded` issue, while a rasterisation failure
`parses_as_svg` would accept (a blank render or an absent sibling) is treated as no usable cover, not an issue.

**A writeback rewriting a manifestation's file**, in the opposite direction from the Design "Writeback pipeline": it
recomputes `current_file_hash` from the file as written and refreshes `has_embedded_cover` from its own post-writeback
validation. The new hash changes the cache key every subsequent request derives, so the next request for that
manifestation misses the cache and generates fresh; nothing in this subject's own code reclaims the file the old hash
still names.

## Failure and recovery

- **No cover, an RLS-hidden manifestation, or a rejected archive.** All three collapse to the same bare
  `AppError::NotFound`, with neither a `Cache-Control` nor a `Vary` header attached by this subject or by anything the
  response passes through afterwards. An RLS-hidden manifestation is response-identical to a manifestation that
  genuinely has no cover, by the construction in Runtime behaviour above.
- **A cover file missing from disk**, whether `get_or_create` resolved a stale cache entry that was removed before the
  file could be opened, or the source EPUB itself has moved: this is a distinct `404` shape, `cover_miss_not_found()`,
  carrying `Cache-Control: private, max-age=60` and the same `Vary` as a success, so a grid of missing covers is not
  re-derived from disk on every navigation, but at a far shorter negative TTL than a real cover's day-long cache, since
  the underlying file can reappear (a re-scan, a remounted library) with no `ETag` for the browser to re-check against.
- **Every other `CoverError`** is a server error (`AppError::Internal`, `500`), never a `404`: a decode failure, an
  unsupported format, a database error, a corrupt ZIP, or any SVG hardening rejection (which the whole SVG pipeline in
  Runtime behaviour surfaces only as `Decode`). The client cannot tell this apart from a `404` by its visible behaviour,
  since its `<img onError>` fallback fires on either, but the two are distinct on the wire.
- **A `spawn_blocking` panic** during extraction, rasterisation, or resizing is caught by the `JoinError` mapping in
  both `get_or_create` and `warm_one`, and surfaces as `CoverError::Decode`: a server error on the request path, a
  logged and swallowed failure on the pre-warm path.
- **A generation race.** Two writers computing the same content-addressed cache key at the same time (a request-path
  miss racing a pre-warm task, or two concurrent request-path misses) each run the full extract-resize-write pipeline
  and each call `write_atomic` for the same destination; this is wasted work, not a correctness risk, since the two
  writes produce identical bytes and the atomic rename leaves one intact file regardless of which finishes last.
- **A `png` or `webp` file at a thumbnail cache path.** It is never matched by `cached_hit`'s `jpg`-only probe for that
  tier; it is neither served nor removed, and a fresh `jpg` copy accumulates alongside it.
- **An orphaned cache file after a writeback.** A writeback that changes `current_file_hash` re-keys every subsequent
  cache lookup for that manifestation; the file cached under the old hash is not deleted by this rewrite, by
  `write_atomic`, or by any other code in the tree. The file accumulates on disk indefinitely; only an operator removing
  it by hand reclaims the space.
- **A `WARM_LIMIT` semaphore acquire failure.** `spawn_warm_thumb` skips the pre-warm task with a logged warning when
  `WARM_LIMIT.acquire()` fails; nothing in the tree closes the semaphore, so no code path causes this branch to fire. A
  missed pre-warm never fails the ingest it was attached to regardless, and the same cover generates on the first real
  request instead.
- **A writeback within a cover's cache window.** Because the browser's cache for a cover is keyed on the URL alone and
  the URL is stable across a writeback, a client that already cached the pre-writeback cover keeps serving it from its
  own cache for the remainder of that cache's `max-age`; once it lapses, the changed `ETag` fails to match and the
  client fetches the new content.

## Security and operations

No SVG bytes are ever written to the cache or streamed to a client; every artefact this subject produces and serves is a
raster the `image` crate's encoders wrote, so the cover route carries no stored-XSS surface regardless of what an
uploaded EPUB's cover SVG contains. The SVG rasteriser runs over attacker-controlled XML pulled from an uploaded archive
under the repository's multi-user, internet-exposed threat model: every guard in the Runtime behaviour walkthrough
(input-byte cap, pre-parse nesting-depth scan, disabled DTD processing, `hardened_resolver`'s per-image and cumulative
budgets, the render-cost gate, the blank-render rejection) defends that boundary, and the resvg crate is built with its
`text` feature compiled out, so an SVG cover relying on live `<text>` elements renders without that text rather than
pulling a font-shaping stack into the dependency tree.

`private, max-age=86400` plus `Vary: Authorization, Cookie` on a cover fit to serve keeps a shared cache (a proxy, or a
shared browser profile switching between Reverie accounts) from storing or replaying an RLS-scoped cover across a
credential boundary; on the API mount, `Authorization` covers every credential the mount's `CurrentUser` extractor
accepts that arrives in that header: Basic, a device-token bearer, and an OIDC bearer alike, not only Basic as the OPDS
mount alone would suggest. A response that carries no image bytes and no `ETag` (the no-cover, RLS-hidden, and
archive-rejected cases) carries neither directive at all; without an explicit `Cache-Control`, whether and how long such
a response is cacheable is left to each cache's own heuristic freshness calculation under RFC 9111 §4.2.2, not to
anything this subject specifies.

This subject introduces no configuration of its own beyond the two settings it depends on (`library_path`,
`opds.enabled`), both owned elsewhere; the SVG hardening budgets, the size-tier pixel caps, the JPEG quality, the cache
header lifetimes, and the `WARM_LIMIT` concurrency bound are all compile-time constants, changeable only by a code
change.

## More information

- [Cover images](../../../../website/src/content/docs/guides/cover-images.md): the user-facing guide to which embedded
  formats render and how an SVG cover's limitations surface.
- [Pre-migration manifestations have no embedded-cover flag](../../../../debt/2026-07-31-embedded-cover-flag-not-backfilled.md):
  the accepted gap in `has_embedded_cover` coverage for manifestations ingested before that column existed, which this
  subject's pre-warm gate reads.
