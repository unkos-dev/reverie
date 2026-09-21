---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0021"
title: "Reading state"
satisfies:
  - "REV-REQ-0018"
  - "REV-REQ-0020"
governed-by:
  - "REV-ADR-0028"
  - "REV-ADR-0011"
---

# Reading state

This Design covers each reader's personal relationship to one book: the `reading_state` table's per-user,
per-manifestation row, the `GET`/`PATCH /api/v1/books/{id}/reading` endpoints, the merge-patch semantics and
status-transition stamps that derive `progress_pct`, `started_at`, `finished_at` and `last_read_at` from a patched
`status`, this subject's own use of the seed-then-lock and precondition-ordering contract to keep two concurrent first
writes from clobbering each other, and the client controls a reader uses to set status and rating from the library
table.

## Purpose and boundaries

This subject owns the `reading_state` table and the specific row-level-security policy scoping it
(`reading_state_owner`); the `get_reading` and `patch_reading` handlers in `backend/src/routes/reading.rs`; the
`ReadingState` and `ReadingStateSummary` wire types in `backend/src/models/reading_state.rs` and the `ReadingStatus`
enum in `backend/src/models/reading_status.rs`; the merge-patch and status-transition-stamp rules `apply_patch` applies;
the seed-then-lock sequence that protects a first write; and the client's reading-state API client and the two grid
editors that write it.

It does not own the row-level-security mechanism itself: the `app.current_user_id` setting and `acquire_with_rls` are
the Design "Row-level security and database context", which this subject relies on for every read and write. It does not
own entity-tag mechanics: the strong entity-tag grammar, `hash_etag`, the `428`/`412` contract, and the client's ETag
capture-and-replay belong to the Design "Conditional requests and optimistic concurrency", which already documents
`patch_reading`'s own seed-then-lock sequence and the relative order in which it evaluates scope, the header, existence,
the precondition, and body validation. This subject supplies only its own hash input (`ReadingEtagFields`) and the
handler code that sequence runs against; it does not restate that Design's general contract. It does not own the scope
and role authorisation model that `require_scope` enforces; that is the Design "Authorization axes". It does not own the
`ApiPath`/`ApiJson` request extractors or the `AppError`-to-Problem-Details mapping; that is the Design "API error
contract and OpenAPI". It does not own the generic library-table cell-edit orchestration (pending-cell state, the
bounded undo stack, the stale-tag conflict toast) in `frontend/src/pages/library/table/useCellEdit.ts` and
`edit-routing.ts`, which applies identically to the metadata pipeline and is the Library table cell editing and undo
subject; this subject owns only the two editor controls that orchestration routes commits through.

Depends on: `crate::db::acquire_with_rls` for the RLS-scoped transaction every query in this subject runs inside; the
`manifestations` table's own row-level-security policies, which `ensure_manifestation_visible` relies on for its
existence probe; `CurrentUser::require_scope` for the `read`/`write` gate; the shared entity-tag grammar and comparison
in `backend/src/routes/etag.rs`; `crate::extract::{ApiPath, ApiJson}` for request decoding.

Depended on by: `GET /api/v1/books` (`backend/src/routes/library/mod.rs::load_reading_state_for_manifestations`), which
batch-loads a `ReadingStateSummary` onto every list row under the caller's own RLS scope — the Design "Books list query
contract"; the library table's status and rating columns and the cell-edit orchestration in the Library table cell
editing and undo subject, which route a commit to this subject's `PATCH` endpoint and rely on its entity-tag for
conflict detection.

## Structure

- `backend/src/routes/reading.rs` is the whole server-side endpoint surface. `router()` builds the two-route
  `OpenApiRouter` merged into `crate::openapi::pilot_router`; `get_reading` and `patch_reading` are its only handlers,
  and the module is tagged `reading` in the generated OpenAPI document. `ensure_manifestation_visible` is the RLS-scoped
  existence probe both handlers call first, returning `AppError::NotFound` when the manifestation is missing or hidden
  by the caller's own visibility policy. `ReadingStateRow` is the shared decode target for both a `SELECT` and the
  `UPDATE`'s `RETURNING` clause; its `Default` gives the all-null "unread" shape a query with no row still needs to hash
  and return. `ReadingEtagFields` and `ReadingStateRow::etag` compute the entity-tag from a fixed-field-order
  serialisation covering every field the `PATCH` can modify. `apply_patch` is a pure function, separated from the
  handler's I/O, that merges an `UpdateReadingRequest` onto a locked row and applies the status-transition stamp rules
  Runtime behaviour describes.
