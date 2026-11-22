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
non-empty `errors` list in which every entry is a valid execution error, as defined under Acceptance criteria, the
client MUST publish none of its data and MUST surface an error that carries each entry's `message` and its `code`,
`errors`, `lockedFields` and `currentEtag` extensions unchanged. Any other body MUST fail without publishing data.

## Rationale

A Relay store that published a malformed body would serve wrong values to every component that reads them, and a store
that published partial data beside an execution fault would show a save as complete when it was not. The client reads
`extensions.code`, and the `errors`, `lockedFields` and `currentEtag` extensions, to decide whether to reload, correct
or retry, so those fields have to reach the caller as the server sent them.

## Acceptance criteria

A valid execution error is a JSON object with these members. Any other extension member is ignored: it neither
invalidates the entry nor reaches the caller.

| Member | Presence | Valid value |
| ------ | -------- | ----------- |
| `message` | Required | A non-empty string |
| `locations` | Optional | An array of objects whose `line` and `column` are positive integers |
| `path` | Optional | An array of strings and integers |
| `extensions` | Optional | An object carrying any of the four members below |
| `extensions.code` | Optional | A string |
| `extensions.errors` | Optional | An object whose values are strings |
| `extensions.lockedFields` | Optional | An array of strings |
| `extensions.currentEtag` | Optional | A strong entity tag: `"`, 22 characters from `A-Z`, `a-z`, `0-9`, `_` and `-`, then `"` |

- The bodies `{}`, `{ "errors": [] }`, `{ "errors": [{ "message": 7 }] }`, `{ "errors": [{ "message": "" }] }`,
  `{ "errors": [{ "message": "Refused", "extensions": { "currentEtag": "weak" } }] }` and `{ "data": null }` each fail
  without publishing data. Checked by `"GraphQL network preserves failures, CSRF recovery and account isolation"` in
  `frontend/tests/browser/api/graphql.test.tsx`.
- An envelope with data and a `field-locked` error surfaces an error that carries the message, `code`, `lockedFields`,
  `currentEtag` and `errors` unchanged. Checked by the same test.
- An envelope with an `internal` or `forbidden` error surfaces a `GraphQLExecutionError`. Checked by the same test.
