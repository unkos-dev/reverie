---
type: REQ
profile-version: 1
id: "REV-REQ-0017"
title: "A 500 response carries no internal error text"
---

# A 500 response carries no internal error text

## Statement

WHEN an operation under `/api/v1` answers with status 500, the response body's `detail` field MUST NOT contain any text
drawn from the underlying failure; it MUST instead be the fixed sentence "An internal error occurred."

## Rationale

A 500 response's underlying cause can carry operational detail an operator never intends a caller to see: a connection
string, a file path, a query fragment, a stack frame. The
[OWASP REST Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html) advises
against exposing such detail in error responses, since a caller — potentially unauthenticated, since a 500 can occur
before authentication completes — can otherwise mine failure text for information about the server's internals.
Replacing every internal failure's message with one fixed sentence removes that channel entirely, independent of what
any individual failure happens to say.

## Acceptance criteria

- A response answering 500 for an internal failure carries exactly the `detail` value "An internal error occurred.", not
  a fragment or paraphrase of the underlying failure's own message. Checked by
  `internal_returns_500_without_leaking_details` in `backend/src/error/mod.rs`.
- The same test constructs an underlying failure whose own message names a database connection string and asserts that
  neither that string nor any fragment of it appears anywhere in the response body, confirming the fixed sentence is a
  full replacement rather than a value appended alongside the original text.