- `backend/src/models/reading_state.rs` carries two wire types and nothing else — queries live in `routes::reading` and
  `routes::library`, per the module's own doc comment. `ReadingState` is the full row the single-book endpoints return.
  `ReadingStateSummary` is a narrower projection batch-loaded onto each `GET /api/v1/books` row, deliberately excluding
  `notes`: at the maximum page size its 10,000-character cap would dominate the response body many times over, so
  clients read notes only through the single-book `GET`.
- `backend/src/models/reading_status.rs` declares `ReadingStatus`, the five-variant enum (`want_to_read`, `reading`,
  `on_hold`, `finished`, `abandoned`) that maps to the Postgres `reading_status` type and to the same snake_case wire
  strings on both sides.
- `backend/migrations/20260810000000_initial_schema.up.sql` creates `reading_state` — composite primary key
  `(user_id, manifestation_id)`, both foreign keys `ON DELETE CASCADE` — its `CHECK` constraints, the
  `trg_reading_state_updated_at` trigger, and the `reading_state_owner` row-level-security policy.
- `backend/src/routes/library/mod.rs::load_reading_state_for_manifestations` batch-loads `ReadingStateSummary` for a
  page of manifestation ids inside the same RLS-scoped transaction the list query already runs in, with no extra
  `WHERE user_id = …` predicate of its own: the policy alone confines it. `backend/src/routes/library/filters.rs`'s
  `push_status_match` and `push_rating_predicates` also query `reading_state`, via `EXISTS`/`NOT EXISTS` sub-queries
  backing the `status_any`/`status_none`/`rating_gte`/`rating_lte`/`rating_empty` parameters on the same endpoint. Both
  belong to the Design "Books list query contract", not to this subject; they are named here as `reading_state`'s other
  production readers.
- `frontend/src/api/reading.ts` is the client's wire boundary: `ReadingStateSchema`, `getReadingState` and
  `updateReadingState`, each documented against the endpoint it wraps.
- `frontend/src/pages/library/table/editors/StatusCellEditor.tsx` and `RatingCellEditor.tsx` are the two reading
  controls a reader interacts with: a native `<select>` that commits immediately on a chosen option, and a keyboard- and
  pointer-operable star group that commits on a digit key, a star click, or Enter after staging a draft with the arrow
  keys. These two components are the whole of this subject's client-side UI surface — everything downstream of a commit
  (building the patch, patching the list cache, undo, conflict recovery) is the library table's generic cell-edit
  orchestration named in Purpose and boundaries. `frontend/src/api/reading.ts` above is this subject's wire boundary,
  not a UI control.

## Interfaces and dependencies

- `GET /api/v1/books/{id}/reading` and `PATCH /api/v1/books/{id}/reading` (`backend/src/routes/reading.rs`), each
  requiring at least `read` or `write` scope respectively over any of the four credential transports the Design
  "Authorization axes" covers.
- `crate::db::acquire_with_rls(&state.pool, current_user.user_id)` opens the transaction every query in this subject
  runs inside; its settings contract and the `reading_state_owner` policy's predicate belong to the Design "Row-level
  security and database context".
- The shared entity-tag mechanics — `hash_etag`, `StrongEntityTag`, `parse_if_match`, `if_match_mismatch`
  (`backend/src/routes/etag.rs`) and their client counterpart (`frontend/src/api/etags.ts`, the capture-and-replay in
  `frontend/src/api/fetch.ts`) — belong to the Design "Conditional requests and optimistic concurrency". This subject
  supplies only `ReadingEtagFields` as the hash input and consumes the shared grammar and comparison.
- `GET /api/v1/books` (`backend/src/routes/library/mod.rs`) embeds a `ReadingStateSummary` on every row via
  `load_reading_state_for_manifestations`; that endpoint's own filter, sort and cursor contract belongs to the Design
  "Books list query contract".
