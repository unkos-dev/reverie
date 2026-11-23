---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0020"
title: "Shelves"
satisfies:
  - "REV-REQ-0057"
  - "REV-REQ-0059"
  - "REV-REQ-0093"
governed-by:
  - "REV-ADR-0056"
  - "REV-ADR-0011"
  - "REV-ADR-0082"
---

# Shelves

Shelves are the reader's own curation buckets: named sets of manifestations a caller builds by hand. A shelf defines
membership, never row order. This Design covers the `/api/v1/shelves*` CRUD and item surface, the GraphQL shelf reads
and mutations, the `shelves` and `shelf_items` tables they share, the handler-level ownership predicate that stands in
for row-level security on those two tables, and the client pages and consumer surfaces that read and write through them.

## Purpose and boundaries

This subject owns shelf create, read, update and delete; the `is_system` flag and the immutability it imposes on a
system-managed shelf; shelf items and their stable presentation order (`added_at ASC, manifestation_id ASC`); the
handler-level `WHERE user_id = caller` ownership predicate that every mutating and shelf-scoped read handler applies in
place of row-level security; the row-level-security-scoped manifestation-visibility probe `add_shelf_item` runs before
it will add an item; the GraphQL `shelf` and `shelves` roots, the `Shelf` type with its `items` connection, and the
`createShelf`, `renameShelf`, `deleteShelf`, `addShelfItem` and `removeShelfItem` mutations; the `shelfLabels` lookup;
and the client's two shelf pages (`frontend/src/pages/shelves/ShelvesListPage.tsx`,
`frontend/src/pages/shelves/ShelfDetailPage.tsx`) and the `readerShelves` shelf-list fragment they and the Library page
read. The GraphQL execution policy, global object IDs and the mutation result convention belong to the Design "GraphQL
execution and Relay browser data".

It does not own row-level security itself: the `app.current_user_id` mechanism, the pools, and the policy census belong
to the Design "Row-level security and database context". This subject is one of several call sites where
`crate::db::acquire_with_rls` is used for something other than scoping the resource the transaction itself queries —
like the enrichment dry-run's own visibility check (`backend/src/routes/enrichment.rs`) — `add_shelf_item`'s probe uses
it only to answer "can this caller see this manifestation", never to scope `shelves` or `shelf_items`, which carry no
row-level-security policy at all. It does not own the three-axis scope, role, and ownership model or the cross-resource
map of which tables use row-level security and which use a handler predicate; that narrative belongs to the Design
"Authorization axes", which names `shelves`/`shelf_items` as the ownership-predicate side of that map and cites this
subject's code as its evidence. It does not own entity-tag mechanics: no shelf endpoint emits an `ETag` or accepts
`If-Match`, and the Design "Conditional requests and optimistic concurrency" owns the mechanisms the other resources
use. It does not own the keyset cursor framework, the `build_next_url` helper it borrows from
`backend/src/routes/library/mod.rs`, or the `split_page` helper in `backend/src/services/reader.rs`, all owned by the
Design "Books list query contract", or that Design's library filter grammar's `shelf` and `shelf_any` predicates, which
scope a books-list query to owned shelf membership and are referenced as a dependent below.

Depends on: `CurrentUser` (`backend/src/auth/middleware.rs`) for identity and its `require_scope`/`require_not_child`
assertions; `crate::db::acquire_with_rls` for the `add_shelf_item` visibility probe only; `ShelfCursor` and
`ShelfItemCursor` (`backend/src/routes/cursor.rs`), `build_next_url` (`backend/src/routes/library/mod.rs`) and
`split_page` (`backend/src/services/reader.rs`) for keyset pagination; the `manifestations` table, read-only, for the
visibility probe and the item foreign key.

Depended on by: the OPDS shelf-scoped feed routes (`backend/src/routes/opds/shelves.rs` and neighbours), which read the
same two tables directly by SQL rather than through this module's Rust functions, and are described by the OPDS
catalogue subject, not here — the OPDS EPUB download route is not shelf-scoped and never queries `shelves` or
`shelf_items` itself, so it is not a dependent of this subject; the Design "Books list query contract"'s shelf filters,
which require ownership of every requested shelf and match no rows if any ID is foreign or unknown; and, on the client,
the Library filter panel's shelf facet, the filter chips row, the Library shelf header, and the library batch bar's "add
selection to shelf" action — all described below only as consumers, since none of them owns shelf state.

## Structure

