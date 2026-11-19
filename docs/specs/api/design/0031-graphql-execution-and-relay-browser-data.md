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
  - "REV-REQ-0018"
  - "REV-REQ-0019"
  - "REV-REQ-0020"
  - "REV-REQ-0021"
  - "REV-REQ-0023"
  - "REV-REQ-0035"
  - "REV-REQ-0036"
  - "REV-REQ-0079"
governed-by:
  - "REV-ADR-0077"
  - "REV-ADR-0079"
  - "REV-ADR-0080"
---

# GraphQL execution and Relay browser data

## Purpose and boundaries

The Rust service exposes authenticated browser application data at `POST /api/graphql`. Its schema exposes the viewer
and reader graph, protected mutations and session-bound operation resolution through shared operations. Library and
account screens retain their HTTP adapters and Query ownership. Authentication protocols, OPDS, binary delivery and
`/api/v1` keep their HTTP contracts.

## Structure

`backend/src/graphql/mod.rs` builds one async-graphql schema with query and mutation roots and no subscriptions.
Ordinary queries use `ReadScope`; mutations and `metadataOperation` use `WriteScope`, independently of HTTP POST. The
adapter uses `ApiJson<Envelope>` and async-graphql-axum's `GraphQLResponse`. Its OpenAPI router joins `pilot_router`
before the API CSP, CSRF and session layers. OpenAPI describes the HTTP envelope; the generated GraphQL schema describes
application fields.

The schema holds the execution policy, never caller identity. Each request supplies its own `CurrentUser` and
application state, pool, actual tower Session and request-local DataLoader to the executor. The viewer reads the
caller's display name through a transaction from `acquire_with_rls` and returns the encoded `User` identity. It exposes
no other account's data.

`backend/src/graphql/library.rs` defines Work, Manifestation, Shelf and Series objects and their connections.
`backend/src/services/reader.rs` owns the transport-independent reads used by HTTP adapters and graph fields.
`loaders.rs` batches keys through caller-scoped transactions and caches results only for one execution request. Every
GraphQL Work, Series and relation requires at least one manifestation visible to the caller, including typed and Node
lookups. Shelf reads retain explicit owner predicates.

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

oxlint loads `eslint-plugin-relay` 2.1.0 through its JS-plugin bridge and enforces `graphql-syntax`,
`no-future-added-value`, `generated-typescript-types`, `hook-required-argument` and `function-required-argument` on
frontend sources. `just js::schema-lint` runs GraphQL-ESLint 4.4.2 under ESLint over `backend/schema.graphql` only. It
checks Relay connection, edge and `PageInfo` shapes, forward pagination arguments and the plugin's recommended naming;
`@fetchable`'s `fetch__*` roots and `field_name` argument are exempt. The Relay compiler sets
`requireCustomScalarTypes`, so a custom scalar without a `customScalarTypes` mapping fails generation. Component tests
use `relay-test-utils` 21.0.1.

IDs are TypeIDs: a lowercase type prefix, `_`, then the record UUID in 26 lowercase characters of the TypeID base32
alphabet. The prefixes are `book` (Manifestation), `work`, `series`, `shelf`, `author`, `tag`, `genre`, `mood`,
`metadata_version` and `user`. Publisher suggestions carry no entity ID. `backend/src/graphql/id.rs` binds each prefix
to its type at compile time through `type-safe-id` 0.3.4 and refuses an ID longer than 80 characters before parsing. The
`Node` interface has exactly `id: ID!`. Nullable `node(id:)` and typed lookup roots return null for malformed,
wrong-type, overlong, hidden and missing records. The User variant resolves only the caller. Reading state has no global
ID.

Browser routes carry these IDs: `/b/:id`, `/series/:id` and `/shelves/:id` loaders pass the path parameter unaltered to
their query, and a null result is a not-found route. Links, Library rows, visit filters and the restoration anchor use
the IDs that queries return. The client converts an ID to a UUID only for display fallbacks that show a short UUID, and
converts a UUID to an ID only for Book links built from retained HTTP responses.

