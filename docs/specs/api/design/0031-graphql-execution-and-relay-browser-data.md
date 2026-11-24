---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0031"
title: "GraphQL execution and Relay browser data"
satisfies:
  - "REV-REQ-0019"
  - "REV-REQ-0021"
  - "REV-REQ-0023"
  - "REV-REQ-0035"
  - "REV-REQ-0079"
  - "REV-REQ-0082"
  - "REV-REQ-0083"
  - "REV-REQ-0084"
  - "REV-REQ-0085"
  - "REV-REQ-0086"
  - "REV-REQ-0087"
  - "REV-REQ-0088"
  - "REV-REQ-0089"
  - "REV-REQ-0090"
  - "REV-REQ-0091"
  - "REV-REQ-0095"
  - "REV-REQ-0096"
governed-by:
  - "REV-ADR-0077"
  - "REV-ADR-0079"
  - "REV-ADR-0080"
  - "REV-ADR-0081"
  - "REV-ADR-0082"
  - "REV-ADR-0083"
  - "REV-ADR-0084"
---

# GraphQL execution and Relay browser data

## Purpose and boundaries

The Rust service serves authenticated browser application data at `POST /api/graphql`. This subject owns the GraphQL
schema and its execution policy, the trusted-document manifest, and the browser's Relay environment and network. The
schema exposes the viewer and reader graph, protected mutations and session-bound operation resolution through the same
service operations that the HTTP adapters call. The Designs that own those operations, such as "Books list query
contract", "Shelves" and "Metadata review and editing", describe their domain rules. The Design "API error contract and
OpenAPI" owns the HTTP envelope and the execution error codes, and the Design "Conditional requests and optimistic
concurrency" owns editing baselines.

The account, administration, authentication, settings and token screens, and the Library's empty-state ingestion
summary, read HTTP endpoints through TanStack Query. Authentication protocols, OPDS, binary delivery and `/api/v1` are
HTTP contracts outside this subject.

## Structure

`backend/src/graphql/mod.rs` builds one async-graphql schema with query and mutation roots and no subscriptions.
Ordinary queries use `ReadScope`; mutations and `metadataOperation` use `WriteScope`, independently of HTTP POST. The
adapter uses `ApiJson<Envelope>` and async-graphql-axum's `GraphQLResponse`. Its OpenAPI router joins `pilot_router`
before the API CSP, CSRF and session layers. OpenAPI describes the HTTP envelope; the generated GraphQL schema describes
application fields.

The schema holds the execution policy, never caller identity. Each request supplies its own `CurrentUser` and
application state, pool, actual tower Session and request-local DataLoader to the executor. The viewer returns the
encoded `User` identity. Its `displayName` reads the caller's display name through a transaction from
`acquire_with_rls`, and its `libraryPreferences` reads the caller's preferences under read scope. It exposes no other
account's data.

`backend/src/graphql/library.rs` defines Work, Manifestation, Shelf and Series objects and their connections.
`backend/src/services/reader.rs` owns the transport-independent reads used by HTTP adapters and graph fields.
`loaders.rs` batches keys through caller-scoped transactions and caches results only for one execution request. Every
GraphQL Work, Series and relation requires at least one manifestation visible to the caller, including typed and Node
lookups. Shelf reads apply explicit owner predicates.

The label roots `authorLabels`, `seriesLabels` and `shelfLabels` accept any number of IDs, including none. Each returns
one record for every distinct ID the caller can see, in the order the IDs were first requested. A malformed ID is a
`validation` error; a hidden, foreign or missing ID is omitted. Series and shelf labels load through the request loader,
so every field of the returned record resolves. The suggestion roots return `Author`, `Series`, `Genre`, `Mood` and
`Tag` records in the order the suggestion service ranks them.

