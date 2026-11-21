---
type: ADR
profile-version: 1
id: "REV-ADR-0083"
title: "Server-typed scalars and enums for GraphQL data formats"
status: "accepted"
recorded-on: "2026-11-20"
decision-makers:
  - "John Unkovich"
---

# Server-typed scalars and enums for GraphQL data formats

## Context and problem statement

Browser application data travels over GraphQL and Relay under REV-ADR-0077. Editing and display depend on values with
exact formats, such as entity-tags, identifiers and instants, and on closed sets of values, such as an Undo operation's
status. A browser check written per operation must name every field and skips any operation it does not list, while the
server already holds the typed value each field is produced from. Where should the formats of browser GraphQL data be
guaranteed, and what does the browser still check?

## Decision drivers

- Each format is defined once, by the type that produces or accepts the value.
- No operation can bypass format validation by being absent from a list of checks.
- A malformed input is refused before any write and is not offered as a temporary failure to retry.
- A user-entered value that breaks a domain rule still returns feedback on its own field.

## Considered options

- Server-typed scalars and enums, with envelope checks in the browser
- Browser checks generated from the schema and operation documents
- Hand-written browser projections per operation

## Decision outcome

Chosen option: **Server-typed scalars and enums, with envelope checks in the browser**, because each format is then
defined once, in the Rust type that produces or accepts the value, and GraphQL result coercion turns any breach into an
execution error on that field, so no operation can skip the check.

A format-bearing value in browser GraphQL data is a custom scalar backed by a Rust type, and a closed set of values is
an enum. Format validation belongs to those server types, and browser validation covers the transport response and the
execution-error envelope. A user-entered value whose refusal must return field feedback stays a string and is validated
by its domain service.

### Consequences

- Positive: a format is stated once, and the schema and generated types carry it to every client.
- Positive: a coercion failure reaches the client as an execution error on the field, so a malformed value is never
  published as data.
- Positive: the browser needs no hand-written format projection per operation.
- Negative: until a released server honours `onError`, the browser refuses a whole response with any error, so one
  field's coercion failure fails the operation.
- Negative: user-entered formats such as ISBNs and publication dates stay strings, so the schema does not express them.

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

- [GraphQL specification, Scalars (September 2025)](https://spec.graphql.org/September2025/#sec-Scalars)
- [GraphQL specification, @specifiedBy](https://spec.graphql.org/September2025/#sec--specifiedBy)