- `backend/src/routes/shelves/mod.rs` holds the seven HTTP adapters: `list_shelves`, `create_shelf`, `rename_shelf`,
  `delete_shelf`, `get_shelf_with_items`, `add_shelf_item`, and `remove_shelf_item`. Its write operations delegate to
  `backend/src/services/shelves.rs`, which applies the boundary this Design names: no row-level security on `shelves` or
  `shelf_items`, ownership enforced by an explicit `WHERE user_id = $current_user` predicate in every operation that
  touches an existing row, and a mismatched predicate resolving to `AppError::NotFound` so a foreign shelf's existence
  is never distinguishable from its absence.
- `backend/src/models/shelf.rs` holds the two response types, `Shelf` and `ShelfItem`. Both are `#[non_exhaustive]` and
  carry no pagination fields. The shared reads (`shelves`, `shelf_items`) and the `ShelfDetailResponse` envelope live in
  `backend/src/services/reader.rs`. The HTTP adapters own `ShelfListResponse`, the request parameters and the `Link`
  header.
- `backend/src/routes/cursor.rs` declares `ShelfCursor` and `ShelfItemCursor` alongside the books list engine's
  `SortCursor`, in the same module and the same opaque, tagged wire format (`sh|…` and `si|…`, base64url-unpadded). The
  GraphQL `shelves` connection tags its `ShelfCursor` values `gsh|…`. `ShelfCursor` carries `(is_system, name, id)`,
  matching the shelf list's `ORDER BY is_system DESC, name ASC, id ASC`; because that sort is mixed-direction, the
  keyset predicate `services::reader::shelves` builds is a two-arm `OR`
  (`is_system < $1 OR (is_system = $1 AND (name, id) > ($2, $3))`), not a single row-tuple comparison. `ShelfItemCursor`
  carries the shelf ID and `(added_at, manifestation_id)`, matching the items query's all-ascending sort; the
  `manifestation_id` tiebreaker is load-bearing because `added_at` is not unique per shelf. The
  `idx_shelf_items_shelf_keyset` index over `(shelf_id, added_at, manifestation_id)` serves that sort.
- Nothing in `routes/shelves/mod.rs` builds an entity-tag or parses `If-Match`; shelf responses carry no `ETag` header.
- Browser shelves share one account-scoped Relay environment. The route loaders prepare native query references for
  `ShelvesListPage` and `ShelfDetailPage`; their primary read failures reach the owning route. The shelf list and item
  connections page to completion through native pagination fragments without another Load more control.
- `readerShelves` in `frontend/src/lib/relay/reader.ts` is the one shelf-list connection. `readerShelvesQuery` spreads
  it for `ShelvesListPage`, and `LibraryPageQuery` spreads it for the Library page. Each page reads it with
  `usePaginationFragment` and loads every page in 50-shelf steps. A failed first page fails the owning query, which
  reaches its route error page. On the Library page a failed continuation page stops the loop, keeps the loaded shelves,
  logs the failure and gives `FilterRail` and `BatchBar` their degraded shelf state; on `ShelvesListPage` it reaches the
  route error page. `FilterChips` and the Library shelf header name selected shelves through the Library label read, not
  through the shelf list.
- Shelf create appends the returned edge to the shelf list connection, so a new shelf shows last until the list next
  loads in server order. Rename updates the shelf record in place. Deletion removes the edge through `@deleteEdge`, and
  its updater invalidates the deleted shelf's record. A failed create, rename or delete acknowledgement refetches the
  shelf list with `network-only`, never replaying the write. Library membership writes run serially and retain
  per-target outcomes. Each publishes the shelf's count and, when that shelf's items are in the store, appends the item
  edge; the active filtered Library connection then refreshes.

### State-writer census

| State item | Where it lives | Writer(s) |
| ---------- | -------------- | --------- |
| `shelves.updated_at` | `shelves` row | The `shelves_set_updated_at` trigger, fired by any `UPDATE` |
| `shelves.is_system` | `shelves` row | The column default (`false`); no handler ever sets it |
| `shelf_items` row existence | `shelf_items` table | `add_shelf_item` (insert), `remove_shelf_item` (delete), `delete_shelf` through the `shelves` cascade, and a manifestation delete through the `manifestations` cascade |
| Browser shelf list | `readerShelves` connection in the Relay store | Its page loads, `createShelf` through `@appendEdge`, `deleteShelf` through `@deleteEdge`, and a recovery refetch |

No state item above has competing owners: the handlers named are its only application writers, a cascade is the database
acting on its own, and Relay applies each browser write to the one connection both pages read.

