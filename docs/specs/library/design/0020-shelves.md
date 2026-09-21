---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0020"
title: "Shelves"
satisfies:
  - "REV-REQ-0020"
  - "REV-REQ-0057"
  - "REV-REQ-0059"
governed-by:
  - "REV-ADR-0028"
  - "REV-ADR-0011"
---

# Shelves

Shelves are the reader's own curation buckets: named, ordered lists of manifestations a caller builds by hand. This
Design covers the `/api/v1/shelves*` CRUD and reorder surface, the `shelves` and `shelf_items` tables it owns, the
handler-level ownership predicate that stands in for row-level security on those two tables, and the client pages and
consumer surfaces that read and write through it.

## Purpose and boundaries

This subject owns shelf create, read, update and delete; the `is_system` flag and the immutability it imposes on a
system-managed shelf; shelf items and their `position` ordering; the reorder endpoint
(`PUT /api/v1/shelves/{id}/items`), including its full-set validation and its `If-Match` precondition against the
shelf's `updated_at`; the handler-level `WHERE user_id = caller` ownership predicate that every mutating and
shelf-scoped read handler applies in place of row-level security; the row-level-security-scoped manifestation-visibility
probe `add_shelf_item` runs before it will add an item; and the client's two shelf pages
(`frontend/src/pages/shelves/ShelvesListPage.tsx`, `frontend/src/pages/shelves/ShelfDetailPage.tsx`) and the
`frontend/src/api/shelves.ts` client that fronts them.

It does not own row-level security itself: the `app.current_user_id` mechanism, the pools, and the policy census belong
to the Design "Row-level security and database context". This subject is one of several call sites where
`crate::db::acquire_with_rls` is used for something other than scoping the resource the transaction itself queries —
like the enrichment dry-run's own visibility check (`backend/src/routes/enrichment.rs`) — `add_shelf_item`'s probe uses
it only to answer "can this caller see this manifestation", never to scope `shelves` or `shelf_items`, which carry no
row-level-security policy at all. It does not own the three-axis scope, role, and ownership model or the cross-resource
map of which tables use row-level security and which use a handler predicate; that narrative belongs to the Design
"Authorization axes", which names `shelves`/`shelf_items` as the ownership-predicate side of that map and cites this
subject's code as its evidence. It does not own strong-entity-tag or `If-Match` mechanics: the Design "Conditional
requests and optimistic concurrency" owns the shared grammar and comparison, the precondition-evaluation order every
protected endpoint follows, and this module's own `etag_header`/`parse_if_match` helpers, naming them as its one
documented exception to the shared mechanism. This subject owns only shelf CRUD and the handler-level ownership
predicate; Structure and Failure and recovery below state only where its own routes plug into that contract. It does not
own the keyset cursor framework or the `build_next_url`/`split_page` pagination helpers it borrows from
`backend/src/routes/library/mod.rs`, owned by the Design "Books list query contract", or that Design's library filter
grammar `shelf` condition, which lets a books-list query scope to one shelf's membership and is referenced only as a
dependent below.

Depends on: `CurrentUser` (`backend/src/auth/middleware.rs`) for identity and its `require_scope`/`require_not_child`
assertions; `crate::db::acquire_with_rls` for the `add_shelf_item` visibility probe only; `ShelfCursor` and
`ShelfItemCursor` plus `build_next_url` and `split_page` (`backend/src/routes/cursor.rs`,
`backend/src/routes/library/mod.rs`) for keyset pagination; the `manifestations` table, read-only, for the visibility
probe and the item foreign key.

Depended on by: the OPDS shelf-scoped feed routes (`backend/src/routes/opds/shelves.rs` and neighbours), which read the
same two tables directly by SQL rather than through this module's Rust functions, and are described by the OPDS
catalogue subject, not here — the OPDS EPUB download route is not shelf-scoped and never queries `shelves` or
`shelf_items` itself, so it is not a dependent of this subject; the Design "Books list query contract"'s `shelf` filter
condition, which references a shelf id the caller must own; and, on the client, the left rail's shelf list, the library
filter rail's shelf facet, the filter chips row, the book detail drawer's "add to shelf" picker, and the library batch
bar's "add selection to shelf" action — all described below only as consumers, since none of them owns shelf state.

