---
type: REQ
profile-version: 1
id: "REV-REQ-0020"
title: "A precondition-protected HTTP write without If-Match answers 428 and changes nothing"
governed-by:
  - "REV-ADR-0011"
---

# A precondition-protected HTTP write without If-Match answers 428 and changes nothing

## Statement

WHEN a `PATCH` request to a book's metadata resource, a `PATCH` request to a book's reading-state resource, or a `PUT`
request to the settings resource carries no `If-Match` header, the server MUST answer `428 Precondition Required` and
leave stored state unchanged.

## Rationale

[RFC 6585 §3](https://www.rfc-editor.org/rfc/rfc6585#section-3) defines `428 Precondition Required` for exactly this
case: a state-changing request that omits a precondition the server requires, distinct from `412`, which answers a
precondition that was supplied but did not hold. Requiring `If-Match` on every write these three operations protect
closes the "lost update" gap RFC 9110 describes, where two clients read the same representation and the second write
silently overwrites the first's changes with neither client aware a conflict occurred.

## Acceptance criteria

- A `PATCH` to a book's metadata with no `If-Match` header answers `428`, and a read afterwards returns the same values
  and `ETag` as before. Checked by `patch_metadata_without_if_match_returns_428` in `backend/src/routes/metadata.rs`.
- A `PATCH` to a book's reading state with no `If-Match` header answers `428`, and a read afterwards returns the same
  values and `ETag` as before. Checked by `patch_reading_without_if_match_returns_428` in
  `backend/src/routes/reading.rs`.
- A `PUT` to the settings resource with no `If-Match` header answers `428` and leaves the stored settings unchanged.
  Checked by `put_settings_without_if_match_returns_428_and_leaves_settings_unchanged` in
  `backend/src/routes/settings/tests.rs`.