- `frontend/src/api/books.ts` mirrors the wire types a second time for the list-embedded shape — `ReadingStatusSchema`
  and `ReadingStateSummarySchema`, validated as part of `BookListItemSchema`.

## Data and state

- **The `reading_state` row.** One per `(user_id, manifestation_id)`, created only by a `PATCH` — never by `GET`, which
  returns the all-null default when no row exists. That all-null shape is the "unread" domain state itself, not an
  error, per `ReadingState`'s own doc comment. Columns: `status` (nullable `reading_status`), `rating` (nullable
  `smallint`, `CHECK` 1–5), `notes` (nullable `text`, `CHECK` ≤10,000 characters), `progress_pct` (nullable `real`,
  `CHECK` 0–100), `started_at`/`finished_at`/`last_read_at` (nullable timestamps), plus `created_at` and a
  trigger-maintained `updated_at` — two bookkeeping columns the API never exposes and the entity-tag hash never covers.
  A `CHECK` constraint (`reading_state_progress_paired_with_timestamp`) requires `progress_pct` and `last_read_at` to be
  null or non-null together; the only application code path that sets either sets both in the same statement.
- **Lifetime.** Created by the first successful `PATCH`, updated by every later one, removed only by the cascading
  foreign keys when the owning user or the manifestation is deleted. No handler in this subject issues a `DELETE`,
  though `reverie_app` holds the grant (owned by the Design "Row-level security and database context").
- **Ownership.** Enforced by the `reading_state_owner` policy (`user_id = app.current_user_id` in both `USING` and
  `WITH CHECK`) and never by role — see Security and operations for how that enforcement splits between the lock and the
  writes. Neither handler in `backend/src/routes/reading.rs` calls `require_not_child` or reads `role`/`is_child`; the
  module's own doc comment states this is deliberate: reading state is self-scoped personal data a child account manages
  for itself, not shared-library curation.
- **`notes`.** Accepts up to 10,000 characters on the wire and in the client's `UpdateReadingFields` type, but no
  editing surface in the client writes it; the only client mutation paths are the status and rating editors.
- **`progress_pct`.** Has no direct client write path: the only non-null value the API can ever produce is `100.0`,
  stamped when a patch sets `status` to `finished`. No code path in `apply_patch` sets it to any other non-null value,
  and none clears a set value back to `null`.

## Runtime behaviour

**A caller reading their state for one book**, `GET /api/v1/books/{id}/reading`:

1. `require_scope(Scope::Read)` — the floor scope, so this rejects only a scopeless credential, which authentication
   refuses earlier regardless.
2. `db::acquire_with_rls` opens the transaction and sets `app.current_user_id`.
3. `ensure_manifestation_visible` probes `manifestations` under the caller's own RLS visibility. A miss returns
   `AppError::NotFound` before any `reading_state` query runs.
4. A `SELECT` returns `Option<ReadingStateRow>`; `unwrap_or_default()` yields the all-null shape when no row exists.
5. The transaction commits; the response carries the computed entity-tag as `ETag` and the row as JSON.

**A caller updating their reading state**, `PATCH /api/v1/books/{id}/reading`:

1. `require_scope(Scope::Write)`.
2. `parse_if_match` runs against the request headers; an absent header returns `AppError::IfMatchRequired` (`428`)
   before any database work.
3. `db::acquire_with_rls` opens the transaction.
4. `ensure_manifestation_visible` runs, with the same not-leaked `404` contract as `GET`.
5. A seed `INSERT … ON CONFLICT (user_id, manifestation_id) DO NOTHING` gives `FOR UPDATE` a row to lock even on a first
   write, without touching an existing row's values.
6. `SELECT … FOR UPDATE` locks the now-guaranteed-to-exist row. The query carries no `user_id` predicate of its own
   (`WHERE manifestation_id = $1` alone), so it is `reading_state_owner`'s `USING` clause, not the query text, that
   confines the lock to the caller's own row; this is also why two different callers patching the same manifestation
   never contend with each other, since each locks a different primary key. This is the point at which two concurrent
   writers *for the same caller* on the same row serialise.
