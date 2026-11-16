---
type: REQ
profile-version: 1
id: "REV-REQ-0020"
title: "A precondition-protected write answers 428 without If-Match"
governed-by:
  - "REV-ADR-0011"
---

# A precondition-protected write answers 428 without If-Match

## Statement

WHEN a `PATCH` request to a book's metadata resource, a `PATCH` request to a book's reading-state resource, or a `PUT`
request to the settings resource carries no `If-Match` header, the server MUST answer `428 Precondition Required` and
MUST NOT begin a database write for that request.

GraphQL metadata, reading-state and exact metadata Undo mutations MUST require explicit baseline mutation arguments. An
omitted or null baseline MUST refuse the operation with `if-match-required` before a database write can begin. A
malformed baseline MUST be refused distinctly with `malformed-header` before database work, rather than treated as
missing or stale.

## Rationale

[RFC 6585 §3](https://www.rfc-editor.org/rfc/rfc6585#section-3) defines `428 Precondition Required` for exactly this
case: a state-changing request that omits a precondition the server requires, distinct from `412`, which answers a
precondition that was supplied but did not hold. Requiring `If-Match` on every write these three operations protect
closes the "lost update" gap RFC 9110 describes, where two clients read the same representation and the second write
silently overwrites the first's changes with neither client aware a conflict occurred.

## Acceptance criteria

- A `PATCH` to a book's metadata with no `If-Match` header answers `428`. Checked by
  `patch_metadata_without_if_match_returns_428` in `backend/src/routes/metadata.rs`.
- A `PATCH` to a book's reading state with no `If-Match` header answers `428`. Checked by
  `patch_reading_without_if_match_returns_428` in `backend/src/routes/reading.rs`.
- A `PUT` to the settings resource with no `If-Match` header answers `428` and leaves the stored settings unchanged.
  Checked by `put_settings_without_if_match_returns_428_and_leaves_settings_unchanged` in
  `backend/src/routes/settings/tests.rs`.
- GraphQL metadata and reading mutations refuse omitted, null and malformed baselines with their distinct execution
  classifications and leave stored state unchanged. Checked by `graphql_edit_baselines`.
- GraphQL exact Undo refuses omitted, null and malformed baselines without consuming the operation. Checked by
  `graphql_undo_boundary`.

## More information

The first two tests above assert the response status. Neither independently re-reads the resource afterwards to confirm
its state is unchanged; that guarantee follows from each handler checking for the header before it opens the database
transaction the write would run in, so a request missing the header never reaches a point where a write could begin.
