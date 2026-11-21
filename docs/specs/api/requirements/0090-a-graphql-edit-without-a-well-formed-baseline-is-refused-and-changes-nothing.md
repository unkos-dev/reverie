---
type: REQ
profile-version: 1
id: "REV-REQ-0090"
title: "A GraphQL edit without a well-formed baseline is refused and changes nothing"
---

# A GraphQL edit without a well-formed baseline is refused and changes nothing

## Statement

WHEN a GraphQL metadata save, reading-state save or exact metadata Undo omits its baseline, supplies `null` for it, or
supplies a value that is not one strong entity-tag, the server MUST answer with an error and leave stored state
unchanged. The baseline is the
[validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) of the editing baseline
the operation was made from. A strong entity-tag is a single `entity-tag` as defined by
[RFC 9110 §8.8.3](https://www.rfc-editor.org/rfc/rfc9110#section-8.8.3) without the `W/` weak indicator, so the `*`
wildcard, a weak tag, a list of tags and an unquoted value are not strong entity-tags.

## Rationale

Without a baseline the server cannot tell whether the caller's values describe what is stored, so applying the write
risks overwriting changes the caller never saw. A malformed baseline can match nothing by strong comparison, and
treating it as absent or as a wildcard would turn it into permission to write.

## Acceptance criteria

- A metadata save and a reading-state save with an omitted, `null` or malformed baseline each receive a response whose
  `errors` list is not empty, and a read afterwards returns the values and validator from before the request. Checked by
  `graphql_edit_baselines` in `backend/src/graphql/mod.rs`.
- An exact Undo with an omitted, `null` or malformed baseline receives a response whose `errors` list is not empty, a
  read afterwards returns the values from before the request, and the operation can still be undone afterwards. Checked
  by `graphql_undo_boundary` in `backend/src/graphql/mod.rs`.
