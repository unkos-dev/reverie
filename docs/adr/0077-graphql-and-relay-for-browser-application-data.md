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
  - "REV-ADR-0016"
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

- GraphQL and Relay for browser application data
- Retain REST and TanStack Query for browser application data
- GraphQL for the Library only, retaining TanStack Query elsewhere

## Decision outcome

Chosen option: **GraphQL and Relay for browser application data**, with the code-first schema implemented in Rust using
async-graphql and async-graphql-axum. Relay owns fetching, normalised server data and mutations for migrated browser
workflows. React Aria owns accessible interaction and virtualisation. The existing oxc React Compiler remains enabled.

The Rust schema produces the GraphQL contract, and the Relay compiler produces operation artefacts and TypeScript types.
The target covers all browser application data. TanStack Query remains available only to remaining HTTP workflows; each
migrated workflow has one server-data owner. Authentication protocols, redirects, OPDS and binary delivery retain their
HTTP contracts. Retained HTTP APIs keep code-first OpenAPI documentation.

GraphQL boundary checks validate transport failures, execution-error envelopes, custom scalar formats and
revision-bearing editing snapshots. Generated types do not substitute for those runtime checks. Zod continues to
validate forms, URL inputs and retained REST response boundaries.

Production executes only build-generated trusted documents and disables schema introspection. Development tooling
retains introspection. Relay's `__typename` remains available. Arbitrary client documents and unknown IDs cannot execute
or register themselves at runtime; an automatic persisted-query cache is not this allowlist. The released frontend and
server carry matching generated document artefacts without raw-query fallback for older clients.

The transport change preserves caller visibility, scope and role checks, session CSRF, matched editing values and
preconditions, authoritative recovery and exact saved-field Undo. Live cache updates do not advance a retained draft.
Ordinary asynchronous queries, cursor pagination and query preparation do not depend on incremental delivery or
subscriptions.

### Consequences

- Positive: Relay provides one normalised owner for related records and generated browser operations.
- Positive: retained HTTP APIs keep their URL-path major version and code-first OpenAPI 3.1 contract. REST conventions
  in REV-ADR-0011 still govern those APIs.
- Positive: migrating a browser workflow to GraphQL removes no REST operation. The versioned `/api/v1` data surface
  remains the contract for device-token and other non-browser clients under REV-ADR-0011 and REV-ADR-0056, and retiring
  any of it requires a separate compatibility decision.
- Positive: the selected validation boundary preserves runtime checks for editing snapshots and scalar formats.
- Negative: schema and operation generation add build integration and matching release artefacts.
- Negative: each workflow migrates as a whole, with explicit connection and aggregate updates.
- Negative: TanStack Query and Relay coexist until the last browser workflow migrates. TanStack Query is then removed;
  coexistence is not a permanent alternative architecture.
- Negative: trusted documents require matching frontend and server releases without raw-query fallback.

## Pros and cons of the options

### Full browser GraphQL and Relay

- Positive: shared identities and compiled operations support related data across the complete browser application.
- Negative: compatibility with the existing Vite+ and oxc React Compiler configuration remains unproved.

### Retain REST and TanStack Query for browser application data

- Positive: the existing request and recovery lifecycle needs no transport migration.
- Negative: related records remain distributed across query projections with explicit invalidation.

### GraphQL for the Library only, retaining TanStack Query elsewhere

- Positive: a smaller initial migration limits the number of affected consumers.
- Negative: overlapping reader and admin records retain separate data owners as a permanent architecture.