## Structure

- `backend/src/routes/shelves/mod.rs` holds all eight handlers and is the only place any of them lives: `list_shelves`,
  `create_shelf`, `rename_shelf`, `delete_shelf`, `get_shelf_with_items`, `add_shelf_item`, `remove_shelf_item`, and
  `reorder_shelf_items`. Its module doc states the boundary this Design names in Purpose: no row-level security on
  `shelves` or `shelf_items`, ownership enforced by an explicit `WHERE user_id = $current_user` predicate in every
  handler that touches an existing row, and a mismatched predicate resolving to `AppError::NotFound` so a foreign
  shelf's existence is never distinguishable from its absence.
- `backend/src/models/shelf.rs` holds the two response types, `Shelf` and `ShelfItem`. Both are `#[non_exhaustive]` and
  carry no pagination fields; the paged response envelopes (`ShelfListResponse`, `ShelfDetailResponse`) are declared in
  `routes/shelves/mod.rs` itself, alongside the request bodies, because pagination is a wire concern local to the
  handler, not part of the model.
- `backend/src/routes/cursor.rs` declares `ShelfCursor` and `ShelfItemCursor` alongside the books list engine's
  `SortCursor`, in the same module and the same opaque, tagged wire format (`sh|…` and `si|…`, base64url-unpadded).
  `ShelfCursor` carries `(is_system, name, id)`, matching `list_shelves`'s `ORDER BY is_system DESC, name ASC, id ASC`;
  because that sort is mixed-direction, the keyset predicate the handler builds is a two-arm `OR`
  (`is_system < $1 OR (is_system = $1 AND (name, id) > ($2, $3))`), not a single row-tuple comparison. `ShelfItemCursor`
  carries `(position, added_at, manifestation_id)`, matching the items query's all-ascending sort; the
  `manifestation_id` tiebreaker is load-bearing because neither `position` nor `added_at` is unique per shelf.
- `routes/shelves/mod.rs` defines its own `etag_header` and `parse_if_match` rather than importing the shared
  `crate::routes::etag` module that `backend/src/routes/reading.rs` (the Design "Reading state") uses. The Design
  "Conditional requests and optimistic concurrency" owns both mechanisms, names this module's pair as its one documented
  exception to the shared module, and states where the two diverge.
- On the client, `frontend/src/api/shelves.ts` is the sole HTTP boundary for this surface: it defines the Zod schemas
  for both wire shapes, walks every cursor-paginated response to completion internally so every caller receives a fully
  assembled array or object, and derives the `If-Match` value from the response body's `updated_at` rather than reading
  a response header. `frontend/src/pages/shelves/ShelvesListPage.tsx` is the `/shelves` list-and-manage page (create,
  rename, delete); `frontend/src/pages/shelves/ShelfDetailPage.tsx` is the `/shelves/:id` page, which adds
  `@dnd-kit/sortable` drag-to-reorder over the same data. `frontend/src/routes/shelves.tsx` and
  `frontend/src/routes/shelf-detail.tsx` are their route-loader pairings, each loading its page's query and swallowing
  the prefetch's own failure, leaving the page's `useSuspenseQuery` as the one error-surfacing point.
