---
type: REQ
profile-version: 1
id: "REV-REQ-0082"
title: "GraphQL edit acknowledgements carry the validator of their values"
---

# GraphQL edit acknowledgements carry the validator of their values

## Statement

WHEN a GraphQL metadata or reading-state save succeeds, the server MUST return the values the operation selected
together with the validator of the revision those values belong to. A revision is one committed state of a book's
metadata or a reader's reading state. A validator is the strong RFC 9110 entity-tag the server issues for one revision.

## Rationale

The browser adopts an acknowledgement's values and validator as the starting point of the next edit. A validator from
another revision would either let the next save overwrite changes the reader never saw or refuse a save the reader made
against current values.

## Acceptance criteria

- A successful GraphQL metadata save or reading-state save returns selected values with the same validator that a fresh
  read of the resource then returns. Checked by `graphql_edit_baselines` in `backend/src/graphql/mod.rs`.