7. `existing.etag()` is computed from the locked row and compared against the parsed `If-Match`. A mismatch returns
   `if_match_mismatch` (`412`, carrying the current tag) immediately and drops the transaction, rolling back the seed
   insert if this transaction performed it.
8. Only once the precondition holds does body validation run: an empty patch, a rating outside `1..=5`, or notes over
   10,000 characters each return `AppError::Validation` (`422`).
9. `apply_patch` merges the request onto the locked row and applies the transition stamps below.
10. `UPDATE … RETURNING` writes the merged row and returns it in one statement; the transaction commits and the response
    carries the new entity-tag.

Steps 2, 5, 6 and 7 are this subject's own use of the seed-then-lock sequence and the precondition-evaluation order the
Design "Conditional requests and optimistic concurrency" specifies for every protected endpoint; that Design documents
the sequence and its relative ordering as a general contract, and this section states only how `patch_reading` carries
it out.

**Status-transition stamps**, applied inside `apply_patch` from the patch's own `status` field (never from a
before/after diff, so repeating the current status re-applies its stamp):

- A patch naming `status: "finished"` always stamps `progress_pct = 100`, `finished_at = now()` and
  `last_read_at = now()`, whatever they held before and whatever the row's previous status was.
- A patch naming `status: "reading"` stamps `started_at` only when it holds no value
  (`existing.started_at.unwrap_or_else(Utc::now)`); re-entering `reading` a second time leaves the original timestamp.
- Any other patched status (`want_to_read`, `on_hold`, `abandoned`), an explicit `null` clearing `status`, or a patch
  that omits `status` entirely, leaves all four fields — `progress_pct`, `started_at`, `finished_at`, `last_read_at` —
  exactly as they were. Clearing `status` after finishing a book does not erase the record that it was finished.

**Two concurrent first writes to the same book**, the scenario the seed-then-lock sequence exists for. The walkthrough
below assumes the connection's default `READ COMMITTED` isolation: `acquire_with_rls` sets no isolation level of its
own, so each transaction runs under whatever `default_transaction_isolation` the connecting role carries, which is
PostgreSQL's own default unless an operator has raised it.

1. A reader's client holds only the "unread" default entity-tag, from a `GET` taken before either write, because no row
   exists yet.
2. Two `PATCH` requests both carrying that tag as `If-Match` reach the handler concurrently, each in its own
   transaction.
3. Both execute the seed `INSERT`; `ON CONFLICT DO NOTHING` means exactly one of them actually inserts the row, but both
   statements succeed either way.
4. Both then run `SELECT … FOR UPDATE` against the same row. PostgreSQL grants the lock to one transaction (A) and
   blocks the other (B) until A commits or rolls back.
5. A's `existing.etag()` equals the "unread" default, since nothing has changed the row yet; the comparison against A's
   `If-Match` succeeds. A validates its body, merges, updates, and commits.
6. Under `READ COMMITTED`, B's `SELECT … FOR UPDATE` unblocks once A commits and re-fetches the row as A left it, not
   the empty default B expected. B's `existing.etag()` is therefore A's post-write tag, which does not match the
   "unread" tag B still holds as `If-Match`. B's request fails with `412`, carrying A's current tag, and B's transaction
   rolls back, including its own no-op seed insert. (Under `REPEATABLE READ` or `SERIALIZABLE`, B cannot see a row
   version newer than its own snapshot after being blocked on it, so PostgreSQL instead raises a serialization failure
   once A commits, which this subject maps to `AppError::Internal` — a `500` in place of the clean `412` above.)
7. The caller that received the `412` retries with the tag the response carried, this time against a body that reflects
   the row as it now stands; the retry succeeds, and both writers' fields end up persisted, because each merge patch
   only touches the keys its own request named.

`patch_reading_concurrent_first_writes_both_survive` exercises exactly this: a `rating` patch and a `notes` patch, sent
together with the same starting tag, resolve to one `200` and one `412`, and a `GET` after the loser retries shows both
fields set. Without the seed step, `FOR UPDATE` would have nothing to lock on an absent row, and two first writes could
both read the same empty default and race to `INSERT` instead of being serialised against one row.

