---
type: REQ
profile-version: 1
id: "REV-REQ-0084"
title: "Every GraphQL root field requires its declared scope"
governed-by:
  - "REV-ADR-0056"
---

# Every GraphQL root field requires its declared scope

## Statement

WHEN a request to `/api/graphql` selects a root field, the GraphQL endpoint MUST refuse that field unless the request's
credential carries the scope the field declares, or a higher scope in the order `read` < `write` < `admin`, whichever
HTTP method carried the request.

## Rationale

A narrowed credential, such as a read-only automation token, is a guarantee only if every root field checks the scope it
declares. Every GraphQL operation arrives at one endpoint, usually by `POST`, so the HTTP method cannot carry the scope
gate. A field that inferred its scope from the method would refuse every read-only query sent with `POST`, and a field
that declared no scope would give the credential capability its owner never granted.

## Acceptance criteria

- Every GraphQL root field declares a scope. Checked by review of the `ReadScope` and `WriteScope` guards on the root
  fields in `backend/src/graphql/mod.rs` and `backend/src/graphql/mutations.rs`.
- A request with a missing or invalid credential is refused with `401`. Checked by `graphql_request_boundary` in
  `backend/src/graphql/mod.rs`.
- A query sent with `POST` under a `read` credential is admitted. Checked by `graphql_request_boundary`.
- A root field that declares `write` refuses a `read` credential with the `forbidden` execution code. A credential
  holding `write` or `admin` scope passes the scope check. Checked by `graphql_request_boundary`, including its
  successful eligible controls.
