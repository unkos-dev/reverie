---
type: ADR
profile-version: 1
id: "REV-ADR-0025"
title: "Cover cache headers, ingest pre-warm, and JPEG thumbnails"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-14"
decision-makers:
  - "John Unkovich"
---

# Cover cache headers, ingest pre-warm, and JPEG thumbnails

## Context and problem statement

Library-grid load felt terrible on a freshly-populated dev instance. Profiling the cover path
(`/api/v1/books/{id}/cover{,/thumb}`) isolated three independent causes, none of which is the EPUB cover decode itself.

Every cover response carried `Cache-Control: no-store` with no `ETag`. The on-disk cache makes a warm server hit around
20 ms, but `no-store` forbids any browser caching, and the absence of a validator rules out cheap `304` revalidation, so
the browser re-downloads every visible cover on every navigation and scroll (around 8 MB for a 75-book grid), regardless
of server warmth. Was `no-store` load-bearing? Covers are row-level-security-scoped per user, but they are public-domain
book-cover images, not credentials or session material.
[`docs/security/codeguard/codeguard-0-http-headers.md`](../security/codeguard/codeguard-0-http-headers.md) reserves
`no-store` for sensitive data, so the header was over-strict rather than a deliberate threat-model control.

Cold generation is lazy and expensive. Covers rasterize and Lanczos3-resize on first view, not at ingest, and the cache
is wiped on library reset, so the first browse after a scan pays a cold generation per visible cover, serialized through
limited CPU.

Thumbnails preserved the source format, so SVG covers, rasterized to PNG by
[Rasterize SVG-declared EPUB covers to PNG](./0024-rasterize-svg-declared-epub-covers-to-png-via-resvg.md), served as a
roughly 100 KB PNG where a JPEG of the same 300 px thumb is roughly 15 KB.

## Decision drivers

- Grid load must not re-download unchanged covers on every navigation.
- Covers stay access-controlled by row-level security; no shared cache may store one.
- A metadata writeback rewrites a cover's `current_file_hash` at the same URL; staleness after a content change must be
  bounded.
- Request timeouts apply everywhere and no blocking IO runs on the async runtime, so warming must not stall the
  synchronous scan request nor run image work on a runtime thread.
- `image` 0.25 ships WebP decode-only, with no encoder.

## Considered options

- Cacheable private headers with a strong ETag
- Immutable long-lived caching
- Pre-warm thumbnails at ingest
- Leave thumbnail generation lazy
- JPEG-only thumbnails
- WebP thumbnails
- Preserve source format for thumbnails

## Decision outcome

Chosen option: **cacheable private headers with a strong ETag**, because repeat views can reuse covers without placing
user-scoped content in a shared cache.

Chosen option: **pre-warm thumbnails at ingest**, because the first grid view should not generate them on the request
path. Full-size covers remain lazy.

Chosen option: **JPEG-only thumbnails**, because they reduce payload size. Transparent source regions are composited
before encoding.

### Consequences

- Positive: repeat grid views serve from the browser cache; cold first views are eliminated by warming; thumbnail
  payloads shrink by roughly 6 to 8 times.
- Positive: the shared-browser cross-user replay threat, a cached RLS-scoped cover served to a different account after a
  switch on the same browser, is closed by `Vary: Authorization, Cookie`, which partitions the private cache by
  credential. Credentials are stable within a session, so per-session caching is preserved while the cross-user replay
  is blocked.
- Negative: a writeback within the `max-age` window shows a stale cover for up to a day. This is accepted because covers
  change rarely (enrichment), and the `ETag` makes it self-correct on the next revalidation.

## Pros and cons of the options

### Cacheable private headers with a strong ETag

- Positive: repeat views serve from the browser cache with no request, and a lapsed cache still gets a cheap
  revalidation via the `ETag`.
- Positive: `private` plus `Vary: Authorization, Cookie` keeps an RLS-scoped cover out of shared caches and out of a
  cross-user replay on a shared browser.
- Negative: a writeback within the `max-age` window can still serve a stale cover until the next revalidation.

### Immutable long-lived caching

- Negative: the cover URL is not hash-addressed, so `immutable` would pin a stale cover for the whole `max-age` window
  after a writeback changes content at the same URL.

### Pre-warm thumbnails at ingest

- Positive: the first grid view after a scan is a warm hit instead of a cold generation on the request path.
- Positive: detached, semaphore-bounded warming cannot stall the synchronous scan request or thundering-herd the
  blocking pool.
- Neutral: full-size covers stay lazy, since the reader view loads only one at a time.

### Leave thumbnail generation lazy

- Negative: the first browse after a scan pays a cold generation per visible cover, serialized through limited CPU.

### JPEG-only thumbnails

- Positive: a JPEG thumbnail at quality 82 is far smaller than the source-format thumbnail it replaces.
- Neutral: JPEG has no alpha, so transparency is composited over white before encoding.
- Negative: the full tier still preserves the source format, so the size reduction applies only to thumbnails.

### WebP thumbnails

- Negative: `image` 0.25 ships WebP decode-only, with no encoder, so this option is unavailable.

### Preserve source format for thumbnails

- Negative: a rasterized SVG cover serves a roughly 100 KB PNG thumbnail where a JPEG of the same 300 px thumb is
  roughly 15 KB.

## More information

Builds directly on
[Rasterize SVG-declared EPUB covers to PNG](./0024-rasterize-svg-declared-epub-covers-to-png-via-resvg.md): the
rasterization is unchanged; this decision governs how the result is cached, warmed, and encoded for delivery. Revisit if
cover serving moves behind a CDN, since the `private`/`ETag` contract would need re-evaluation, or if a WebP encoder
becomes available in the image stack.