Work holds shared metadata, contributors and the selected series. Manifestation holds file metadata and caller-owned
reading state. Contributor role and series position belong to edges. `Manifestation.metadata` returns the current
`MetadataRevision`, which carries matching values, domain IDs, entity-tag and selected relation pages from one database
snapshot. Its relation entries, `RevisionContributor`, `RevisionGenre`, `RevisionMood` and `RevisionTag`, have no ID, so
Relay stores them under their revision; each links to its live record for display and navigation.

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

`frontend/src/lib/relay/reader.ts` supplies the Library rows, detail, shelves and series reads. The Library route module
`frontend/src/routes/library.tsx` defines `libraryBootstrapQuery`. `LibraryPage.tsx` defines the screen query
`LibraryPageQuery` and the `LibraryPage_labels` label fragment, `TypeaheadMultiSelect.tsx` defines the suggestions
fragment and `use-library-preferences.ts` defines the preferences fragment. The command palette defines its search
query, `CommandPaletteQuery`, beside the component that reads it. The basic detail includes only the small
pending-version count. The Versions tab requests and pages pending proposals when it opens. Required Work authors, tags,
shelves, shelf items and series works complete their bounded native pages. The confirmed-account provider owns the
shared environment.

### Custom scalars

The metadata operations carry three Reverie scalars. Each declares `@specifiedBy` with a URL to this section, and the
Relay compiler maps each to `string`.

- `ETag` is a strong RFC 9110 §8.8.3 entity-tag in its quoted wire form. Input accepts one strong tag and refuses the
  `*` wildcard, a weak `W/` tag, a list and an unquoted value. Output is the tag the server issued.
- `UUID` accepts only the 36-character hyphenated form as input, in either case. Output is lower-case hyphenated.
- `DateTime` is an instant. Input must be RFC 3339 text ending in `Z`. Output is RFC 3339 in UTC ending in `Z`, with
  fractional seconds only when they are not zero.

A value that fails input coercion is an execution error with code `validation` on the field that received it, so the
operation begins no write. Output values come from typed server state. `FieldOperationStatus` and `OperationOutcome` are
enums. ISBN-10, ISBN-13, publication date and language are `String`. Domain validation checks their input and returns
field feedback, and a stored year-only publication date reads as written. Reading revisions use `ETag` for validators
and `DateTime` for timestamps. Reading status and contributor role are enums.

### Global object IDs

IDs are TypeIDs (TypeID specification 0.3.0): a lowercase type prefix, `_`, then the record UUID in 26 lowercase
characters of the TypeID base32 alphabet. The prefixes are `book` (Manifestation), `work`, `series`, `shelf`, `author`,
`tag`, `genre`, `mood`, `metadata_version` and `user`. `metadata_rev` (MetadataRevision) carries a name-based UUIDv8
(RFC 9562 §5.5 and Appendix B.2): SHA-256 over a fixed Reverie namespace ID and the revision's owner identities and
aggregate revisions. Each revision has its own ID, so a new revision is a new Relay record. `MetadataRevision` does not
implement `Node`, and no field fetches a past revision. `backend/src/graphql/id.rs` binds each prefix to its type at
compile time through `type-safe-id` 0.3.4. That crate's parser copies a mismatched prefix of any length, so `id.rs`
refuses an ID longer than 80 characters before parsing. The `Node` interface has exactly `id: ID!`. Nullable `node(id:)`
and typed lookup roots return null for malformed, wrong-type, overlong, hidden and missing records. The User variant
resolves only the caller. A `reading_rev` ID derives from the caller, manifestation and reading validator.
`ReadingRevision` is an immutable record reached through its manifestation, without a `Node` lookup.

Browser routes carry these IDs: `/b/:id`, `/series/:id` and `/shelves/:id` loaders pass the path parameter unaltered to
their query, and a null result is a not-found route. Links, Library rows, visit filters and the restoration anchor use
the IDs that queries return. The client converts an ID to a UUID only to show a short UUID, and converts a UUID to an ID
only for Book links built from HTTP responses.

