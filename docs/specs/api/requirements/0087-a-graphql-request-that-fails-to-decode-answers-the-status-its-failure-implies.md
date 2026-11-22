---
type: REQ
profile-version: 1
id: "REV-REQ-0087"
title: "A GraphQL request that fails to decode answers the status its failure implies"
---

# A GraphQL request that fails to decode answers the status its failure implies

## Statement

WHEN a request body sent to `/api/graphql` fails to decode before execution, the GraphQL endpoint MUST answer with the
status its failure implies: 400 for a body that is not valid JSON, 413 for a body over the size limit, 415 for a body
sent without an `application/json` content type, and 422 for valid JSON that does not match the request envelope.

## Rationale

[RFC 9110](https://www.rfc-editor.org/rfc/rfc9110) §15.5 assigns each of these statuses a distinct recovery: a 413
caller must shrink the payload, a 415 caller must change a header, and a 422 caller must change the payload's shape. A
GraphQL client that received one flattened code could not tell a transport fault it can fix from a document the server
will never accept.

## Acceptance criteria

| Failure                                                      | Status |
| ------------------------------------------------------------ | ------ |
| Request body that is not valid JSON                          | 400    |
| Request body over the size limit                             | 413    |
| Request body sent without an `application/json` content type | 415    |
| Request body that is valid JSON but not a GraphQL request    | 422    |

- Each failure in the table answers its status. Checked by `graphql_request_boundary` in `backend/src/graphql/mod.rs`.
- A request body that decodes as a GraphQL request envelope is not refused by this decode boundary, and its query
  reaches execution. Checked by the admitted `viewer` query in `graphql_request_boundary`.
