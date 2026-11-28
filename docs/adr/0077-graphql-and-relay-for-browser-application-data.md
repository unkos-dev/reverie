---
type: ADR
profile-version: 1
id: "REV-ADR-0077"
title: "GraphQL and Relay for browser application data"
status: "accepted"
recorded-on: "2026-11-14"
decided-on: "2026-11-14"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0010"
---

# GraphQL and Relay for browser application data

## Context and problem statement

The browser application reads related library records and edits shared metadata alongside caller-owned reading state.
Its data layer needs one owner for fetching and mutation publication while retaining matched editing baselines and
authoritative recovery. Which transport and client data library should own browser application data?

## Decision drivers

- Related work and manifestation records need consistent identities across browser workflows.
- Editing values and their matching preconditions stay together through errors and live updates.
- Generated contracts reduce manual duplication between the Rust service and TypeScript consumers.
- Browser interactions use plain HTTP only where a standard or protocol requires it.
- Accessible interaction and the enabled React Compiler remain selected independently of transport.

## Considered options

- Full browser GraphQL and Relay
- Retain REST and TanStack Query for browser application data
- GraphQL for the Library only, retaining TanStack Query elsewhere

## Decision outcome

Chosen option: **Full browser GraphQL and Relay**, because one normalised client store gives related work and
manifestation records consistent identities across every browser workflow, and a schema generated from the Rust service
replaces manual duplication between the service and its TypeScript consumers.

Browser application data moves to a code-first GraphQL schema and compiled Relay operations across the whole browser
application, including account and authentication screens. A browser interaction stays on plain HTTP only where a
standard or protocol requires it. The OIDC authorisation code flow is "a redirection-based flow" (RFC 6749 §4.1). File
downloads and cover images reach the browser by URL as bytes, and GraphQL over HTTP defines JSON responses.

### Consequences

- Positive: Relay provides one normalised owner for related records and generated browser operations.
- Negative: device-token scripts and other non-browser clients keep reaching Reverie through `/api/v1` until a separate
  decision gives them GraphQL access, because trusted documents cannot serve third-party callers.
- Negative: schema and operation generation add build integration and matching release artefacts.
- Negative: each workflow requires explicit connection and aggregate updates when moved to Relay.
- Negative: moving workflows incrementally requires running TanStack Query alongside Relay until every browser workflow
  uses Relay; coexistence is not a permanent target architecture.
- Negative: trusted-document execution requires matching frontend and server releases without raw-query fallback.

## Pros and cons of the options

### Full browser GraphQL and Relay

- Positive: shared identities and compiled operations support related data across the complete browser application.
- Negative: Relay's compiler and Babel transform add build-pipeline coupling beside Vite+ and the React Compiler.
- Negative: async-graphql is selected over Juniper because Juniper rejects omitted non-null input fields that declare a
  default and omits list indexes from execution error paths that Relay requires.

### Retain REST and TanStack Query for browser application data

- Positive: the existing request and recovery lifecycle needs no transport migration.
- Negative: related records remain distributed across query projections with explicit invalidation.

### GraphQL for the Library only, retaining TanStack Query elsewhere

- Positive: a smaller initial migration limits the number of affected consumers.
- Negative: overlapping reader and admin records retain separate data owners as a permanent architecture.

## More information

Reconsider the GraphQL server library if clients need to submit arbitrary documents, or if Juniper resolves both
input-default coercion and indexed error paths.

Device-token access to GraphQL needs its own decision. The graphql.org security guidance says trusted documents "can't
be used for public APIs", because third-party operations are not known in advance, and names depth, breadth, batch and
rate limits as the defences for a public schema.