Work, Series, Shelf and Manifestation declare `@fetchable(field_name: "id")`. Their nullable concrete lookup roots are
`fetch__Work(id: ID!)`, `fetch__Series(id: ID!)`, `fetch__Shelf(id: ID!)` and `fetch__Manifestation(id: ID!)`. They
share the typed roots' read scope guards and visibility checks. Paging fragments on these types set
`@refetchable(preferFetchable: true)` so Relay refetches through the concrete root. The concrete parent preserves
async-graphql's page complexity multipliers when validating named fragment spreads. Query paging fragments use their
Query root directly.

## Data and state

`backend/graphql-documents.json` maps Relay's SHA256 operation IDs to query text for the 51 trusted documents. The
server embeds it at compile time. The image's backend stage copies the backend directory, carrying the matching map
without a runtime registry. Cached clients with unknown IDs receive a refusal; the network never falls back to text. The
Relay compiler's `noSourceControl: true` leaves staging to explicit Git commands.

`active-user.ts` owns account confirmation and loss. Confirmation establishes an account-scoped Relay environment;
repeated confirmation of that account reuses it. Account replacement or loss aborts its requests, clears its records and
releases that environment before creating another. An identity epoch prevents a delayed `/auth/me` completion from
re-establishing an account after sign-out. TanStack Query owns the HTTP application data. CSRF state lives in
`api/csrf.ts`, outside Relay records.

## Runtime behaviour

Release builds always select production policy; debug builds select development policy. There is no environment setting.
Production refuses submitted text and unknown IDs before execution, with no runtime registration. Development accepts
text for tooling. Both modes validate variables and enforce root-field scope. Production disables schema introspection;
development enables it. `__typename` is available in both.

Fields with explicit weighting, which are every connection, the search, label and suggestion lists, and the Undo
operation lists `MetadataSaved.operations` and `MetadataOperation.operations` (bound 144), cost one plus their effective
bound times child complexity. Other fields use async-graphql's default complexity, including output lists such as
`FieldsLocked.fields` and the preference `hiddenColumns`. A capped relation is always a connection with `pageInfo`, so
nothing is silently truncated. Continuation cursors keep the remainder reachable. Requests above a maximum fail
validation. Validation and execution share the same bounds and size validators. Every connection requires an explicit
non-null `first`. GraphQL ignores `REVERIE_OPDS_PAGE_SIZE`.

| Connection                                           | Maximum `first` |
| ---------------------------------------------------- | --------------: |
| Work manifestations                                  |              20 |
| Work authors and contributors                        |              20 |
| Work identifiers                                     |              20 |
| Manifestation genres, moods, identifiers and ratings |              20 |
| Manifestation tags                                   |              50 |
| Pending metadata versions                            |              50 |
| Root books and shelves, series works and shelf items |             100 |

Search costs its shared 20-result bound, suggestions their shared 10-result default and 1-20 clamp, and labels the
number of IDs requested, with no cap of their own. A zero-size connection costs one. Checked arithmetic refuses overflow
and excessive cost.

Both policies refuse an operation whose cost exceeds the complexity limit of 200,000 or whose depth exceeds 12.
async-graphql excludes `__typename` from these counts. Axum's default 2 MiB JSON-body limit applies to the request body.

`versions.rs` resolves only a selected Manifestation `versions` field. The pending count is exact across all eligible
rows. The pending connection orders `last_seen_at DESC, id DESC`, with a parent-bound cursor, a per-parent SQL window
and PageInfo derived from exact count and bounded probes. Scalar and junction canonical pointers and JSON-null audit
rows are excluded. No overall row cap hides the remainder. HTTP detail caps its pending payload at 200 rows. Children
return an empty result before any proposal count or value query, including typed, Node and nested reads. Accepted counts
use canonical state only. Basic detail selects the pending count without reading a proposal page. Selecting the count
and pending page uses two batched operations, each with canonical scalar, junction and pending SQL, independent of
parent count.

