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

The Rust service exposes authenticated browser application data at `POST /api/graphql`. Its schema exposes the viewer
and reader graph through shared catalogue operations. Library and account screens retain their HTTP adapters and Query
ownership. Authentication protocols, OPDS, binary delivery and `/api/v1` keep their HTTP contracts.

## Structure

`backend/src/graphql/mod.rs` builds one async-graphql schema with an empty mutation root and no subscriptions. The query
roots require read scope through `ReadScope`, using the existing `CurrentUser` assertions. The adapter uses
`ApiJson<Envelope>` and async-graphql-axum's `GraphQLResponse`. Its OpenAPI router joins `pilot_router` before the API
CSP, CSRF and session layers. OpenAPI describes the HTTP envelope; the generated GraphQL schema describes application
fields.

The schema holds the execution policy, never caller identity. Each request supplies its own `CurrentUser` and
application state, pool and request-local DataLoader to the executor. The viewer reads the caller's display name through
a transaction from `acquire_with_rls` and returns the encoded `User` identity. It exposes no other account's data.

`backend/src/graphql/library.rs` defines Work, Manifestation, Shelf and Series objects and their connections.
`backend/src/services/reader.rs` owns the transport-independent reads used by HTTP adapters and graph fields.
`loaders.rs` batches keys through caller-scoped transactions and caches results only for one execution request.
Unprotected catalogue tables require permitted linked manifestations. Shelf reads retain explicit owner predicates.

Work holds shared metadata, contributors and the selected series. Manifestation holds file metadata and caller-owned
reading state. Contributor role and series position belong to edges. Metadata editing snapshots carry their matching
values, domain IDs and entity-tag from one database snapshot. They do not depend on later relation publication.

## Interfaces and dependencies

The importing backend declares async-graphql and async-graphql-axum 7.2.1. The frontend declares Relay runtime,
react-relay, compiler and Babel plugin 21.0.1. Vite+ applies the Relay Babel transform through `@rolldown/plugin-babel`
0.3.1 alongside `react({ compiler: true })` in production and Browser Mode. Generated TypeScript artefacts live in
`frontend/src/lib/relay/__generated__`.

`just rust::regen` exports `backend/schema.graphql`; `just rust::drift` compares it without writing.
`just js::relay-gen` generates operations and the document map from `frontend/relay.config.json`. It starts the map
empty to prune removed operations. `just js::relay-check` generates in a temporary directory and compares both the map
and artefacts. An empty operation selection fails. Frontend build and test recipes run that check. Path filters select
backend checks for frontend operations and frontend checks for GraphQL schema changes.

IDs use standard padded base64 of `Type:<uuid>`, with distinct Work, Manifestation, Shelf, Series and User prefixes. The
`Node` interface has exactly `id: ID!`. Nullable `node(id:)` and typed lookup roots return null for malformed,
wrong-type, hidden and missing records. The User variant resolves only the caller. Reading state has no global ID.

`frontend/src/lib/relay/reader.ts` contains representative Library, detail, versions, shelf, series, discovery and Node
queries. The Library and basic detail fragments omit versions. These operations have no UI consumers and publish no
reader records in a browser workflow.

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

Every connection and output list costs one plus its effective bound times child complexity. Page arguments use the
configured page size as both default and ceiling. Validation and execution share the size validator. A validation
extension scopes the request's configured ceiling through Tokio task-local storage for async-graphql's synchronous cost
callbacks. The scope ends with validation. Search uses its shared 20-result cap, suggestions use their shared 10-result
default and 1-20 clamp, labels use the validated input count up to 60, and pending versions use their 200-row per-parent
cap. Zero-size connections still cost one. Checked arithmetic refuses overflow and excessive cost.

The complexity limit is 14,223,380 in both policies. Temporary async-graphql `Analyzer` measurements of the generated
operations use the default 50-row page and default relation bounds. The Library measurement executes against 50 rows.
The measured maximum M is 11,378,704. Its 25 percent margin, rounded up, is 2,844,676.

| Operation                |       Cost | Depth |
| ------------------------ | ---------: | ----: |
| `readerLibraryQuery`     |    225,801 |     8 |
| `readerDetailQuery`      |      4,509 |     6 |
| `readerVersionsQuery`    |      1,606 |     4 |
| `readerShelfQuery`       |    225,808 |     9 |
| `readerShelvesQuery`     |        701 |     4 |
| `readerSeriesQuery`      | 11,378,704 |    12 |
| `readerSearchQuery`      |     90,201 |     7 |
| `readerSuggestionsQuery` |        132 |     2 |
| `readerNodeQuery`        |      6,280 |     6 |
| `viewerQuery`            |          4 |     2 |

The finite depth limit is 12, the maximum observed depth of these operations. It admits the complete series page and its
nested file relations. A separate measurement with a configured seven-row default costs 4,824 and returns seven Library
rows, confirming that validation follows configuration. All representative operations execute under the calibrated
policy. The existing admission check proves over-budget and depth refusal in both policies. async-graphql excludes
`__typename` from these counts. Axum's default 2 MiB JSON-body limit still applies.

`versions.rs` resolves only a selected Manifestation `versions` field. Adults load canonical scalar pointers, junction
pointers and pending proposals in three set-based statements, independent of parent count. The pending query keeps its
per-parent 200-row cap, newest-first ordering, canonical exclusions and JSON-null exclusion. Children return an empty
pending result before any proposal-value query, including typed, Node and nested reads. Accepted counts use canonical
state only. Temporary SQL counters measure zero versions statements for basic detail and three for selected versions,
including one proposal statement. Selecting two parents also executes three statements.

The Relay network sends generated IDs and variables through `apiFetch` with same-origin credentials and an abort signal.
It validates the response envelope before publication. The viewer has no custom scalar or editing snapshot. Reader
proposal JSON and revision-bearing snapshots have no browser consumer. Their runtime validation belongs at the
publication and use boundary. Execution faults are errors and do not publish partial data or trigger a resend.

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
