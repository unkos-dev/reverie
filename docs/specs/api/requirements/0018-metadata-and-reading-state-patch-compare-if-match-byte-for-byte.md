---
type: REQ
profile-version: 1
id: "REV-REQ-0018"
title: "Precondition-protected writes compare If-Match byte for byte"
governed-by:
  - "REV-ADR-0011"
---

# Precondition-protected writes compare If-Match byte for byte

## Statement

WHEN a `PATCH` request to a book's metadata resource or to a book's reading-state resource, or a `PUT` request to the
settings resource, carries an `If-Match` header, the server MUST compare the header's entity-tag with the resource's
current entity tag by strong comparison of every octet, with no normalisation or case-folding of the opaque content, and
MUST answer `412 Precondition Failed` when any octet differs; the `412` response MUST carry the resource's current
`ETag`.

GraphQL metadata, reading-state and exact metadata Undo mutations MUST compare their explicit baseline argument with the
resource's current strong entity-tag byte for byte under the write lock. A mismatch MUST refuse the write with the
`if-match-mismatch` execution code and the authorised current validator, before semantic patch validation or
save-request reuse checking. Successful selected acknowledgement values MUST carry the validator belonging to that same
snapshot.

## Rationale

[RFC 9110 §13.1.2](https://www.rfc-editor.org/rfc/rfc9110#section-13.1.2) requires strong comparison for `If-Match`: two
entity-tags match only when both are strong and their opaque content is identical octet for octet. A comparison that
normalised or case-folded the opaque content could treat two representations as equal when the issuing server did not
intend them to be, defeating the purpose of a strong validator in an optimistic-concurrency check. Carrying the current
`ETag` on the `412` identifies the current representation. A retained edit still needs a matched read before a new
editing decision can adopt that validator.

## Acceptance criteria

- A `PATCH` to a book's metadata carrying an `If-Match` that does not match the resource's current entity tag answers
  `412` and the response carries the resource's current `ETag`. Checked by
  `patch_metadata_with_stale_if_match_returns_412_with_current_etag` in `backend/src/routes/metadata.rs`.
- A `PATCH` to a book's reading state carrying an `If-Match` that does not match the resource's current entity tag
  answers `412` and the response carries the resource's current `ETag`. Checked by
  `patch_reading_with_stale_if_match_returns_412_with_current_etag` in `backend/src/routes/reading.rs`.
- A `PUT` to the settings resource carrying an `If-Match` that does not match the resource's current entity tag answers
  `412`, the response carries the resource's current `ETag`, and the stored settings are unchanged. Checked by
  `put_settings_with_stale_etag_returns_412_and_leaves_settings_unchanged` in `backend/src/routes/settings/tests.rs`.
- A `PATCH` to either book resource, or a `PUT` to the settings resource, carrying an `If-Match` that matches the
  resource's current entity tag exactly is not refused on precondition grounds. Checked by
  `patch_metadata_with_matching_if_match_succeeds` in `backend/src/routes/metadata.rs`,
  `patch_reading_with_matching_if_match_succeeds` in `backend/src/routes/reading.rs`, and
  `put_settings_with_matching_etag_persists_and_returns_the_new_etag` in `backend/src/routes/settings/tests.rs`.
- The comparison is octet-exact and case-sensitive, so two entity-tags differing only in the case of their opaque
  content do not match. Checked by `strong_comparison_is_octet_exact` in `backend/src/routes/etag.rs`.
- The comparison holds for `obs-text` octets (`%x80`-`%xFF`) in the opaque content, which are valid entity-tag content
  but not valid UTF-8, so a comparison implemented over `&str` rather than bytes could not reach these values at all.
  Checked by `strong_comparison_holds_for_obs_text` in `backend/src/routes/etag.rs`.
- GraphQL metadata and reading mutations refuse stale baselines without changing stored values, admit matching
  baselines, and return selected values with their matching validator. A stale unchanged metadata resend keeps its
  original request identity and resolves committed evidence without another write. Checked by `graphql_edit_baselines`.
- GraphQL exact Undo refuses a stale baseline and returns matching selected values and validator after an eligible
  restoration. Checked by `graphql_undo_boundary`.