Work, Series, Shelf and Manifestation declare `@fetchable(field_name: "id")`. Their nullable concrete lookup roots are
`fetch__Work(id: ID!)`, `fetch__Series(id: ID!)`, `fetch__Shelf(id: ID!)` and `fetch__Manifestation(id: ID!)`. They
share the typed roots' read scope guards and visibility checks. Paging fragments on these types set
`@refetchable(preferFetchable: true)` so Relay refetches through the concrete root. The concrete parent preserves
async-graphql's page complexity multipliers when validating named fragment spreads. Query paging fragments use their
Query root directly.

`frontend/src/lib/relay/reader.ts` supplies Library, detail, shelves, series, active suggestions and label reads. The
command palette defines its search query, `CommandPaletteQuery`, beside the component that reads it. The basic detail
includes only the small pending-version count. The Versions tab requests and pages pending proposals when it opens.
Required Work authors, tags, shelves, shelf items and series works complete their bounded native pages. The
confirmed-account provider owns the shared environment.

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

Every connection and output list costs one plus its effective bound times child complexity. A capped relation is always
a connection with `pageInfo`, so nothing is silently truncated. Continuation cursors keep the remainder reachable.
Requests above a maximum fail validation. Validation and execution share the same bounds and size validators. Every
connection requires an explicit non-null `first`. GraphQL ignores `REVERIE_OPDS_PAGE_SIZE`.

| Connection                                           | Operation size | Maximum |
| ---------------------------------------------------- | -------------: | ------: |
| Work manifestations                                  |              2 |      20 |
| Work authors and contributors                        |              5 |      20 |
| Work identifiers                                     |              5 |      20 |
| Manifestation genres, moods, identifiers and ratings |              5 |      20 |
| Manifestation tags                                   |             10 |      50 |
| Pending metadata versions                            |             20 |      50 |
| Root books and shelves, series works and shelf items |             50 |     100 |

The sizes above are explicit representative operation arguments, not server defaults. Search retains its shared
20-result bound, suggestions their shared 10-result default and 1-20 clamp, and labels their validated input count up to
60. Zero-size connections still cost one. Checked arithmetic refuses overflow and excessive cost.

The complexity limit is 200,000 in both policies. async-graphql `Analyzer` measurements of every trusted operation use
maximum variable sizes: 100 for primary pages, 50 for pending pages and 60 IDs for labels. Literal sizes remain as
written. The maximum measured cost is 17,904 and the maximum measured depth is 8 across all 57 trusted documents. The
depth limit is 12. Library initial and pagination reads cost 7,201, Series initial reads cost 8,957, and Series
pagination costs 17,904. Sixty-ID label requests cost 121. Admission limits remain 200,000 and depth 12.

| Operation                          | Maximum cost | Depth |
| ---------------------------------- | -----------: | ----: |
| `readerLibraryQuery`               |        7,201 |     8 |
| `readerLibraryPaginationQuery`     |        7,201 |     8 |
| `readerSeriesWorksPaginationQuery` |       17,904 |     8 |
| `readerSeriesQuery`                |        8,957 |     8 |

Both policies refuse excess cost and depth. async-graphql excludes `__typename` from these counts. Axum's default 2 MiB
JSON-body limit still applies.

`versions.rs` resolves only a selected Manifestation `versions` field. The pending count is exact across all eligible
rows. The pending connection orders `last_seen_at DESC, id DESC`, with a parent-bound cursor, a per-parent SQL window
and PageInfo derived from exact count and bounded probes. Scalar and junction canonical pointers and JSON-null audit
rows remain excluded. No overall row cap hides the remainder. HTTP detail retains its existing 200-row payload cap.
Children return an empty result before any proposal count or value query, including typed, Node and nested reads.
Accepted counts use canonical state only. Basic detail selects the pending count without reading a proposal page.
Selecting the count and pending page uses two batched operations, each with canonical scalar, junction and pending SQL,
independent of parent count.

