---
type: REQ
profile-version: 1
id: "REV-REQ-0018"
title: "Metadata and reading-state PATCH compare If-Match byte for byte"
governed-by:
  - "REV-ADR-0011"
---

# Metadata and reading-state PATCH compare If-Match byte for byte

## Statement

WHEN a `PATCH` request to a book's metadata resource or to a book's reading-state resource carries an `If-Match` header,
the server MUST compare the header's entity-tag with the resource's current entity tag by strong comparison of every
octet, with no normalisation or case-folding of the opaque content, and MUST answer `412 Precondition Failed` when any
octet differs; the `412` response MUST carry the resource's current `ETag`.

## Rationale

[RFC 9110 §13.1.2](https://www.rfc-editor.org/rfc/rfc9110#section-13.1.2) requires strong comparison for `If-Match`: two
entity-tags match only when both are strong and their opaque content is identical octet for octet. A comparison that
normalised or case-folded the opaque content could treat two representations as equal when the issuing server did not
intend them to be, defeating the purpose of a strong validator in an optimistic-concurrency check. Carrying the current
`ETag` on the `412` lets a caller holding a stale representation recover and retry in one round trip instead of issuing
a follow-up `GET`.

## Acceptance criteria

- A `PATCH` to a book's metadata carrying an `If-Match` that does not match the resource's current entity tag answers
  `412` and the response carries the resource's current `ETag`. Checked by
  `patch_metadata_with_stale_if_match_returns_412_with_current_etag` in `backend/src/routes/metadata.rs`.
- A `PATCH` to a book's reading state carrying an `If-Match` that does not match the resource's current entity tag
  answers `412` and the response carries the resource's current `ETag`. Checked by
  `patch_reading_with_stale_if_match_returns_412_with_current_etag` in `backend/src/routes/reading.rs`.
- A `PATCH` to either resource carrying an `If-Match` that matches the resource's current entity tag exactly is not
  refused on precondition grounds. Checked by `patch_metadata_with_matching_if_match_succeeds` in
  `backend/src/routes/metadata.rs` and `patch_reading_with_matching_if_match_succeeds` in
  `backend/src/routes/reading.rs`.
- The comparison is octet-exact and case-sensitive, so two entity-tags differing only in the case of their opaque
  content do not match. Checked by `strong_comparison_is_octet_exact` in `backend/src/routes/etag.rs`.
- The comparison holds for `obs-text` octets (`%x80`-`%xFF`) in the opaque content, which are valid entity-tag content
  but not valid UTF-8, so a comparison implemented over `&str` rather than bytes could not reach these values at all.
  Checked by `strong_comparison_holds_for_obs_text` in `backend/src/routes/etag.rs`.
