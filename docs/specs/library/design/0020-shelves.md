---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0020"
title: "Shelves"
satisfies:
  - "REV-REQ-0057"
  - "REV-REQ-0059"
governed-by:
  - "REV-ADR-0056"
  - "REV-ADR-0011"
---

# Shelves

Shelves are the reader's own curation buckets: named sets of manifestations a caller builds by hand. A shelf defines
membership, never row order. This Design covers the `/api/v1/shelves*` CRUD and item surface, the `shelves` and
`shelf_items` tables it owns, the handler-level ownership predicate that stands in for row-level security on those two
tables, and the client pages and consumer surfaces that read and write through it.

## Purpose and boundaries

This subject owns shelf create, read, update and delete; the `is_system` flag and the immutability it imposes on a
system-managed shelf; shelf items and their stable presentation order (`added_at ASC, manifestation_id ASC`); the
handler-level `WHERE user_id = caller` ownership predicate that every mutating and shelf-scoped read handler applies in
place of row-level security; the row-level-security-scoped manifestation-visibility probe `add_shelf_item` runs before
it will add an item; and the client's two shelf pages (`frontend/src/pages/shelves/ShelvesListPage.tsx`,
`frontend/src/pages/shelves/ShelfDetailPage.tsx`) and the `frontend/src/api/shelves.ts` client that fronts them.

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
use. It does not own the keyset cursor framework or the `build_next_url`/`split_page` pagination helpers it borrows from
`backend/src/routes/library/mod.rs`, owned by the Design "Books list query contract", or that Design's library filter
grammar's `shelf` and `shelf_any` predicates, which scope a books-list query to owned shelf membership and are
referenced as a dependent below.

Depends on: `CurrentUser` (`backend/src/auth/middleware.rs`) for identity and its `require_scope`/`require_not_child`
assertions; `crate::db::acquire_with_rls` for the `add_shelf_item` visibility probe only; `ShelfCursor` and
`ShelfItemCursor` plus `build_next_url` and `split_page` (`backend/src/routes/cursor.rs`,
`backend/src/routes/library/mod.rs`) for keyset pagination; the `manifestations` table, read-only, for the visibility
probe and the item foreign key.

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
  `backend/src/services/shelves.rs`, which retains the boundary this Design names: no row-level security on `shelves` or
  `shelf_items`, ownership enforced by an explicit `WHERE user_id = $current_user` predicate in every operation that
  touches an existing row, and a mismatched predicate resolving to `AppError::NotFound` so a foreign shelf's existence
  is never distinguishable from its absence.
- `backend/src/models/shelf.rs` holds the two response types, `Shelf` and `ShelfItem`. Both are `#[non_exhaustive]` and
  carry no pagination fields. The shared reads and paged response envelopes (`ShelfListResponse`, `ShelfDetailResponse`)
  live in `backend/src/services/reader.rs`. HTTP adapters retain the request and header contract.
- `backend/src/routes/cursor.rs` declares `ShelfCursor` and `ShelfItemCursor` alongside the books list engine's
  `SortCursor`, in the same module and the same opaque, tagged wire format (`sh|…` and `si|…`, base64url-unpadded).
  `ShelfCursor` carries `(is_system, name, id)`, matching `list_shelves`'s `ORDER BY is_system DESC, name ASC, id ASC`;
  because that sort is mixed-direction, the keyset predicate the handler builds is a two-arm `OR`
  (`is_system < $1 OR (is_system = $1 AND (name, id) > ($2, $3))`), not a single row-tuple comparison. `ShelfItemCursor`
  carries `(added_at, manifestation_id)`, matching the items query's all-ascending sort; the `manifestation_id`
  tiebreaker is load-bearing because `added_at` is not unique per shelf. The `idx_shelf_items_shelf_keyset` index over
  `(shelf_id, added_at, manifestation_id)` serves that sort.
- Nothing in `routes/shelves/mod.rs` builds an entity-tag or parses `If-Match`; shelf responses carry no `ETag` header.
- Browser shelves share one account-scoped Relay environment. The route loaders prepare native query references for
  `ShelvesListPage` and `ShelfDetailPage`; their primary read failures reach the owning route. The shelf list and item
  connections page to completion through native pagination fragments without another Load more control.
- `frontend/src/api/shelves.ts` supplies the shared complete shelf-list read for `FilterRail`, `FilterChips` and
  `LibraryPage`. These auxiliary consumers keep their existing degraded state and log read failures. The module's
  consumer list records the primary, loader and auxiliary error tiers.
- Shelf create and rename publish returned native records and refetch the list to restore server ordering. Deletion
  removes the native connection edge and record. A failed create acknowledgement triggers a recovery read, never an
  automatic create replay. Library membership writes run serially, retain per-target outcomes, refresh shelf counts and
  refetch the active filtered Library connection.

### State-writer census

| State item | Where it lives | Writer(s) |
| ---------- | -------------- | --------- |
| `shelves.updated_at` | `shelves` row | The `shelves_set_updated_at` trigger, fired by any `UPDATE` |
| `shelves.is_system` | `shelves` row | The column default (`false`); no handler ever sets it |
| `shelf_items` row existence | `shelf_items` table | `add_shelf_item` (insert), `remove_shelf_item` (delete), `delete_shelf` through the `shelves` cascade, and a manifestation delete through the `manifestations` cascade |

No state item above has competing owners: the handlers named are its only application writers, and a cascade is the
database acting on its own. What a reader needs is which call reaches which write.

