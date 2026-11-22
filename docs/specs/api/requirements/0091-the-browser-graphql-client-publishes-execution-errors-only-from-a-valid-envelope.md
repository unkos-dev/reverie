---
type: REQ
profile-version: 1
id: "REV-REQ-0091"
title: "The browser GraphQL client publishes execution errors only from a valid envelope"
---

# The browser GraphQL client publishes execution errors only from a valid envelope

## Statement

WHEN the browser GraphQL client receives a 2xx response to `/api/graphql`, it MUST publish the response's data only when
the body is a GraphQL response envelope with a non-null `data` object and no `errors` list. WHEN the envelope carries a
non-empty `errors` list in which every error has a string `message`, the client MUST publish none of its data, MUST
surface an error that preserves each error's `message` and `extensions` unchanged, and MUST NOT send the operation
again. Any other body MUST fail without publishing data.

## Rationale

A Relay store that published a malformed body would serve wrong values to every component that reads them, and a store
that published partial data beside an execution fault would show a save as complete when it was not. The client reads
`extensions.code`, and the `errors`, `lockedFields` and `currentEtag` extensions, to decide whether to reload, correct
or retry, so those fields have to reach the caller as the server sent them. A replayed mutation could apply an edit
twice.

## Acceptance criteria

- The bodies `{}`, `{ "errors": [] }`, `{ "errors": [{ "message": 7 }] }` and `{ "data": null }` each fail without
  publishing data. Checked by `"GraphQL network preserves failures, CSRF recovery and account isolation"` in
  `frontend/tests/browser/api/graphql.test.tsx`.
- An envelope with data and a `field-locked` error surfaces an error that carries the message, `code`, `lockedFields`,
  `currentEtag` and `errors` unchanged, after exactly one request. Checked by the same test.
- An envelope with an `internal` or `forbidden` error surfaces a `GraphQLExecutionError`. Checked by the same test.