- Five further client surfaces read the shelf list or a shelf's items as an auxiliary concern, degrading in place rather
  than throwing to a route boundary on failure: `components/shell/LeftRail.tsx` (the shelves nav entry's nested list),
  `components/shell/FilterRail.tsx` (the shelf filter facet), `pages/library/FilterChips.tsx` (rendering a shelf id
  filter chip's display name), `pages/library/BookDetailDrawer.tsx` (the "add to shelf" picker), and
  `pages/library/BatchBar.tsx` (the "add selection to shelf" action, which calls `addShelfItem` once per selected
  manifestation in a serial loop). None of these five owns shelf state; each is a `useQuery` reader (not
  `useSuspenseQuery`) that logs its own failure, the tier the `api/shelves.ts` module doc assigns it.

### State-writer census

| State item | Where it lives | Writer(s) |
| ---------- | -------------- | --------- |
| `shelves.updated_at` | `shelves` row | The `shelves_set_updated_at` trigger, fired by any `UPDATE` |
| `shelf_items.position` | `shelf_items` row | `add_shelf_item` (append) and `reorder_shelf_items` (full rewrite) |
| `shelves.is_system` | `shelves` row | The column default (`false`); no handler ever sets it |
| `shelf_items` row existence | `shelf_items` table | `add_shelf_item` (insert), `remove_shelf_item` (delete), `delete_shelf` through the `shelves` cascade, and a manifestation delete through the `manifestations` cascade |

Every state item above has exactly one write path even where several handlers can trigger it, so ownership here is not
in question; what a reader needs is which call reaches which write.

`shelves.updated_at` is written by the `shelves_set_updated_at` trigger (`BEFORE UPDATE`, running `set_updated_at()`),
which fires on any `UPDATE` to the row regardless of which column changed. `rename_shelf` relies on the trigger alone:
its own `UPDATE` sets only `name`, and the trigger supplies `updated_at`. `add_shelf_item`, `remove_shelf_item`, and
`reorder_shelf_items` each issue an explicit `UPDATE shelves SET updated_at = now() WHERE id = $1` purely to fire the
trigger, since those three handlers would otherwise never touch the `shelves` row at all.

`shelf_items.position` is written by `add_shelf_item` (`COALESCE(MAX(position), -1) + 1`, appending past the current
maximum) and by `reorder_shelf_items` (a full `UNNEST`-driven rewrite to `0..len-1` for every row on the shelf, in one
statement). `remove_shelf_item` never renumbers the rows it leaves behind, so a shelf's positions can carry gaps after a
removal without affecting read order, which sorts by `position ASC, added_at ASC, manifestation_id ASC` rather than
relying on contiguous values.

`shelf_items` row existence changes only through `add_shelf_item`'s
`INSERT … ON CONFLICT (shelf_id, manifestation_id) DO NOTHING` (so a duplicate add is a no-op for membership) and
`remove_shelf_item`'s `DELETE`; no other handler issues a statement against this table, though a shelf or manifestation
delete removes its rows through the cascading foreign keys.

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
  (`position ASC, added_at ASC, manifestation_id ASC`), `read` scope, available to a child.
- `POST /api/v1/shelves/{id}/items` — append a manifestation, `write` scope, available to a child.
- `DELETE /api/v1/shelves/{id}/items/{manifestation_id}` — remove an item, `write` scope, available to a child.
- `PUT /api/v1/shelves/{id}/items` — full-set reorder, `write` scope, available to a child, requiring `If-Match`.

Every operation's `security(...)` annotation lists the same scope across all four credential transports
(`session_cookie`, `device_token_bearer`, `oidc_jwt_bearer`, `opds_basic`); the Design "Authorization axes" covers why
that array is one list rather than four, and the deny-by-default grid that proves each declared scope has a working
gate. The two read-scope operations (`list_shelves`, `get_shelf_with_items`) call no `require_scope` themselves:
`CurrentUser`'s extraction already refuses a scopeless credential, so any resolved caller already holds at least `read`.
`create_shelf`, `rename_shelf` and `delete_shelf` additionally call `require_not_child`; `add_shelf_item`,
`remove_shelf_item` and `reorder_shelf_items` do not, so a child account can add, remove, and reorder items on its own
shelves and read them, but cannot create, rename, or delete a shelf.

`ApiJson` and `ApiPath` (`backend/src/extract.rs`) are the request-body and path-parameter extractors every mutating
handler uses; their rejection behaviour is the API error contract's concern, referenced here only as the extractors in
use.

On the client, `frontend/src/api/shelves.ts` is the only module that calls these eight endpoints; every consumer surface
listed in Structure imports from it rather than calling `apiFetch` directly. `queryKeys.shelves.list()` and
`queryKeys.shelves.detail(id)` (`frontend/src/lib/query/keys.ts`) are the two query keys every reader uses. Most
mutations invalidate the coarser `queryKeys.shelves.all` instead — every create, rename and delete on `ShelvesListPage`,
plus `BookDetailDrawer` and `BatchBar`'s own shelf writes — and React Query's prefix matching still refetches every
reader from that broader key; only the reorder mutation's `onSettled` on `ShelfDetailPage` invalidates `.detail(id)`
directly.

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

Every `ETag` this subject emits is the shelf's `updated_at`. The Design "Conditional requests and optimistic
concurrency" owns the tag's construction, its comparison, and the client round trip it depends on. Item paging never
changes which `updated_at` is reported: `get_shelf_with_items` reads the shelf identity once per request regardless of
which items page is requested, so a caller walking multiple item pages sees one stable entity-tag across the walk unless
a concurrent write lands mid-walk (see Failure and recovery for what a client does about that).

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
   `Shelf` response with an `ETag` header carrying the trigger-set `updated_at`.

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
   `SELECT id FROM shelves WHERE id = $1 AND user_id = $2 FOR UPDATE`, compute the next position, insert the item
   (`ON CONFLICT (shelf_id, manifestation_id) DO NOTHING`), bump `updated_at` explicitly, and commit.
5. A repeat call with the same manifestation id is a no-op for `shelf_items` (the `ON CONFLICT` clause), but step 4's
   `updated_at` bump still runs unconditionally, so the shelf's `ETag` changes on a duplicate add even though its
   membership does not.

**Two concurrent reorders against the same shelf**, exercised by `parallel_reorders_with_same_if_match_serialize`:

1. Both requests carry the same `If-Match` value, captured from one prior `GET`.
2. Both call `reorder_shelf_items`, each opening its own transaction and running
   `SELECT updated_at FROM shelves WHERE id = $1 AND user_id = $2 FOR UPDATE`. Postgres serialises the two `FOR UPDATE`
   locks: the first transaction to acquire the lock proceeds; the second waits for the first to commit or roll back.
3. The first transaction's `updated_at` still matches the shared `If-Match` value, so it passes the precondition, checks
   that the posted item list is the same length as the shelf's current items and that every posted id is already on the
   shelf, rewrites every `shelf_items.position` in one `UPDATE … FROM unnest(...)` statement, bumps `updated_at`, and
   commits.
4. The second transaction, now unblocked, re-reads `updated_at` under its own lock and finds the value the first
   transaction just wrote — no longer equal to the `If-Match` it carries — so it returns `AppError::IfMatchMismatch`
   (412) without writing anything.
5. Exactly one of the two requests succeeds; the test asserts the pair of outcomes is always `[204, 412]` in some order,
   never two successes and never two failures.

**Client-side drag reorder**, `ShelfDetailPage.tsx`'s `reorder` mutation:

1. `onMutate` cancels any in-flight query for the shelf's key, snapshots the current cache value, and writes the
   dragged-to order into the cache immediately (optimistic), returning the snapshot as mutation context.
2. The mutation calls `reorderShelfItems`, which sends the full ordered id list and the `If-Match` header built from
   `data.updated_at` as it stood before the drag.
3. On success, `onSettled` invalidates the query key, triggering a refetch that replaces the optimistic value with the
   server's own.
4. On a `412` specifically, `onError` restores the pre-drag snapshot and shows a toast asking the operator to refresh;
   any other failure restores the snapshot and shows the response's detail text. `onSettled` still runs afterward,
   reloading in both the success and the failure case, so the cache never carries the optimistic value for longer than
   one round trip.

## Failure and recovery

- **Foreign or missing shelf.** Every handler that targets an existing shelf by id resolves a missing row and a row
  owned by a different `user_id` to the same `AppError::NotFound` (404). No handler in this module returns `403` for an
  ownership mismatch.
- **System-shelf mutation.** `rename_shelf` and `delete_shelf` return `AppError::SystemShelfImmutable` (409) once
  ownership has already been confirmed; a caller learns "this shelf is mine but immutable" only after the ownership
  check, never before it.
- **Reorder precondition.** A missing `If-Match` returns `AppError::IfMatchRequired` (428); a value that parses but does
  not match the locked row's `updated_at` returns `AppError::IfMatchMismatch` (412). The Design "Conditional requests
  and optimistic concurrency" owns the shared precondition contract and states why this response, unlike the shared
  module's own 412, carries no `ETag` header of its own.
- **Malformed `If-Match` on the reorder endpoint.** `routes::shelves::parse_if_match` rejects a weak validator, an
  unquoted value, or an RFC 3339 timestamp that does not parse, with `AppError::Validation` (422). The Design
  "Conditional requests and optimistic concurrency" owns this parser, its divergence from the shared module's `400`, and
  the set of malformed forms each one checks for.
- **Partial, foreign or repeated reorder set.** A posted item list whose length does not match the shelf's current item
  count, that names an id not on the shelf, or that names one id more than once, returns `AppError::Validation` (422)
  before any `UPDATE` runs; the length, membership and repetition checks happen inside the same transaction as the
  `FOR UPDATE` lock, so nothing is rewritten if any check fails. Together the three checks prove the posted list is a
  permutation of the shelf's current items.
- **Cursor from another list.** `ShelfCursor::parse` and `ShelfItemCursor::parse` accept only their own tag (`sh`, `si`)
  and refuse a cursor minted by the books list or by the other shelf list with `AppError::Validation` (422), so a cursor
  never positions a walk in a list it was not cut from.
- **A child probing shelf existence via manifestation ids.** Covered in Runtime behaviour: the RLS-scoped probe in
  `add_shelf_item` and the ownership check both resolve to the same `AppError::NotFound`, so a child cannot distinguish
  "no such shelf" from "shelf is yours but that manifestation isn't visible to you".
- **A read hitting a malformed timestamp on the client.** `api/shelves.ts`'s Zod schemas validate `created_at`,
  `updated_at`, and `added_at` as bare `z.iso.datetime()`. A response that fails this check throws before any consumer
  sees a partially-typed value; `ShelvesListPage` and `ShelfDetailPage` let that throw reach their route's error
  boundary (`useSuspenseQuery`), while the five auxiliary consumers named in Structure catch it via `useQuery`'s own
  error state and degrade in place, logging the failure themselves since only a `401` is centrally routed.
- **A write that the client's own schema then rejects.** `createShelf` and `renameShelf` parse the response body after
  the server has already committed the write; a schema rejection there reports failure for an operation that already
  succeeded; the caller's own `onSuccess` never runs, so the shelf list is not invalidated and a retried create can add
  a second shelf under the same name, since nothing enforces name uniqueness at any layer. `api/shelves.ts`'s module doc
  names this as a deliberate non-goal: recovery is a manual refetch, not an automatic retry.

## Security and operations

Ownership of a `shelves` or `shelf_items` row is enforced entirely by the `WHERE user_id = $current_user` (or the
equivalent join through `shelves`) predicate each handler writes into its own query, because neither table carries a
row-level-security policy. Not every statement repeats the predicate: the shelf `DELETE`, the reorder `UPDATE` and the
`updated_at` bump in the item handlers run against the id alone, and inherit ownership from the ownership-bound
`SELECT ... FOR UPDATE` that precedes them in the same transaction. This is a documented, deliberate divergence from the
pattern the Design "Row-level security and database context" covers for most other per-user tables. There is no
database-level fallback if one of these predicates is dropped: the `reverie_app` role's grant on both tables is
unconditional, so a query missing its ownership clause would compile, run, and return rows across every account. The
`add_shelf_item` row-level-security probe is the one place this subject touches that mechanism, and only to prevent an
existence-probing attack against manifestation visibility, never to scope the shelf itself. This is the ownership axis
REV-ADR-0028 assigns to the data layer, enforced by this subject on every route regardless of the additional role gate
three of them apply.

Read access to a shelf and its items requires no more than the operation's declared `read` scope and no role check
beyond ownership: a child account can view its own shelves and items. Adding, removing, and reordering items likewise
carry no `require_not_child` gate, so a child can organise the membership and order of shelves it owns. Creating,
renaming, and deleting a shelf are adult-only (`require_not_child`), so a child cannot create a new shelf or remove or
rename an existing one, including one it did not create itself but that was created for it (a shelf's `user_id` is fixed
at creation and never reassigned).

Not applicable: this subject exposes no operational surface of its own beyond the endpoints already described — no
service to run, restart, or scale independently of the backend as a whole.