**A child account reading or writing its own state** follows the same steps as an adult's, with one difference at the
visibility probe:

1. Both handlers acquire the same RLS-scoped transaction an adult or admin caller would.
2. `ensure_manifestation_visible`'s probe runs against the caller's own visibility: for a child, only a manifestation on
   one of the child's own shelves is admitted, so a book the child cannot see `404`s exactly as a nonexistent one would
   (the shelf-gated visibility policy itself belongs to the Design "Row-level security and database context").
3. Once the manifestation is visible, every later step — seed, lock, precondition, merge, write — runs identically to an
   adult caller's: `reading_state_owner` scopes by `user_id` alone, and no policy or handler check in this subject reads
   `role` or `is_child`.
4. A second account, adult or child, sees none of the first account's row: the same policy that admits a caller's own
   row excludes every other caller's, verified for two adult accounts by `cross_user_reading_state_is_isolated`.

## Failure and recovery

- **A missing or RLS-hidden manifestation.** `AppError::NotFound` (`404`), from the same `ensure_manifestation_visible`
  probe on both verbs, existence-not-leaked.
- **A `PATCH` with no `If-Match` header.** `AppError::IfMatchRequired` (`428`), before any database work runs.
- **An `If-Match` value the shared entity-tag grammar refuses** (unquoted, the `*` wildcard, an entity-tag list, a weak
  `W/"..."` tag, more than one header instance): `AppError::MalformedHeader` (`400`), from the parser this subject
  shares with the metadata endpoint and does not own.
- **A stale `If-Match`.** `AppError::IfMatchMismatch` (`412`), carrying the row's current tag so the caller can
  re-synchronise in one round trip, evaluated before the request body: a stale tag and an invalid body on the same
  request both resolve to `412`, not `422`, because the precondition is checked first.
- **An empty patch, an out-of-range rating, or over-length notes.** `AppError::Validation` (`422`), reached only once
  `If-Match` has matched.
- **Two concurrent first writes to the same book.** Resolved by the seed-then-lock sequence in Runtime behaviour, never
  a silent lost update: under the default `READ COMMITTED` isolation the loser gets a clean `412` to retry against;
  under a stricter isolation level it gets a serialization-failure `500` instead, covered there.
- **A value that would decode to a timestamp outside the representable range** (`'infinity'`, `'-infinity'`). Rejected
  by a `CHECK` constraint on each timestamp column (`reading_state_started_at_ts_decode_range` and its siblings) at the
  database layer. No field this subject's `PATCH` accepts can produce such a value — every timestamp it writes comes
  from `Utc::now()` — so this constraint guards a write path outside this subject's own handlers, not the API.

## Security and operations

Ownership is the only axis this subject's own `reading_state` policy enforces beyond scope, and it is enforced
differently by statement. The seed `INSERT` supplies `current_user.user_id` explicitly and the final `UPDATE` carries an
explicit `WHERE user_id = $1 AND manifestation_id = $2`; the `reading_state_owner` policy's `WITH CHECK` clause
validates both redundantly. The `SELECT … FOR UPDATE` between them carries no `user_id` predicate at all, so the
policy's `USING` clause is the only thing confining that lock to the caller's own row. `ensure_manifestation_visible`
gates both handlers on a different table's policy — manifestation visibility, not `reading_state` ownership — before
either statement runs; that check belongs to the Design "Row-level security and database context" and is not this one's
own axis. Neither handler calls `require_not_child`: the module's own doc comment states this is deliberate, since
reading state is data a reader keeps about themselves, not a curation surface a child is barred from. This is the
ownership axis REV-ADR-0028 assigns to the data layer, used here in place of, not alongside, a role gate.

`notes` and `rating` are bounded twice: once in the handler (`NOTES_MAX_CHARS`, the `1..=5` range check) and once by a
database `CHECK` constraint on the same column, so a value reaching the table by any write path other than this
subject's own handler is still bounded. The precondition check runs under the same row lock the merge applies against
(`SELECT … FOR UPDATE` before the entity-tag comparison, before the `UPDATE`, all in one transaction), so a concurrent
writer cannot slip a change in between the check and the write.

There is no operator-facing configuration and no process this subject starts, restarts or scales.
