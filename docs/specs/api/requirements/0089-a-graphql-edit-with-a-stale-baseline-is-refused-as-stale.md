---
type: REQ
profile-version: 1
id: "REV-REQ-0089"
title: "A GraphQL edit with a stale baseline is refused as stale"
---

# A GraphQL edit with a stale baseline is refused as stale

## Statement

WHEN a GraphQL metadata save, reading-state save or exact metadata Undo has passed authorisation, target resolution and
input coercion, and its baseline differs in any octet from the current
[validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) of its target, the
server MUST refuse it as stale and leave stored state unchanged, ahead of patch validation and request-ID reuse checks.
The baseline is the validator of the
[editing baseline](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) the operation
was made from. A request-ID reuse check refuses a metadata save whose request ID an earlier save in the same browser
session has already recorded. A refusal ahead of patch validation and request-ID reuse checks means that a stale
operation is refused as stale even when its patch would fail validation or its request ID has been used.

## Rationale

A stale baseline means the caller's values no longer describe what is stored, so any other verdict on the operation
judges input made against old values. Answering stale first tells the caller to read the current revision before it
corrects or repeats anything, and leaving stored state unchanged keeps a write from overwriting changes the caller never
saw.

## Acceptance criteria

- A metadata save and a reading-state save whose baseline is stale are refused as stale, a read afterwards returns the
  values from before the request, and the same save with the current baseline succeeds. Checked by
  `graphql_edit_baselines` in `backend/src/graphql/mod.rs`.
- A metadata save with a stale baseline and a patch that would fail validation is refused as stale, not as invalid.
  Checked by `graphql_edit_baselines` in `backend/src/graphql/mod.rs`.
- A metadata save with a stale baseline and a request ID that an earlier committed save in the same session recorded is
  refused as stale, not as a reused request. Checked by `graphql_edit_baselines` in `backend/src/graphql/mod.rs`.
- An exact Undo with a stale baseline is refused as stale, and the operation can still be undone afterwards. Checked by
  `graphql_undo_boundary` in `backend/src/graphql/mod.rs`.
- Two validators that differ only in the case of their opaque content do not match. Checked by
  `strong_comparison_is_octet_exact` in `backend/src/routes/etag.rs`.
