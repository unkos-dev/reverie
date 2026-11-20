---
type: ADR
profile-version: 1
id: "REV-ADR-0083"
title: "GraphQL and Relay for browser application data with server-typed formats"
status: "proposed"
recorded-on: "2026-11-20"
decision-makers:
  - "John Unkovich"
---

# GraphQL and Relay for browser application data with server-typed formats

## Context and problem statement

The browser application reads related library records and edits shared metadata alongside caller-owned reading state
through GraphQL and Relay. Its data boundary must keep editing values with their matching preconditions and refuse data
whose formats are wrong before a reader acts on it. A client check written per operation must name every field and skips
any operation it does not list, while the server already holds the typed value each field is produced from. Where should
the formats of browser GraphQL data be guaranteed, and what does the browser still check?

This record would replace REV-ADR-0077, which chose GraphQL and Relay for browser application data with browser checks
of custom scalar formats. It restates the surrounding choices from that record that it depends on: GraphQL and Relay for
browser application data, trusted-document admission, the retained HTTP contracts and the GraphQL server library.

## Decision drivers

- Each format is defined once, by the type that produces or accepts the value.
- No operation can bypass format validation by being absent from a list of checks.
- A malformed input is refused before any write and is not offered as a temporary failure to retry.
- Editing values keep their matching preconditions through errors and live updates.
- Authentication, privacy, retained HTTP consumers and device-token clients keep their existing contracts.
- Only reviewed operations execute in production.

## Considered options

- Server-typed scalars and enums, with envelope checks in the browser
- Browser checks generated from the schema and operation documents
- Hand-written browser projections per operation

## Decision outcome

Chosen option: **Server-typed scalars and enums, with envelope checks in the browser**, because each format is then
defined once, in the Rust type that produces or accepts the value, and GraphQL result coercion turns any breach into an
execution error on that field, so no operation can skip the check.

GraphQL and Relay own browser application data. The code-first schema is implemented in Rust with async-graphql and
async-graphql-axum 7.2.1, and the Relay compiler produces operation artefacts and TypeScript types. React Aria owns
accessible interaction and virtualisation, and the oxc React Compiler remains enabled. TanStack Query remains only for
browser workflows still on HTTP; each migrated workflow has one server-data owner. Authentication protocols, redirects,
OPDS and binary delivery keep their HTTP contracts, and retained HTTP APIs keep code-first OpenAPI documentation.

A format-bearing value is a custom scalar backed by a Rust type and declared with `@specifiedBy`, and a closed set of
values is an enum. The browser network validates the transport response and the execution-error envelope of every
GraphQL response. Operations move one at a time: an operation keeps its hand-written projection until the server types
its formats and its editing recovery still holds. A user-entered value whose refusal must return field feedback stays a
string and is validated by its domain service. Zod continues to validate forms, URL inputs and retained REST responses.

Production executes only build-generated trusted documents and disables schema introspection, for session and device
token callers alike. Development tooling retains introspection. Arbitrary documents and unknown IDs cannot execute or
register at runtime, and the released frontend and server carry matching document artefacts without raw-query fallback.
The versioned `/api/v1` data surface remains the contract for device-token and other non-browser clients until a real
third-party client needs GraphQL access.

The transport preserves caller visibility, scope and role checks, session CSRF, matched editing values and
preconditions, authoritative recovery and exact saved-field Undo. Live cache updates do not advance a retained draft.

### Consequences

- Positive: a format is stated once, and the schema and generated types carry it to every client.
- Positive: a coercion failure reaches the client as an execution error on the field, so a malformed value is never
  published as data.
- Positive: client code sheds a hand-written projection as each operation moves.
- Positive: migrating a browser workflow to GraphQL removes no REST operation, and REV-ADR-0011 still governs the
  retained HTTP APIs.
- Negative: hand-written projections and server-typed formats coexist until the last operation moves.
- Negative: until a released server honours `onError`, the browser refuses a whole response with any error, so one
  field's coercion failure fails the operation.
- Negative: user-entered formats such as ISBNs and publication dates stay strings, so the schema does not express them.
- Negative: async-graphql has had no release since 7.2.1, so upstream fixes, including security fixes, may not arrive.
  Exposure stays low while only trusted documents execute.
- Negative: schema and operation generation add build integration and matching release artefacts.
- Negative: TanStack Query and Relay coexist until the last browser workflow migrates.

## Pros and cons of the options

### Server-typed scalars and enums, with envelope checks in the browser

- Positive: validation follows the schema, so a new field or operation is covered without a client change.
- Positive: GraphQL result coercion (spec §3.5) already defines the failure behaviour.
- Negative: each scalar needs a Rust type, a `@specifiedBy` description and a Relay type mapping.

### Browser checks generated from the schema and operation documents

- Positive: coverage follows the documents automatically.
- Negative: each format is implemented twice, in Rust and in TypeScript, and the browser check only detects what the
  server already prevents.

### Hand-written browser projections per operation

- Positive: no schema change is needed.
- Negative: projections drift from their selections, and an operation without an entry is silently unchecked.

## More information

Reconsider the GraphQL server library if clients need to submit their own documents, or if a Juniper release fixes both
input-default coercion and indexed error paths. Juniper 0.18.0 was evaluated: it rejects an omitted non-null input field
that has a default, and its execution paths omit list indexes, which Relay needs to place field errors.

Reconsider device-token access to GraphQL when the first real third-party client exists. That client would submit its
own documents, which trusted-document admission does not allow.

- [GraphQL specification, Scalars (September 2025)](https://spec.graphql.org/September2025/#sec-Scalars)
- [GraphQL specification, @specifiedBy](https://spec.graphql.org/September2025/#sec--specifiedBy)
