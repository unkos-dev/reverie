---
type: ADR
profile-version: 1
id: "REV-ADR-0080"
title: "Honest non-null schema with client-handled GraphQL field errors"
status: "accepted"
recorded-on: "2026-11-19"
decided-on: "2026-11-17"
decision-makers:
  - "John Unkovich"
---

# Honest non-null schema with client-handled GraphQL field errors

## Context and problem statement

Reverie's GraphQL schema must say which fields can be null, and its Relay client must decide what to do when a field
fails. Under the error propagation that GraphQL servers apply today, a failed non-null field nulls its nearest nullable
parent. That rule pushes schemas towards one of two compromises: mark fields nullable so that one failure stays local,
or mark them non-null and accept that one failure can blank everything up to the nearest nullable parent.

The GraphQL specification's working direction removes that compromise. The error behaviours RFC (`onError`, RFC 1) lets
a client ask the server not to propagate errors (`onError: "NULL"`). The server then returns `null` at the failed
position with an entry in `errors`, and the client handles it. The `@semanticNonNull` RFC (RFC 0) took another route and
has been marked stale.

The browser's Relay network refuses any response that carries execution errors, so no partial data reaches the store.
How should the schema mark which fields can be null, and how should the client handle field errors, while that refusal
remains?

## Decision drivers

- The schema states which positions can be null in the domain, so generated types do not force checks for values that
  cannot be absent.
- One failed field does not have to blank unrelated parts of a screen once the server can return partial data.
- Use the error-handling direction the GraphQL working group and Relay document, not a Reverie-specific protocol.
- Code written now does not need rewriting when the server gains `onError`.
- The client never renders a silently missing value as if it were real data.

## Considered options

- Honest non-null with client-handled field errors
- `@semanticNonNull` on the schema
- Nullable fields for resilience

## Decision outcome

Chosen option: **Honest non-null with client-handled field errors**, because it is the only option that both keeps the
schema truthful and follows the working group's `onError` direction, which Relay's field-error directives already
anticipate.

The schema marks a field non-null (`!`) when null is not a valid domain value. Client queries and standalone fragments
adopt `@throwOnFieldError` by convention so missing or failed fields route to error boundaries rather than rendering
silently blank.

### Consequences

- Positive: generated types match the domain, so components need no defensive checks on non-null fields.
- Positive: `@throwOnFieldError` throws when data is missing after a graph relationship change, routing gaps to the
  nearest error boundary instead of rendering blank.
- Positive: partial publication can later be enabled in the network layer without rewriting non-null schema types or
  component boundaries.
- Negative: while the server lacks `onError` support, a single field error still fails the whole response and prevents
  partial data publication.
- Negative: under standard GraphQL error propagation, a failure in a non-null field nulls more of the response than a
  nullable schema would.
- Negative: no lint rule enforces `@throwOnFieldError`, so code review maintains the convention.

## Pros and cons of the options

### `@semanticNonNull` on the schema

- Positive: Relay supports it, and with `@throwOnFieldError` it generates non-null types for fields that the server can
  still null on error.
- Negative: its RFC is at stage 0 and marked stale. The working group's stated direction is `onError`.
- Negative: async-graphql has no support, so the schema would need a custom directive that only Relay reads.

### Nullable fields for resilience

- Positive: works with every current server and client, with no waiting on upstream releases.
- Negative: the schema no longer says which values can be absent, so every consumer checks fields that are never null in
  practice.
- Negative: a client cannot tell a failed field from a real null unless it also reads `errors`.

## More information

- [GraphQL specification pull request 1163](https://github.com/graphql/graphql-spec/pull/1163): the error behaviours
  RFC, which adds `onError: "NULL"`.
- [GraphQL specification pull request 1065](https://github.com/graphql/graphql-spec/pull/1065): the `@semanticNonNull`
  RFC.
- At the decision date, neither async-graphql 7.2.1 nor Relay 21.0.1 implemented `onError`.
- Reconsider this decision if the working group abandons `onError`, or if Relay drops `@throwOnFieldError`.
- Revisit whole-response error refusal when a released async-graphql supports `onError`.
