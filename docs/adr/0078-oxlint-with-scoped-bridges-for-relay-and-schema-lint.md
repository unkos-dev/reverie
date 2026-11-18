---
type: ADR
profile-version: 1
id: "REV-ADR-0078"
title: "oxlint with scoped bridges for Relay and schema lint"
status: "accepted"
recorded-on: "2026-11-18"
decided-on: "2026-11-17"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0030"
---

# oxlint with scoped bridges for Relay and schema lint

## Context and problem statement

Reverie lints JavaScript and TypeScript with oxlint. The decision this record supersedes chose oxlint with native rules
only, and rejected the JS-plugin bridge because it keeps the ESLint ecosystem installed.

Relay is the browser's GraphQL client, and its conventions now reach both the frontend sources and the exported schema
in `backend/schema.graphql`. The reader's Relay migration (#1881) hand-rolled state that Relay already provides and
shipped two defects that review did not catch. Nothing mechanical checks either side today. Relay documents
`eslint-plugin-relay` as its lint plugin, and oxlint has no native equivalent. The standard schema linter,
GraphQL-ESLint, parses `.graphql` files, and oxlint's JS plugins accept only JavaScript and TypeScript.

Which lint engines may Reverie run, and for which inputs?

## Decision drivers

- One lint engine for JavaScript and TypeScript, with oxlint's speed and type-aware rules.
- Relay conventions are checked mechanically on both sides of the API, not left to review.
- Use the tools Relay and the GraphQL community document, rather than hand-written rules.
- Each extra engine or bridge has a narrow scope and a condition for its removal.

## Considered options

- oxlint with the `eslint-plugin-relay` bridge, plus ESLint limited to the schema
- oxlint native rules only, relying on the Relay compiler and review
- oxlint with the `eslint-plugin-relay` bridge, deferring schema lint until oxlint supports other file formats

## Decision outcome

Chosen option: **oxlint with the `eslint-plugin-relay` bridge, plus ESLint limited to the schema**, because it is the
only option that checks Relay conventions on both the client and the schema with the documented tools.

oxlint remains the engine for all JavaScript and TypeScript. Its JS-plugin bridge loads `eslint-plugin-relay` for
frontend sources. ESLint runs only GraphQL-ESLint, and only over `backend/schema.graphql`. ESLint stays until the oxlint
that Vite+ bundles lets JS plugins lint non-JavaScript files; the schema lint then moves under oxlint and ESLint is
removed.

### Consequences

- Positive: frontend Relay code and the exported schema fail lint when they break Relay's conventions.
- Positive: no TypeScript reaches ESLint, so oxlint alone judges application code.
- Negative: a second lint engine and its dependency tree stay installed until oxlint can lint `.graphql` files.
- Negative: the JS-plugin API is alpha and outside semver, so a Vite+ bump can break the Relay rules.
- Negative: GraphQL-ESLint requires `graphql` 16, so the root workspace resolves `graphql` 16 beside the frontend's
  `graphql` 17.

## Pros and cons of the options

### oxlint native rules only, relying on the Relay compiler and review

- Positive: keeps the full-removal outcome of the superseded decision, with no bridge and no second engine.
- Negative: the compiler checks names and generated types, but not connection shapes, required hook arguments or schema
  conventions. Review missed defects of that kind.

### Deferring schema lint until oxlint supports other file formats

- Positive: no second engine.
- Negative: the schema stays unchecked for an unknown period while the native Relay migration reshapes it.

## More information

The debt entry [ESLint runs beside oxlint for schema lint](../../debt/2026-11-18-eslint-for-graphql-schema-lint.md)
records the lift condition and the removal steps.
