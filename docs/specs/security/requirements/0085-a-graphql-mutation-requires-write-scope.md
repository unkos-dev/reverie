---
type: REQ
profile-version: 1
id: "REV-REQ-0085"
title: "A GraphQL mutation requires write scope"
governed-by:
  - "REV-ADR-0056"
---

# A GraphQL mutation requires write scope

## Statement

WHEN a request to `/api/graphql` selects a mutation root field, the GraphQL endpoint MUST refuse that field for any
credential that does not carry at least `write` scope.

## Rationale

`read` is the floor every valid credential carries, and mutation root fields are how the GraphQL API changes stored
data. If a mutation accepted `read`, a credential its owner narrowed to read-only could make changes anyway.

## Acceptance criteria

- Every mutation root field refuses a `read` credential with the `forbidden` execution code. Checked by
  `graphql_request_boundary` in `backend/src/graphql/mod.rs`, whose schema inventory fails unless the check covers every
  mutation root field.
- An eligible caller holding `write` scope can execute every mutation root field. Checked by `graphql_request_boundary`.
