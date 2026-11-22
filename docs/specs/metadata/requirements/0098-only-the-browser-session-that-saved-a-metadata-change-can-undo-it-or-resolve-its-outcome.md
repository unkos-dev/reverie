---
type: REQ
profile-version: 1
id: "REV-REQ-0098"
title: "Only the browser session that saved a metadata change can undo it or resolve its outcome"
governed-by:
  - "REV-ADR-0069"
---

# Only the browser session that saved a metadata change can undo it or resolve its outcome

## Statement

WHEN a request asks to undo a saved metadata change or to resolve the outcome of a metadata save or Undo, the system
MUST act on that change's retained evidence only for the browser session that made the save: it MUST refuse the request
when HTTP Basic credentials or a device token authenticate it, MUST refuse as unavailable an Undo from any other browser
session that carries the current baseline, and MUST report the outcome as unknown to a resolution from any other browser
session, including another session of the same account, without changing stored state in any of these cases. A saved
metadata change is a field change that a browser metadata save recorded for exact Undo. An Undo carries the current
baseline when its `If-Match` value over `/api/v1`, or its `baseline` input over GraphQL, equals the current validator of
the book's metadata. Resolving an outcome is asking, by save request ID or Undo operation ID, whether a save or an Undo
committed.

## Rationale

Exact Undo retains the field's previous values, including draft values a reader may not want kept, for the editing
context that made the save. An operation or request ID is otherwise only a string: if any credential could present one,
a device token or another signed-in device could restore a change, or learn whether a save committed, in an editing
context that never saw it. Binding the evidence to the issuing browser session keeps Undo a feature of the screen where
the reader made the edit.

## Acceptance criteria

- The browser session that made a save can restore the change and resolve its outcome, which reports the save and then
  the restoration. Checked by `metadata_undo_binds_receipts_to_session` in `backend/src/routes/metadata.rs` and
  `graphql_undo_boundary` in `backend/src/graphql/mod.rs`.
- An Undo and a resolution authenticated by HTTP Basic credentials, or by a device token that carries write scope, each
  produce an error with the `forbidden` code. Checked over GraphQL by `graphql_undo_boundary`; `/api/v1` reaches the
  same check through `session_id` in `backend/src/services/metadata/editing/undo.rs`.
- An Undo from another browser session of the same account that carries the current baseline is refused as unavailable.
  It answers `422` over `/api/v1`, checked by `metadata_undo_binds_receipts_to_session`, and returns `UndoUnavailable`
  over GraphQL, checked by `graphql_undo_boundary`. A resolution from that session reports `UNKNOWN`. Checked by
  `graphql_undo_boundary`.
- An Undo from another account's browser session that carries the current baseline returns `UndoUnavailable`, and a
  resolution from that session reports `UNKNOWN`. Checked by `graphql_undo_boundary`.
- None of the refused requests consumes the Undo operation: the issuing session restores the change afterwards. Checked
  by `metadata_undo_binds_receipts_to_session` and `graphql_undo_boundary`.

## More information

The `/api/v1` operations are `POST /api/v1/books/{id}/metadata/undo` and
`POST /api/v1/books/{id}/metadata/operations/resolve`. The GraphQL operations are the `undoMetadata` mutation and the
`metadataOperation` query. An Undo whose baseline is stale is refused as stale whichever session sends it, under
[REV-REQ-0018](../../api/requirements/0018-a-precondition-protected-rest-write-compares-if-match-byte-for-byte.md#statement)
over `/api/v1` and
[REV-REQ-0095](../../api/requirements/0095-a-graphql-edit-with-a-stale-baseline-is-refused-as-stale.md#statement) over
GraphQL.
