---
type: REQ
profile-version: 1
id: "REV-REQ-0089"
title: "A GraphQL internal failure carries no internal error text"
---

# A GraphQL internal failure carries no internal error text

## Statement

WHEN a request to `/api/graphql` fails internally, either as an HTTP 500 response or as a GraphQL execution error with
the `internal` code, the GraphQL endpoint MUST answer with the fixed text "An internal error occurred." in place of the
failure's own text, and MUST NOT expose any text drawn from the underlying failure anywhere in the response.

## Rationale

An internal failure's underlying cause can carry operational detail that an operator never intends a caller to see, such
as a connection string, a file path or a query fragment. The
[OWASP REST Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html) advises
against exposing such detail in error responses. A GraphQL execution error is a second route to the caller beside the
HTTP status, so both routes need the same fixed replacement.

## Acceptance criteria

- An internal execution error carries exactly the message "An internal error occurred.". Its envelope contains no
  fragment of an underlying failure message that names a database credential. Checked by `graphql_request_boundary` in
  `backend/src/graphql/mod.rs`.
- An HTTP 500 response from `/api/graphql` carries exactly the `detail` value "An internal error occurred.". Held by
  review: the endpoint answers HTTP failures through the same `AppError` mapping that REV-REQ-0017 checks.