`mutations.rs` calls the shared metadata editing and Undo, reading and shelf operations. Typed patches preserve absent
and null values. Metadata save, reading and Undo take explicit nullable baselines. Review and shelves keep their
existing preconditions. Scope, adult status, caller visibility, shelf ownership and credential ceilings remain separate
checks. Exact Undo and resolution require the real live issuing browser session and retain expiry, budgets and
non-consuming refusals. Basic and device tokens may save metadata but receive no Undo capability.

A selected metadata snapshot pairs scalar values and bounded editable relation pages with one aggregate revision. Read
loaders batch core rows and selected pages in one caller-scoped REPEATABLE READ transaction. Write operations
materialise selected acknowledgements under their parent locks before commit. Scalar-only paths skip unrelated relation
projection loads. HTTP asks for its complete result. Whole-list editing needs every page of that field at one revision;
continuation pages remain available through the concrete Manifestation lookup. Selected pages are keyed by parent, field
or role, and effective `first`/`after` arguments, including distinct aliases.

Selection inspection uses async-graphql look-ahead. It expands fragments without applying their type conditions or
fragment-level `@include`/`@skip`, so it may load extra fields beyond the executed selection. Resolver defaults apply
where arguments are omitted. There is no custom fragment traversal.

Development raw queries can assign too little cost to named fragments spread beneath `node`'s Node interface in
async-graphql 7.2.1. Production admits only trusted documents, and none uses that pattern.

The Relay network sends generated IDs and variables through `apiFetch` with same-origin credentials and an abort signal.
It validates the response envelope before publication. The viewer has no custom scalar or editing snapshot. Reader
proposal JSON reaches the lazy Versions view. Editing documents validate their selected projections before publication.
Execution faults are errors and do not publish partial data or trigger a resend.

The book, series, shelf and shelves route queries, the fragments those pages read, and `CommandPaletteQuery` carry
`@throwOnFieldError`. A field error or data missing after a graph relationship change throws to the nearest error
boundary: the route `errorElement`, or the command palette's own boundary. The schema marks a field non-null when null
is not a valid domain value.

Each reader route loader starts its query with `loadQuery`, which retains the operation from load start. The loader
awaits the same in-flight request for its not-found check and breadcrumb title. Abort, failure, a not-found result or an
account change during the load releases the reference. Otherwise the page's `useQueryLoader` takes the reference and
releases it on unmount or replacement. The command palette loads its query from the input handler through
`useQueryLoader`. A newer query or a closed palette releases the previous reference, so an obsolete search contributes
no result.

`frontend/src/lib/relay/mutations.ts` contains selected title, form, author and reading reads and acknowledgements, plus
the other protected actions. `readerAuthorEditing` refetches through `fetch__Manifestation`. The network checks each
editing document's selected snapshot, domain IDs, UUID handles, strong validators, timestamps and PageInfo before
`sink.next`. `executeEditing` captures a detached raw execution payload, without using fragment reads, `fetchQuery`,
store lookups or `commitMutation.onCompleted`. `captureAuthorPages` accepts only a complete cursor chain with one pair
of domain IDs and one revision. Library cells, the metadata dialog, Versions actions and batch Set status use these
selected raw executions. Preferences read and sparse update share the canonical service with HTTP; defaults hide
Subtitle while explicit arrays, including an empty override, retain their meaning. Metadata refresh schedules the
canonical claim-preserving enrichment operation with write scope and non-child authorisation.

## Failure and recovery

Account cancellation completes waiting executions without acknowledging a payload and suppresses late publication.
Caller unsubscribe remains disposal. An originating 401 reaches its error observer before account cleanup aborts the
other executions. Temporary or uncertain editing failures expose explicit Retry of the retained tuple. Metadata
conflicts resolve the request UUID first; unknown evidence never advances the starting baseline. A matched Reload adopts
selected values and baseline together without a save. Validation retains correction input, and permission or
locked-field refusals retain their own feedback. Exact Undo adopts eligible operations once and retains original expiry.

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
