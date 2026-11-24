---
type: REQ
profile-version: 1
id: "REV-REQ-0095"
title: "A GraphQL edit with a stale baseline is refused as stale"
---

# A GraphQL edit with a stale baseline is refused as stale

## Statement

WHEN a GraphQL `updateMetadata`, `updateReadingState` or `undoMetadata` mutation supplies a baseline that differs in any
octet from the current [validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement)
of its target, the server MUST refuse the mutation with a response that identifies the baseline as stale and MUST leave
stored state unchanged, even when the mutation's patch would fail validation or its save request ID has already been
used. The baseline is the validator of the values the caller's edit was made from. A save request ID is the `requestId`
value of an `updateMetadata` input, and it has already been used when an earlier save in the same browser session
committed with it.

## Rationale

A stale baseline means another writer changed the target after the caller read it, so applying the edit would overwrite
a change the caller never saw: the lost update that
[RFC 9110 §13.1.1](https://www.rfc-editor.org/rfc/rfc9110#section-13.1.1) preconditions exist to prevent. The caller has
to read the current values before anything else about its edit means anything. A response that reported a patch
validation failure or a reused request instead would ask the reader to correct an edit that was made against values that
no longer hold.

## Acceptance criteria

- An `updateMetadata` with a stale baseline returns `StaleBaseline`, and a read afterwards returns the values and
  validator from before the attempt. A save with the current baseline returns `MetadataSaved`. Checked by
  `graphql_edit_baselines` in `backend/src/graphql/mod.rs`.
- An `updateMetadata` with a stale baseline and an empty patch, which fails patch validation, returns `StaleBaseline`.
  Checked by `graphql_edit_baselines`.
- Sending a committed `updateMetadata` again unchanged, so that its baseline is stale and its save request ID has been
  used, returns `StaleBaseline`. The same save request ID with the current baseline returns `MetadataInvalid`. Checked
  by `graphql_edit_baselines`.
- An `updateReadingState` with a stale baseline returns `StaleBaseline`, and a read afterwards returns the values and
  validator from the last successful save. Checked by `graphql_edit_baselines`.
- An `undoMetadata` with a stale baseline returns `StaleBaseline` without consuming the Undo operation: the same session
  restores the change with the current baseline afterwards. Checked by `graphql_undo_boundary` in
  `backend/src/graphql/mod.rs`.