`shelves.updated_at` is written by the `shelves_set_updated_at` trigger (`BEFORE UPDATE`, running `set_updated_at()`),
which fires on any `UPDATE` to the row regardless of which column changed. `rename_shelf` relies on the trigger alone:
its own `UPDATE` sets only `name`, and the trigger supplies `updated_at`. `add_shelf_item` and `remove_shelf_item` each
issue `UPDATE shelves SET updated_at = now() WHERE id = $1`, so a membership change moves the shelf's `updated_at`.

`shelf_items` row existence changes only through `add_shelf_item`'s
`INSERT … ON CONFLICT (shelf_id, manifestation_id) DO NOTHING` (so a duplicate add is a no-op for membership) and
`remove_shelf_item`'s `DELETE`; no other handler inserts into or deletes from this table directly, though a shelf or
manifestation delete removes its rows through the cascading foreign keys.

`shelves.is_system` defaults to `false`. `create_shelf`'s `INSERT` never sets the column, and no seeding, first-login or
migration step sets it either, so only a direct SQL write creates a system shelf. A system shelf sorts first in
`list_shelves` (`ORDER BY is_system DESC`), refuses `rename_shelf` and `delete_shelf` with
`AppError::SystemShelfImmutable` (409), and shows a System badge in place of the client's rename and delete controls.

## Interfaces and dependencies

- `GET /api/v1/shelves` — list the caller's shelves, `read` scope, keyset-paginated over
  `(is_system DESC, name ASC, id ASC)`.
- `POST /api/v1/shelves` — create a shelf, `write` scope, adult-only (`require_not_child`).
- `PATCH /api/v1/shelves/{id}` — rename a non-system shelf, `write` scope, adult-only.
- `DELETE /api/v1/shelves/{id}` — delete a non-system shelf, `write` scope, adult-only.
- `GET /api/v1/shelves/{id}` — shelf identity plus one keyset-paginated page of items
  (`added_at ASC, manifestation_id ASC`), `read` scope, available to a child.
- `POST /api/v1/shelves/{id}/items` — add a manifestation, `write` scope, available to a child.
- `DELETE /api/v1/shelves/{id}/items/{manifestation_id}` — remove an item, `write` scope, available to a child.

Every operation's `security(...)` annotation lists the same scope across all three credential transports
(`session_cookie`, `device_token_bearer`, `opds_basic`); the Design "Authorization axes" covers why that array is one
list rather than three, and the deny-by-default grid that proves each declared scope has a working gate. The two
read-scope operations (`list_shelves`, `get_shelf_with_items`) call no `require_scope` themselves: `CurrentUser`'s
extraction already refuses a scopeless credential, so any resolved caller already holds at least `read`. `create_shelf`,
`rename_shelf` and `delete_shelf` additionally call `require_not_child`; `add_shelf_item` and `remove_shelf_item` do
not, so a child account can add and remove items on its own shelves and read them, but cannot create, rename, or delete
a shelf.

`ApiJson` and `ApiPath` (`backend/src/extract.rs`) are the request-body and path-parameter extractors every mutating
handler uses; their rejection behaviour is the API error contract's concern, referenced here only as the extractors in
use.

Browser readers use the generated shelf operations in `reader.ts` and mutations in `mutations.ts`, committed through
`useMutation` and `commitMutation`. Relay owns normalised records and connection membership. Mutation results publish
changed names, counts and edges.

Graph shelf roots and request-local loaders call the owner-scoped reads in `reader.rs`. `Shelf` has a TypeID domain ID
and exposes scalar shelf metadata plus a bounded `items(first:, after:)` connection. GraphQL shelves and items require
`first` from 0 through 100, independently of `REVERIE_OPDS_PAGE_SIZE`. The root `shelves` connection orders system
shelves first, then by name and ID. Items order by addition time, then manifestation ID, with the shelf ID bound into
the graph cursor. Every edge has its own cursor and PageInfo flags come from bounded SQL probes.

Typed and Node lookups return null for foreign, hidden, missing or invalid shelf identities. `shelfLabels(ids:)` decodes
each ID as a shelf ID, refusing a malformed one with `validation`, loads the shelves through the owner-scoped request
loader and returns the caller's own shelves in first-requested order; a foreign or missing shelf is omitted. Item
hydration rechecks manifestation visibility through the caller's transaction. Relay is the browser owner of these
records; read execution adds no shelf writer.

