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
- Authentication, privacy and retained HTTP consumers keep their existing contracts.
- Accessible interaction and the enabled React Compiler remain selected independently of transport.

## Considered options

- Full browser GraphQL and Relay
- Retain REST and TanStack Query for browser application data
- GraphQL for the Library only, retaining TanStack Query elsewhere

## Decision outcome

Chosen option: **Full browser GraphQL and Relay**, because one normalised client store gives related work and
manifestation records consistent identities across every browser workflow, and a schema generated from the Rust service
replaces manual duplication between the service and its TypeScript consumers.

The code-first schema is implemented in Rust with async-graphql and async-graphql-axum, and the Relay compiler produces
operation artefacts and TypeScript types. Relay owns fetching, normalised server data and mutations for all browser
application data. Authentication protocols, redirects, OPDS and binary delivery keep their HTTP contracts.

### Consequences

- Positive: Relay provides one normalised owner for related records and generated browser operations.
- Negative: schema and operation generation add build integration and matching release artefacts.
- Negative: each workflow migrates as a whole, with explicit connection and aggregate updates.

## Pros and cons of the options

### Full browser GraphQL and Relay

- Positive: shared identities and compiled operations support related data across the complete browser application.
- Negative: every browser screen moves to GraphQL, including the account and settings screens that share no records with
  the Library.

### Retain REST and TanStack Query for browser application data

- Positive: the existing request and recovery lifecycle needs no transport migration.
- Negative: related records remain distributed across query projections with explicit invalidation.

### GraphQL for the Library only, retaining TanStack Query elsewhere

- Positive: a smaller initial migration limits the number of affected consumers.
- Negative: overlapping reader and admin records retain separate data owners as a permanent architecture.

## More information

Reconsider the GraphQL server library if clients need to submit their own documents, or if a Juniper release fixes both
input-default coercion and indexed error paths. Juniper 0.18.0 was evaluated on 2026-11-20: it rejects an omitted
non-null input field that has a default, and its execution paths omit list indexes, which Relay needs to place field
errors. async-graphql 7.2.1 was the latest async-graphql release on that date.

Reconsider device-token access to GraphQL when the first real third-party client exists. That client would submit its
own documents, which trusted-document admission does not allow.