`mutations.rs` calls the shared metadata editing and Undo, reading and shelf operations. Typed patches preserve absent
and null values. Metadata save, Undo and reading saves take a non-null baseline in their input. Review and shelf
mutations take no baseline. Scope, adult status, caller visibility, shelf ownership and credential ceilings are separate
checks.

### Mutation results

Shelf, metadata editing, reading, metadata refresh and library preference mutations take one `input` object named
`<Mutation>Input` and return a `<Mutation>Result` union of one success member, named for the event, and their expected
outcomes:

| Mutation | Success member and its fields | Outcome members |
| -------- | ----------------------------- | --------------- |
| `createShelf` | `ShelfCreated`: `shelf`, list `edge` | `ShelfNameInvalid` |
| `renameShelf` | `ShelfRenamed`: `shelf` | `ShelfNameInvalid`, `SystemShelfImmutable`, `NotFound` |
| `deleteShelf` | `ShelfDeleted`: `deletedShelfId` | `SystemShelfImmutable`, `NotFound` |
| `addShelfItem` | `ShelfItemAdded`: `shelf`, item `edge` | `NotFound` |
| `removeShelfItem` | `ShelfItemRemoved`: `shelf`, `deletedManifestationId` | `NotFound` |
| `updateMetadata` | `MetadataSaved`: `requestId`, `operations`, `manifestation` | `StaleBaseline`, `MetadataInvalid`, `FieldsLocked`, `EditTooLarge`, `NotFound` |
| `undoMetadata` | `MetadataRestored`: `operation`, `manifestation` | `StaleBaseline`, `UndoUnavailable`, `UndoConflict`, `FieldsLocked`, `EditTooLarge`, `NotFound` |
| `acceptMetadataVersion` | `MetadataVersionAccepted`: `manifestation` | `FieldsLocked`, `MetadataInvalid`, `NotFound` |
| `rejectMetadataVersion` | `MetadataVersionRejected`: `manifestation` | `NotFound` |
| `revertMetadataField` | `MetadataFieldReverted`: `manifestation` | `FieldsLocked`, `MetadataInvalid`, `NotFound` |
| `lockMetadataField` | `MetadataFieldLockAdded`: `manifestation` | `NotFound` |
| `unlockMetadataField` | `MetadataFieldLockRemoved`: `manifestation` | `NotFound` |
| `updateReadingState` | `ReadingStateUpdated`: `manifestation` | `StaleBaseline`, `MetadataInvalid`, `NotFound` |
| `refreshMetadata` | `MetadataRefreshQueued`: `manifestationId` | `NotFound` |
| `updateLibraryPreferences` | `LibraryPreferencesUpdated`: `viewer` | `LibraryPreferencesInvalid` |

