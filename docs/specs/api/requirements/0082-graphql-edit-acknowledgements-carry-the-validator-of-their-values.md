---
type: REQ
profile-version: 1
id: "REV-REQ-0082"
title: "GraphQL edit acknowledgements carry the validator of their values"
---

# GraphQL edit acknowledgements carry the validator of their values

## Statement

WHEN a GraphQL metadata save, reading-state save or exact metadata Undo succeeds, the server MUST return the values the
operation selected together with the validator of the revision those values belong to. A validator is the strong
entity-tag that identifies one revision, and a revision is the resource state a validator identifies.

## Rationale

The browser adopts an acknowledgement's values and validator as the starting point of the next edit. A validator from
another revision would either let the next save overwrite changes the reader never saw or refuse a save the reader made
against current values.

## Acceptance criteria

- A successful GraphQL metadata save or reading-state save returns selected values with the same validator that a fresh
  read of the resource then returns. Checked by `graphql_edit_baselines`.
- A successful exact Undo returns the restored values with the same validator that a fresh read of the resource then
  returns. Checked by `graphql_undo_boundary`.
