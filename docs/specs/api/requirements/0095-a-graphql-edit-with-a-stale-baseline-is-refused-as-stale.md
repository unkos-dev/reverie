---
type: REQ
profile-version: 1
id: "REV-REQ-0095"
title: "A GraphQL edit with a stale baseline is refused as stale"
---

# A GraphQL edit with a stale baseline is refused as stale

## Statement

WHEN a GraphQL `updateMetadata` or `updateReadingState` mutation supplies a baseline that differs in any octet from the
current [validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) of its target,
the server MUST refuse the mutation with a response that identifies the baseline as stale and MUST leave stored state
unchanged, even when the mutation's patch would fail validation. The baseline is the validator of the values the
caller's edit was made from.

## Rationale

A stale baseline means another writer changed the target after the caller read it, so applying the edit would overwrite
a change the caller never saw: the lost update that
[RFC 9110 §13.1.1](https://www.rfc-editor.org/rfc/rfc9110#section-13.1.1) preconditions exist to prevent. The caller has
to read the current values before anything else about its edit means anything. A patch validation failure would ask the
reader to correct an edit made against values that no longer hold.

## Acceptance criteria

- An `updateMetadata` with a stale baseline returns `StaleBaseline`, and a read afterwards returns the values and
  validator from before the attempt. A save with the current baseline returns `MetadataSaved`. Checked by
  `graphql_edit_baselines` in `backend/src/graphql/mod.rs`.
- An `updateMetadata` with a stale baseline and an empty patch, which fails patch validation, returns `StaleBaseline`.
  Checked by `graphql_edit_baselines`.
- An `updateReadingState` with a stale baseline returns `StaleBaseline`, and a read afterwards returns the values and
  validator from the last successful save. Checked by `graphql_edit_baselines`.