The Design "GraphQL execution and Relay browser data" lists the results of `createShelf`, `renameShelf`, `deleteShelf`,
`addShelfItem` and `removeShelfItem`
([Mutation results](../../api/design/0031-graphql-execution-and-relay-browser-data.md#mutation-results)). The mutations
call `services::shelves` with write scope and apply the same adult-only CRUD, owner predicates and child membership
permissions as HTTP. They take no shelf precondition. Browser workflows use them through their page and Library action
owners.

## Data and state

`shelves` and `shelf_items` carry no row-level-security policy. The `shelves` table's own comment records this: "No RLS.
Ownership enforced at application layer", with every query scoped by `user_id` (`backend/migrations/`
`20260810000000_initial_schema.up.sql`). `reverie_app` holds `SELECT, INSERT, DELETE, UPDATE` on both tables and
`reverie_readonly` holds `SELECT` alone; neither grant is conditioned on a policy, because neither table has one. A
`shelves` row belongs to exactly one `user_id` (`ON DELETE CASCADE` from `users`); a `shelf_items` row belongs to
exactly one `shelf_id` (`ON DELETE CASCADE` from `shelves`) and references one `manifestation_id` (`ON DELETE CASCADE`
from `manifestations`), with `(shelf_id, manifestation_id)` as the primary key, so a manifestation can appear on a shelf
at most once and a shelf's row count bounds its item count.

`GET /api/v1/shelves` and `GET /api/v1/shelves/{id}` share one page-size setting with the OPDS surface and the
books-list endpoint (`GET /api/v1/books`, `backend/src/routes/library/mod.rs`): `state.config.opds.page_size`,
operator-configurable through `REVERIE_OPDS_PAGE_SIZE` (default 50, see `backend/src/config/opds.rs`). Nothing in this
subject declares a page size of its own.

`GET /api/v1/shelves/{id}` reads the shelf row on every request, so each items page reports the shelf's `updated_at` at
the time of that request. The browser shelf detail page pages its items through `ShelfDetailPage`'s `readerShelfItems`
pagination fragment. The shelf list selects each shelf's ID, name, system flag and item count, and no timestamps.

## Runtime behaviour

**Renaming a shelf**, `PATCH /api/v1/shelves/{id}`:

1. `rename_shelf` checks `require_scope(Write)` and `require_not_child()`, then trims and validates the new name.
2. It opens a transaction and runs `SELECT is_system FROM shelves WHERE id = $1 AND user_id = $2 FOR UPDATE`. A row that
   does not exist, or exists under a different `user_id`, returns `AppError::NotFound` — the two cases are
   indistinguishable to the caller.
3. If `is_system` is true, the handler returns `AppError::SystemShelfImmutable` (409) without writing anything.
4. Otherwise it runs `UPDATE shelves SET name = $1 WHERE id = $2 AND user_id = $3 RETURNING …`. The
   `shelves_set_updated_at` trigger fires on this `UPDATE` and sets `updated_at` to the transaction's current time; the
   handler never sets that column itself.
5. It re-reads the item count with a separate `SELECT COUNT(*)` in the same transaction, commits, and returns the
   `Shelf` response carrying the trigger-set `updated_at`.

**Adding an item to a child's shelf**, `POST /api/v1/shelves/{id}/items`, exercised by
`add_shelf_item_404_when_child_cannot_see_manifestation`:

1. `add_shelf_item` checks `require_scope(Write)` only — a child may reach this handler.
2. It opens a short-lived transaction through `crate::db::acquire_with_rls(&state.pool, current_user.user_id)` and runs
   `SELECT id FROM manifestations WHERE id = $1` inside it. For a child caller, the `manifestations_select_child` policy
   (owned by the Design "Row-level security and database context") admits only manifestations already linked through one
   of that child's own `shelf_items` rows. A manifestation the child cannot see this way returns no row, and the handler
   returns `AppError::NotFound` — identical to the "shelf not owned" case, so a caller cannot distinguish "wrong shelf"
   from "manifestation not visible to you" by probing random manifestation ids.
3. That transaction is never committed or rolled back explicitly; it is a read-only probe and is dropped at the end of
   the block, which is sufficient because it made no writes.
4. Only once the probe passes does the handler open its own transaction on the plain pool, lock the target row with
   `SELECT id FROM shelves WHERE id = $1 AND user_id = $2 FOR UPDATE`, insert the item
   (`ON CONFLICT (shelf_id, manifestation_id) DO NOTHING`), bump `updated_at` explicitly, and commit.
5. A repeat call with the same manifestation id is a no-op for `shelf_items` (the `ON CONFLICT` clause), but step 4's
   `updated_at` bump still runs unconditionally, so the shelf's `updated_at` changes on a duplicate add even though its
   membership does not.

**Reading a shelf's items**, `GET /api/v1/shelves/{id}`:

1. `get_shelf_with_items` calls `services::reader::shelf_items`, which reads the shelf row under the
   `WHERE id = $1 AND user_id = $2` predicate; a missing or foreign shelf returns `AppError::NotFound`.
2. It selects `manifestation_id, added_at` from `shelf_items` for that shelf,
   `ORDER BY added_at ASC, manifestation_id ASC`, with one more row than the page size so the last row signals whether
   another page exists. A cursor adds the row-tuple predicate `(added_at, manifestation_id) > ($1, $2)`.
3. Two items added in the same instant keep a fixed relative order because `manifestation_id` breaks the tie, so a page
   boundary between them neither drops nor repeats a row.

## Failure and recovery

- **Foreign or missing shelf.** Every handler that targets an existing shelf by id resolves a missing row and a row
  owned by a different `user_id` to the same `AppError::NotFound` (404). No handler in this module returns `403` for an
  ownership mismatch.
- **System-shelf mutation.** `rename_shelf` and `delete_shelf` return `AppError::SystemShelfImmutable` (409) once
  ownership has already been confirmed; a caller learns "this shelf is mine but immutable" only after the ownership
  check, never before it. GraphQL reports the same order of checks as `NotFound` and `SystemShelfImmutable` results.
- **Cursor from another list.** `ShelfCursor::parse` accepts only `sh`, or `gsh` for the GraphQL `shelves` connection,
  and `ShelfItemCursor::parse` accepts only `si`. Each refuses a cursor minted by the books list or by the other shelf
  list, and `ShelfCursor::parse` also refuses one minted by the other API, with `AppError::Validation` (422).
  `ShelfItemCursor::parse` also compares the cursor's shelf ID with the requested shelf and refuses a mismatch as
  `CursorError::ShelfMismatch`, so `GET /api/v1/shelves/{id}` answers 422 to an item cursor cut from another shelf. The
  GraphQL `items` cursor binds its shelf ID and relation kind, and a cursor from another parent or relation is refused
  with the `invalid-cursor` code.
- **A child probing shelf existence via manifestation ids.** Covered in Runtime behaviour: the RLS-scoped probe in
  `add_shelf_item` and the ownership check both resolve to the same `AppError::NotFound`, so a child cannot distinguish
  "no such shelf" from "shelf is yours but that manifestation isn't visible to you".
- **A malformed timestamp in a reader response.** The GraphQL network validates the shelf detail query's selected
  timestamps before publication, and the failure reaches the shelf detail route.
- **A deleted shelf in a retained Library visit.** Deleting a shelf invalidates its record. A return to a Library visit
  that selected it finds the visit's lookup stale, fetches it, and drops the shelf from the visit before the first books
  request.
- **A failed acknowledgement after a committed create.** The owning page refetches its native shelf list without
  replaying the create. Entered form input and local error feedback remain available.

## Security and operations

Ownership of a `shelves` or `shelf_items` row is enforced entirely by the `WHERE user_id = $current_user` (or the
equivalent join through `shelves`) predicate each handler writes into its own query, because neither table carries a
row-level-security policy. Not every statement repeats the predicate: the shelf `DELETE` and the `updated_at` bump in
the item handlers run against the id alone, and inherit ownership from the ownership-bound `SELECT ... FOR UPDATE` that
precedes them in the same transaction. This differs from the row-level-security pattern the Design "Row-level security
and database context" covers for most other per-user tables. There is no database-level fallback if one of these
predicates is dropped: the `reverie_app` role's grant on both tables is unconditional, so a query missing its ownership
clause would compile, run, and return rows across every account. This subject uses row-level security only for
manifestation visibility, never to scope the shelf itself. The `add_shelf_item` row-level-security probe prevents an
existence-probing attack against manifestation visibility. The GraphQL `Shelf.items` read runs inside a caller-scoped
row-level-security transaction (`relation_pages` in `backend/src/services/reader.rs`), so its join to `manifestations`
returns only manifestations the caller can see, while its owner predicate scopes the shelf. This subject enforces
ownership on every route, whatever additional role gate three of them apply.

Read access to a shelf and its items requires no more than the operation's declared `read` scope and no role check
beyond ownership: a child account can view its own shelves and items. Adding and removing items likewise carry no
`require_not_child` gate, so a child can organise the membership of shelves it owns. Creating, renaming, and deleting a
shelf are adult-only (`require_not_child`), so a child cannot create a new shelf or remove or rename an existing one (a
shelf's `user_id` is fixed at creation and never reassigned).

Not applicable: this subject exposes no operational surface of its own beyond the endpoints already described — no
service to run, restart, or scale independently of the backend as a whole.
