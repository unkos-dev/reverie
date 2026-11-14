---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0031"
title: "GraphQL execution and Relay browser data"
satisfies:
  - "REV-REQ-0005"
  - "REV-REQ-0006"
  - "REV-REQ-0014"
  - "REV-REQ-0015"
  - "REV-REQ-0016"
  - "REV-REQ-0017"
  - "REV-REQ-0021"
  - "REV-REQ-0023"
  - "REV-REQ-0035"
  - "REV-REQ-0036"
  - "REV-REQ-0079"
governed-by:
  - "REV-ADR-0077"
---

# GraphQL execution and Relay browser data

## Purpose and boundaries

The Rust service exposes authenticated browser application data at `POST /api/graphql`. Its viewer query establishes the
request, generation and Relay network contracts. Library and account screens retain their HTTP adapters and Query
ownership. Authentication protocols, OPDS, binary delivery and `/api/v1` keep their HTTP contracts.

## Structure

`backend/src/graphql/mod.rs` builds one async-graphql schema with an empty mutation root and no subscriptions. The
viewer resolver requires read scope through `ReadScope`, using the existing `CurrentUser` assertions. The adapter uses
`ApiJson<Envelope>` and async-graphql-axum's `GraphQLResponse`. Its OpenAPI router joins `pilot_router` before the API
CSP, CSRF and session layers. OpenAPI describes the HTTP envelope; the generated GraphQL schema describes application
fields.

The schema holds the execution policy, never caller identity. Each request supplies its own `CurrentUser` and
application pool to the executor. The viewer reads the caller's display name through a transaction from
`acquire_with_rls` and returns an ID prefixed with `User:`. It exposes no other account's data.

## Interfaces and dependencies

The importing backend declares async-graphql and async-graphql-axum 7.2.1. The frontend declares Relay runtime,
react-relay, compiler and Babel plugin 21.0.1. Vite+ applies the Relay Babel transform through `@rolldown/plugin-babel`
0.3.0 alongside `react({ compiler: true })` in production and Browser Mode. Generated TypeScript artefacts live in
`frontend/src/lib/relay/__generated__`.

`just rust::regen` exports `backend/schema.graphql`; `just rust::drift` compares it without writing.
`just js::relay-gen` generates operations and the document map from `frontend/relay.config.json`. It starts the map
empty to prune removed operations. `just js::relay-check` generates in a temporary directory and compares both the map
and artefacts. An empty operation selection fails. Frontend build and test recipes run that check. Path filters select
backend checks for frontend operations and frontend checks for GraphQL schema changes.

## Data and state

`backend/graphql-documents.json` maps Relay's SHA256 operation IDs to query text. The server embeds it at compile time.
The image's backend stage copies the backend directory, carrying the matching map without a runtime registry. Cached
clients with unknown IDs receive a refusal; the network never falls back to text. The Relay compiler's
`noSourceControl: true` leaves staging to explicit Git commands.

`active-user.ts` owns account confirmation and loss. Confirmation establishes an account-scoped Relay environment;
repeated confirmation of that account reuses it. Account replacement or loss aborts its requests, clears its records and
releases that environment before creating another. An identity epoch prevents a delayed `/auth/me` completion from
re-establishing an account after sign-out. The existing Query client owns all remaining HTTP application data. CSRF
state remains in `api/csrf.ts`, outside Relay records.

## Runtime behaviour

Release builds always select production policy; debug builds select development policy. There is no environment setting.
Production refuses submitted text and unknown IDs before execution, with no runtime registration. Development accepts
text for tooling. Both modes validate variables and enforce root-field scope. Production disables schema introspection;
development retains it. `__typename` remains available.

The schema limits counted selection depth to eight and complexity to 64. The viewer operation has depth two and
complexity four. async-graphql excludes `__typename` from these counts. Its aliases remain subject to Axum's default 2
MiB JSON-body limit. The viewer has no recursive field; a test uses its real fields with a depth limit of one to prove
the library's depth refusal in both policies.

The Relay network sends generated IDs and variables through `apiFetch` with same-origin credentials and an abort signal.
It validates the response envelope before publication. No custom scalar or revision-bearing editing snapshot exists in
the viewer schema. Execution faults are errors and do not publish partial data or trigger a resend.

## Failure and recovery

JSON transport failures retain Problem Details and the extractor's 400, 413, 415 or 422 status. Missing or invalid
credentials fail through `CurrentUser`. Resolver scope failures carry the `forbidden` extension code. Internal resolver
failures carry `internal` and the fixed message "An internal error occurred."; private diagnostics stay in structured
server logs.

The browser preserves non-2xx HTTP errors regardless of body shape, and retains execution classifications and safe field
messages. Only an HTTP middleware CSRF refusal refreshes the token and sends the identical request once more. Execution
faults, partial results and uncertain writes never replay automatically. An HTTP 401 enters the existing once-guarded
account-loss funnel. A field-level denial does not prove session loss.

## Security and operations

Session-authenticated POST queries require the existing CSRF token. Session-less Basic and device-token requests retain
the credential-type exemption. Read-only tokens can query over POST; the HTTP method does not grant mutation capability.
Role, child status, ownership and credential ceilings remain independent of document admission and scope.

The adapter returns `Cache-Control: no-store` and receives the reserved-prefix API security headers. It logs neither raw
GraphQL documents nor variables. Schema and manifest drift checks protect release agreement; generation commits the
schema, operation artefacts and embedded map together.
