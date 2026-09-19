---
type: REQ
profile-version: 1
id: "REV-REQ-0048"
title: "SVG cover bytes are never served"
governed-by:
  - "REV-ADR-0024"
---

# SVG cover bytes are never served

## Statement

WHEN a manifestation's declared cover file is an SVG image, the cover endpoints MUST serve only a raster rendering of
that cover, at both the full and thumbnail sizes and on both the OPDS and API mounts, and MUST NOT return SVG bytes in
any cover response body; WHEN the cover service refuses to rasterise that SVG as hazardous, the cover endpoints MUST NOT
serve the source SVG either, returning an error response with no cover image body instead.

## Rationale

An SVG document can carry script, so a cover route that ever returned the source bytes would open a stored-XSS surface
reachable by any uploaded book, in a self-hosted server exposed to more than one account. Readers depend on the cover
endpoints returning only image formats a browser renders inertly; a caller that receives raw SVG where it expects a
cover image has no way to tell a legitimate cover from an attack payload.

## Acceptance criteria

- A request for a manifestation whose declared cover is an SVG returns `image/png` at full size and `image/jpeg` at
  thumbnail size, on both mounts. Checked by `svg_cover_rasterizes_and_serves` in `backend/src/routes/opds/tests.rs`.
- No cover response, on either mount or at either size, carries a `Content-Type` of `image/svg+xml`. Not checked by any
  automated test: no test asserts the absence of the header across all response shapes.
- An SVG cover that the cover service refuses (a malformed document, or one that fails a hardening check) yields no
  image body: the request returns `500 Internal Server Error` with a `problem+json` body, not the SVG bytes and not a
  `200`. Checked by `malformed_svg_cover_does_not_serve` in `backend/src/routes/opds/tests.rs`.
- A rejected SVG cover is never written to the on-disk cover cache. Checked by `malformed_svg_cover_does_not_serve` in
  `backend/src/routes/opds/tests.rs`, which asserts the cache directory holds no entries afterwards.
