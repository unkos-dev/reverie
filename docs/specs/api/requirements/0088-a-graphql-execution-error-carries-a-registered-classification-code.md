---
type: REQ
profile-version: 1
id: "REV-REQ-0088"
title: "A GraphQL execution error carries a registered classification code"
---

# A GraphQL execution error carries a registered classification code

## Statement

WHEN a GraphQL request admitted to execution fails document validation, input coercion or field resolution, the GraphQL
endpoint MUST report each failure as an entry in the response's `errors` list whose `extensions.code` is one of the
registered classification codes, and a field validation failure MUST also carry an `extensions.errors` object that maps
each refused input field name to its validation message.

## Rationale

A caller that branches on a GraphQL failure needs a stable classification, not free-form message text. The browser
client decides between reloading, correcting and retrying from `extensions.code`, and a form shows each field's message
beside that field. A failure without a registered code would leave the client matching on message text, which changes
with wording. A failure reported as an HTTP status instead of a GraphQL error would be indistinguishable from a
transport fault.

## Acceptance criteria

The registered classification codes are `validation`, `invalid-cursor`, `forbidden`, `not-found`, `if-match-required`,
`if-match-mismatch`, `malformed-header`, `undo-unavailable`, `undo-conflict`, `edit-too-large`,
`system-shelf-immutable`, `field-locked` and `internal`.

- A refused field carries the `forbidden` code. Checked by `graphql_request_boundary` in `backend/src/graphql/mod.rs`.
- A refused connection cursor carries the `invalid-cursor` code. Checked by `graphql_cursor_contract` in
  `backend/src/graphql/mod.rs`.
- A field validation failure carries the `validation` code, keeps its own message, and carries an `extensions.errors`
  object that maps each refused field name to its message. Checked by `graphql_request_boundary`.
- An execution error raised without extensions, such as a failed input coercion, is given the `validation` code. Checked
  by `graphql_edit_baselines` and `graphql_undo_boundary` in `backend/src/graphql/mod.rs`, which send a baseline that
  fails `ETag` coercion.