`shelves.updated_at` is written by the `shelves_set_updated_at` trigger (`BEFORE UPDATE`, running `set_updated_at()`),
which fires on any `UPDATE` to the row regardless of which column changed. `rename_shelf` relies on the trigger alone:
its own `UPDATE` sets only `name`, and the trigger supplies `updated_at`. `add_shelf_item` and `remove_shelf_item` each
issue an explicit `UPDATE shelves SET updated_at = now() WHERE id = $1` purely to fire the trigger, since those two
handlers would otherwise never touch the `shelves` row at all and `updated_at` would not move with membership.

`shelf_items` row existence changes only through `add_shelf_item`'s
`INSERT … ON CONFLICT (shelf_id, manifestation_id) DO NOTHING` (so a duplicate add is a no-op for membership) and
`remove_shelf_item`'s `DELETE`; no other handler inserts into or deletes from this table directly, though a shelf or
manifestation delete removes its rows through the cascading foreign keys.

`shelves.is_system` deserves a direct statement: the flag, the `AppError::SystemShelfImmutable` (409) it can trigger on
`rename_shelf` and `delete_shelf`, the `ORDER BY is_system DESC` on `list_shelves`, and the client's disabled
rename/delete affordance for a system shelf are all present and exercised by tests that insert a shelf with
`is_system = TRUE` directly by SQL. Nothing in the running application sets that flag: `create_shelf`'s `INSERT` never
sets the column, and no seeding, first-login, or migration step does either. The mechanism is real and tested; the state
it guards against does not arise from any caller-reachable action.

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

Browser readers use the generated shelf operations in `reader.ts` and mutations in `mutations.ts`. Relay owns normalised
records and connection membership. Targeted native updates and refetches publish changed membership, names and counts.
Public HTTP adapters retain their response and ownership contracts.

Graph shelf roots and request-local loaders call the owner-scoped reads in `reader.rs`. `Shelf` has a base64 domain ID
and exposes scalar shelf metadata plus a bounded `items(first:, after:)` connection. GraphQL shelves and items require
`first` from 0 through 100, independently of `REVERIE_OPDS_PAGE_SIZE`. The root `shelves` connection retains
system-first, name, then ID ordering. Items retain addition time, then manifestation ID ordering, with the shelf ID
bound into the graph cursor. Every edge has its own cursor and PageInfo flags come from bounded SQL probes.

Typed and Node lookups return null for foreign, hidden, missing or invalid shelf identities. Item hydration rechecks
manifestation visibility through the caller's transaction. Relay is the browser owner of these records; read execution
adds no shelf writer.

GraphQL create, rename, delete and membership mutations call `services::shelves` with write scope. They preserve adult
CRUD, owner predicates, system-shelf refusals and child membership permissions. Create and rename return the shelf;
deletion and membership changes identify the affected records. These actions introduce no shelf precondition. Browser
workflows use these generated mutations through their existing page and Library action owners.

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

Item paging never changes which `updated_at` is reported: `get_shelf_with_items` reads the shelf identity once per
request regardless of which items page is requested, so a caller walking multiple item pages sees one stable value
across the walk unless a concurrent write lands mid-walk, in which case `api/shelves.ts` keeps the first page's
identity.

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

1. `get_shelf_with_items` reads the shelf row under the `WHERE id = $1 AND user_id = $2` predicate; a missing or foreign
   shelf returns `AppError::NotFound`.
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
  check, never before it.
- **Cursor from another list.** `ShelfCursor::parse` and `ShelfItemCursor::parse` accept only their own tag (`sh`, `si`)
  and refuse a cursor minted by the books list or by the other shelf list with `AppError::Validation` (422), so a cursor
  never positions a walk in a list it was not cut from.
- **A child probing shelf existence via manifestation ids.** Covered in Runtime behaviour: the RLS-scoped probe in
  `add_shelf_item` and the ownership check both resolve to the same `AppError::NotFound`, so a child cannot distinguish
  "no such shelf" from "shelf is yours but that manifestation isn't visible to you".
- **A malformed timestamp in a reader response.** The GraphQL network validates selected shelf timestamps before
  publication. Primary pages surface failure through their route; auxiliary consumers retain degraded feedback and log
  the failure.
- **A failed acknowledgement after a committed create.** The owning page refetches its native shelf list without
  replaying the create. Entered form input and local error feedback remain available.

## Security and operations

Ownership of a `shelves` or `shelf_items` row is enforced entirely by the `WHERE user_id = $current_user` (or the
equivalent join through `shelves`) predicate each handler writes into its own query, because neither table carries a
row-level-security policy. Not every statement repeats the predicate: the shelf `DELETE` and the `updated_at` bump in
the item handlers run against the id alone, and inherit ownership from the ownership-bound `SELECT ... FOR UPDATE` that
precedes them in the same transaction. This is a documented, deliberate divergence from the pattern the Design
"Row-level security and database context" covers for most other per-user tables. There is no database-level fallback if
one of these predicates is dropped: the `reverie_app` role's grant on both tables is unconditional, so a query missing
its ownership clause would compile, run, and return rows across every account. The `add_shelf_item` row-level-security
probe is the one place this subject touches that mechanism, and only to prevent an existence-probing attack against
manifestation visibility, never to scope the shelf itself. This is the ownership axis REV-ADR-0056 assigns to the data
layer, enforced by this subject on every route regardless of the additional role gate three of them apply.

Read access to a shelf and its items requires no more than the operation's declared `read` scope and no role check
beyond ownership: a child account can view its own shelves and items. Adding and removing items likewise carry no
`require_not_child` gate, so a child can organise the membership of shelves it owns. Creating, renaming, and deleting a
shelf are adult-only (`require_not_child`), so a child cannot create a new shelf or remove or rename an existing one (a
shelf's `user_id` is fixed at creation and never reassigned).

Not applicable: this subject exposes no operational surface of its own beyond the endpoints already described — no
service to run, restart, or scale independently of the backend as a whole.