Outcome types are shared across mutations and each carries a caller-safe `message`. `ShelfNameInvalid` refuses an empty
shelf name, `LibraryPreferencesInvalid` refuses an invalid preference write, and `SystemShelfImmutable` refuses renaming
or deleting a system shelf. `MetadataInvalid` carries `fieldErrors`, a list of `field` and `message` pairs keyed by the
service field name, which is empty for a refusal that names no field. `FieldsLocked` carries `fields`, the canonical
keys of the refused locked fields. A malformed, foreign, missing or caller-invisible target is a `NotFound` member. A
service refusal that a mutation's union does not declare becomes an internal execution error. Forbidden and internal
failures are execution errors, as are an input that fails scalar coercion and a committed metadata write whose
manifestation cannot be read back. How each surface recovers from an execution error is described in
[Conditional requests and optimistic concurrency](0007-conditional-requests-and-optimistic-concurrency.md#failure-and-recovery)
for saves and Undo, and in [Metadata review and editing](../../metadata/design/0018-metadata-review-and-editing.md) for
review actions. Metadata success members return the `manifestation`. Its `metadata` field answers with the revision the
write projected inside its transaction, selected through look-ahead on `manifestation.metadata`. Relay normalisation
then points the manifestation at the new revision without an updater. Clients branch on `__typename` in `onCompleted`
and treat `onError` as the execution failure path. The Library edit owner handles typed refusals and retains input for
explicit recovery. `LibraryPreferencesUpdated.viewer` answers `libraryPreferences` from the row the write returned, so
the acknowledgement normalises into the viewer's preferences record. A refused preference write is
`LibraryPreferencesInvalid`. The reading acknowledgement returns the manifestation linked to the revision built from the
row its write returned. Exact Undo and resolution require the live browser session that issued the operation, and apply
its expiry, budgets and non-consuming refusals. Basic and device tokens may save metadata but receive no Undo
capability.

### Selected revisions

A selected metadata revision pairs scalar values and bounded editable relation pages with one aggregate revision. Read
loaders batch core rows and selected pages in one caller-scoped REPEATABLE READ transaction. Write operations
materialise selected acknowledgements under their parent locks before commit. Scalar-only paths skip unrelated relation
projection loads. HTTP asks for its complete result. Whole-list editing needs every page of that field at one revision;
continuation pages use the concrete Manifestation lookup. Selected pages are keyed by parent, revision, field and
effective pagination arguments. The same revision UUID also binds each cursor to its own relation list.

Selection inspection uses async-graphql look-ahead. It expands fragments without applying their type conditions or
fragment-level `@include`/`@skip`, so it may load extra fields beyond the executed selection. Resolver defaults apply
where arguments are omitted. There is no custom fragment traversal.

### Browser execution

The Relay network sends generated IDs and variables through `apiFetch` with same-origin credentials and an abort signal.
It validates the response envelope before publication. For each operation named in `editingProjections` or
`readerProjections` (`frontend/src/api/graphql.ts`), the network also checks the payload against that operation's
selected projection. Other operations publish after envelope validation alone. Reader proposal JSON reaches the lazy
Versions view. Execution faults are errors and do not publish partial data or trigger a resend.

Library screen and editing reads use `@throwOnFieldError` on queries and fragments. The directive fails a read with
missing selected data at the receiving error boundary. Envelope validation keeps execution-error data out of the store.
Relay can combine fields of one immutable revision ID. An incomplete read cannot form a baseline, and only pages of that
same revision can complete a whole-list edit.

The screen query contains the conditional, aliased `readerLibrary` connection. Table cells, Covers cards and batch
actions read their fragments. The displayed and candidate readers use `readerLibraryPaginationQuery` through
`usePaginationFragment`. The author editor loads `cellsAuthorsQuery` and continues through
`readerAuthorEditingPaginationQuery`. The labels and typeahead use their generated refetch queries. Library edits use
native `commitMutation` with success payload fragments. The edit owner tracks each cell's pending and recovery state
when virtualised rows unmount and keeps Undo pending through the eligibility read between mutations, beyond the lifetime
tracked by `useMutation`'s single `isInFlight` flag. The Library operations publish without operation-specific
projections. The Book dialog and Versions continue to use their own fragments and hooks.

The Library route loader confirms the account, retains `libraryBootstrapQuery` for the destination's visit owner and
fetches it with `fetchQuery` under `store-or-network`, resolving on completion with the last payload. It then starts
`LibraryPageQuery` with `loadQuery` under `store-or-network` and assigns that reference to the same owner, which
releases both on abort, failure, a changed account, a newer preparation of the same entry, or entry disposal. The Design
"Library filter and sort state" describes the destination, the restore check and the page's reads.

The environment's `missingFieldHandlers` holds one `pluralLinked` handler. For a root `authorLabels`, `seriesLabels` or
`shelfLabels` field it returns the requested IDs, and for every other field it returns nothing. Relay uses that answer
only when every returned ID is an existing record, and then publishes it into the store, so a label lookup for records
that suggestions or the shelf list already loaded needs no request. An unanswered lookup is missing data, and its query
fetches. The store sets `queryCacheExpirationTime` to five minutes. A fresh retained Library return uses its existing
connection. Deleting a shelf invalidates the deleted shelf's record, so the next check of an operation that reads that
record reports it stale and fetches.

Each other reader route loader starts its query with `loadQuery`, which retains the operation from load start. The
loader awaits the same in-flight request for its not-found check and breadcrumb title. Abort, failure, a not-found
result or an account change during the load releases the reference. Otherwise the page's `useQueryLoader` takes the
reference and releases it on unmount or replacement. The command palette loads its query from the input handler through
`useQueryLoader`. A newer query or a closed palette releases the previous reference, so an obsolete search contributes
no result. The Versions tab loads the edit sheet's `mutationsFormQuery` through `useQueryLoader` from its Edit metadata
button, with the default `store-or-network` policy. The sheet reads it with `usePreloadedQuery` under Suspense and its
own error boundary. Its explicit Reload calls `fetchQuery` with `network-only` from the event handler, so the open form
stays mounted until the fresh revision arrives.

`frontend/src/lib/relay/mutations.ts` holds the Book form and other protected actions. The table-editing owner holds the
Library's save, reading, Undo and recovery operations. The Design "Conditional requests and optimistic concurrency"
describes how each editor captures its baseline.

`Viewer.libraryPreferences` and `updateLibraryPreferences` share the canonical preference service with HTTP. Density,
view and column names are enums, and each sort stack is a list of `LibrarySortLevel` values. Output omits a stored
column key outside `LibraryColumn`, and storage keeps it. An input changes only the groups it names, and `null` resets
density, view or sort. `columns` changes the named columns inside the single upsert, against the stored override or,
when none is stored, the defaults; `resetHiddenColumns` drops the override. An input that names nothing, an empty or
refused sort, a column named twice, or `columns` together with `resetHiddenColumns` is refused. The defaults hide
Subtitle, and an override that hides nothing shows every column. Metadata refresh schedules the canonical
claim-preserving enrichment operation with write scope and non-child authorisation.

## Failure and recovery

Account cancellation completes waiting executions without acknowledging a payload and suppresses late publication.
Caller unsubscribe is disposal. An originating 401 reaches its error observer before account cleanup aborts the other
executions. Editing failures follow the recovery flows in the Design "Conditional requests and optimistic concurrency"
([Runtime behaviour](0007-conditional-requests-and-optimistic-concurrency.md#runtime-behaviour)).

JSON transport failures return Problem Details with the extractor's 400, 413, 415 or 422 status. Missing or invalid
credentials fail through `CurrentUser`. Resolver scope failures carry the `forbidden` extension code. Internal resolver
failures carry `internal` and the fixed message "An internal error occurred."; private diagnostics stay in structured
server logs.

The browser surfaces a non-2xx HTTP response as `ApiError` whatever its body shape, and keeps execution classifications
and safe field messages on `GraphQLExecutionError`. Only an HTTP middleware CSRF refusal refreshes the token and sends
the identical request once more. Execution faults, partial results and uncertain writes never replay automatically. An
HTTP 401 enters the once-guarded account-loss funnel. A field-level denial does not prove session loss.

## Security and operations

Session-authenticated POST queries require the session's CSRF token. Session-less Basic and device-token requests are
exempt by credential type. Read-only tokens can query over POST; the HTTP method does not grant mutation capability.
Role, child status, ownership and credential ceilings are independent of document admission and scope.

The adapter returns `Cache-Control: no-store` and receives the reserved-prefix API security headers. It logs neither raw
GraphQL documents nor variables. Schema and manifest drift checks protect release agreement; generation commits the
schema, operation artefacts and embedded map together.
