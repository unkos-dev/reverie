---
status: "accepted"
date: 2026-06-14
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Cover cache headers, ingest pre-warm, and JPEG thumbnails

## Context and Problem Statement

Library-grid load felt terrible on a freshly-populated dev instance. Profiling
the cover path (`/api/v1/books/{id}/cover{,/thumb}`) isolated three independent
causes, none of which is the EPUB cover decode itself:

1. **No client caching.** Every cover response carried `Cache-Control:
no-store` with no `ETag`. The on-disk cache makes a warm server hit ~20 ms,
   but `no-store` forbids _any_ browser caching and the absence of a validator
   rules out cheap `304` revalidation, so the browser re-downloads every
   visible cover on every navigation and scroll (~8 MB for a 75-book grid),
   regardless of server warmth.
2. **Cold generation is lazy and expensive.** Covers rasterize + Lanczos3-resize
   on first view, not at ingest, and the cache is wiped on library reset. The
   first browse after a scan pays a cold generation per visible cover,
   serialized through limited CPU.
3. **Oversized thumbnails.** Thumbnails preserved the source format, so SVG
   covers (rasterized to PNG by
   [the SVG-cover ADR](2026-06-13-svg-cover-rasterization.md)) served as a
   ~100 KB PNG where a JPEG of the same 300 px thumb is ~15 KB.

Was `no-store` load-bearing? Covers are RLS-scoped per user, but they are
public-domain book-cover _images_, not credentials or session material.
`docs/security/codeguard/codeguard-0-http-headers.md` reserves `no-store` for
_sensitive_ data. So the header was over-strict, not a deliberate
threat-model control.

## Decision Drivers

- Grid load must not re-download unchanged covers every navigation.
- Covers stay access-controlled (RLS); no shared cache may store one.
- A Step 8 metadata writeback rewrites a cover's `current_file_hash` at the
  _same_ URL; staleness after a content change must be bounded.
- "Request timeouts everywhere" + "no blocking IO on the async runtime"
  (backend invariants): warming must not stall the synchronous scan request
  nor run image work on a runtime thread.
- `image` 0.25 ships WebP **decode-only** (no encoder).

## Considered Options

- **A. Cacheable headers:** `Cache-Control: private, max-age` + strong `ETag` +
  `If-None-Match` → `304`.
- **A′. `immutable` + long max-age** (rejected): the cover URL is _not_
  content-addressed; a writeback changes content at the same URL, so
  `immutable` would pin a stale cover for the whole max-age window.
- **B. Pre-warm at ingest** vs leaving generation lazy.
- **C. JPEG thumbnails** vs WebP (unavailable) vs status-quo PNG.

## Decision Outcome

**A: caching headers.** Covers serve `Cache-Control: private, max-age=86400`
with a strong `ETag` of `"{current_file_hash[..16]}-{size}"`, and the handler
answers a matching `If-None-Match` with `304 Not Modified`. `private` keeps
shared proxies/CDNs from storing a cover, and `Vary: Authorization, Cookie`
partitions the per-user private cache so a shared browser cannot replay an
RLS-scoped cover across an account switch (covers are RLS-visibility-scoped, so
the same URL can be a `200` for one user and a `404` for another). `max-age`
serves repeat views from the browser cache with no request; once it lapses the
`ETag` drives a cheap revalidation. `immutable` is rejected because the URL is
not hash-addressed: the `ETag` (which _is_ derived from the file hash) gives
correct revalidation after a writeback instead.

**B: pre-warm the thumbnail at ingest.** `process_file` fires a
concurrency-bounded, best-effort background task that generates the thumbnail
for each newly-ingested EPUB. The first grid view is then a warm hit. Warming
is detached (the synchronous scan returns immediately), bounded by a process
semaphore (no thundering herd on the blocking pool), and never fails ingest.
Full-size covers stay lazy: the reader view loads one at a time.

**C: JPEG thumbnails.** The thumbnail tier always encodes to JPEG (quality
82); the full tier preserves the source format. Since JPEG has no alpha,
transparency is composited over white first; a bare channel-drop would render
the transparent regions of a non-canvas-filling SVG cover black. WebP would
preserve alpha directly, but `image` 0.25 has no WebP encoder.

### Consequences

- Good, because repeat grid views serve from the browser cache; cold first
  views are eliminated by warming; thumbnail payloads shrink ~6–8×.
- Good, because moving generation into `spawn_blocking` (both the warm path and
  the lazy `get_or_create` miss path) removes image work from the async runtime
  thread: a latent blocking-IO bug.
- Neutral, because the shared-browser cross-user replay (THREAT): a cached
  RLS-scoped cover served to a different account after a switch on the same
  browser: is closed by `Vary: Authorization, Cookie`, which partitions the
  private cache by credential. Credentials are stable within a session, so
  per-session caching is preserved while the cross-user replay is blocked.
- Bad, because a writeback within the `max-age` window shows a stale cover for
  up to a day. Accepted: covers change rarely (enrichment), and the `ETag`
  makes it self-correct on the next revalidation.
- Neutral, because cache entries written before this ADR keep their old
  encoding until their hash changes; the change is not retroactive.

### Confirmation

- `routes::opds::tests::cover_cache_populates_and_serves` asserts
  `private, max-age=86400`, a quoted strong `ETag`, and `If-None-Match` → `304`.
- `routes::opds::tests::svg_cover_rasterizes_and_serves` asserts the thumb is
  JPEG and the full cover preserves PNG.
- `services::covers::resize::tests` pin thumb-always-JPEG / full-preserves-format;
  `services::covers::tests::warm_one_*` pin the warm path and its idempotency.

## More Information

Builds directly on [Rasterize SVG-declared EPUB covers to
PNG](2026-06-13-svg-cover-rasterization.md): the rasterization is unchanged;
this ADR governs how the result is cached, warmed, and encoded for delivery.
Revisit if cover serving moves behind a CDN (the `private`/`ETag` contract
would need re-evaluation) or if a WebP encoder becomes available in the image
stack.
