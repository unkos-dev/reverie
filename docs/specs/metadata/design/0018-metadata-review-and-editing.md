---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0018"
title: "Metadata review and editing"
satisfies:
  - "REV-REQ-0052"
  - "REV-REQ-0055"
  - "REV-REQ-0056"
---

# Metadata review and editing

This Design covers how an operator reviews, accepts, rejects, reverts, locks and manually edits a manifestation's or
work's bibliographic metadata: the `metadata_versions` journal and its canonical-pointer promotion mechanism, the
review-queue reads, the accept/reject/revert/lock/unlock routes, the manual `PATCH` surface with its per-field
apply/clear dispatch and external-identifier editing, and the client's edit dialog and Versions tab that drive them.

## Purpose and boundaries

This subject owns `backend/src/routes/metadata.rs` end to end: the review-queue reads
(`GET /api/v1/manifestations/{id}/metadata`, `GET /api/v1/works/{id}/metadata`), the accept, reject, revert, lock and
unlock routes, the matched `GET`/`PATCH` pair at `/api/v1/books/{id}/metadata` that an operator edits through, the
field-dispatch helpers those routes share (`apply_version`, `clear_field`, `apply_contributors_patch`,
`apply_vocabulary_patch`, `apply_identifier_patch(es)`, `insert_manual_version`), and the client surface that drives
them: `frontend/src/api/metadata.ts`, the metadata- and identifier-editing exports of `frontend/src/api/books.ts`, and
`frontend/src/pages/book/{EditMetadataDialog,VersionsTab}.tsx`. It owns the `metadata_versions` journal's promotion
mechanism (canonical pointers, junction `source_version_id`s) and the manual-edit and resolution write paths onto it
(`insert_manual_version`, `reject_manifestation`); it does not own every writer of the journal itself or of the
canonical columns it promotes into (see the state-writer census).

It does not own the `If-Match` precondition grammar, the strong entity-tag hashing (`hash_etag`), the `428`/`412`
contract, or the client's ETag capture-and-replay: that is the Design "Conditional requests and optimistic concurrency",
and this subject only states where its own routes plug into it. It does not own the background flush of an applied
canonical value into the on-disk `EPUB` file: that is the Design "Writeback pipeline"; this subject only enqueues a
`writeback_jobs` row (`enqueue_writeback`) and never touches the file itself. It does not own the enrichment
orchestrator, which decides which incoming observation auto-applies and, separately from this subject's own
`apply_version`/`clear_field`, runs its own canonical-column and pointer writes when one does
(`backend/src/services/enrichment/orchestrator.rs`), nor the `field_locks` CRUD and lookup functions themselves
(`backend/src/services/enrichment/field_lock.rs`), nor the automatic-apply decision that consumes a lock flag computed
in advance (`backend/src/services/enrichment/policy.rs`); all three belong to the Design "Enrichment pipeline". This
subject's `lock`/`unlock` routes are thin callers of that module's `lock`/`unlock` functions, and its own manual-edit
path never consults a lock (see Security and operations). It does not own the FRBR data model (`works`,
`manifestations`, authors, or the ISBN-triggered work-rematch algorithm in `backend/src/models/work.rs`), which is the
Design "Works and manifestations data model"; this subject calls `work::rematch_on_isbn_change`,
`work::find_or_create_author` and `work::refresh_first_author_sort` without describing their internals. It does not own
the two-level external-identifier registry's provider-visibility or rating-cache concerns
(`backend/src/models/external_identifier.rs`, `backend/src/services/metadata/external_id.rs`); this subject owns only
the *editing* path onto that registry (the canonical `identifiers.<level>.<scheme>` field addressing, validation, and
journal-integrated apply/clear), not the registry's storage model or read-side visibility filtering. `cover` is out of
scope: neither `apply_version` nor `clear_field` dispatches on it, and `UpdateMetadataFields` carries no `cover` key,
even though a `cover_version_id` canonical-pointer column exists on `manifestations` for another subject to fill.

Depends on: `crate::db::acquire_with_rls` and the `manifestations`/`works` row-level-security policies (Design
"Row-level security and database context") for every route; `CurrentUser::require_not_child` and `require_scope` (Design
"Authorization axes") on every route; `routes::etag::hash_etag`, `routes::etag::if_match_mismatch` and
`routes::etag::parse_if_match` for the manual `PATCH`'s precondition; `services::metadata::isbn` and
`services::metadata::external_id` for field-level parsing; `routes::library::load_genres_for_manifestations`,
`routes::library::load_moods_for_manifestations` and `routes::library::load_tags_for_manifestations` to assemble the
editable metadata span.

Depended on by: `GET /api/v1/books/{id}` (`backend/src/routes/library/mod.rs`), which reads `metadata_versions` directly
to populate the book-detail Versions-tab payload: a dependency in the other direction from every other route this
subject owns, and described only as an interface here, since that handler belongs to the Books list query contract
subject. The client library table's cell-editing pipeline also targets `PATCH /api/v1/books/{id}/metadata` as one of its
two write routes, a dependency of the Library table cell editing and undo subject on this one.

## Structure

### State-writer census

- `works.{title,subtitle,description,language}` and the matching `{title,subtitle,description,language}_version_id`
  pointers: this subject writes them via `apply_version` (accept, revert-to-version, manual set) or `clear_field`
  (revert-to-null, manual clear), under the `FOR UPDATE OF m, w` lock its mutating routes take. The enrichment
  orchestrator (`backend/src/services/enrichment/orchestrator.rs`, part of the Design "Enrichment pipeline") is a second
  writer of every one of these columns and pointers, through its own scalar-apply code, under its own
  `SELECT ... FOR UPDATE` on the work row.
- `manifestations.{pages,publisher,isbn_10,isbn_13,pub_date}` and their matching `*_version_id` pointers: same two
  writers as above, scoped to one manifestation instead of its work. `content_rating` and `content_rating_version_id`
  are the one field in this family this subject alone writes; the enrichment orchestrator's scalar-apply code has no
  `content_rating` arm.
- `work_authors` rows and their `source_version_id`: this subject rebuilds the role wholesale via `delete_role_rows` +
  `insert_role_rows`, from either `apply_version`'s `contributors.*` arm or `apply_contributors_patch`; this subject is
  the only writer. The enrichment orchestrator's field-apply dispatch has no `contributors.*` arm, so an incoming
  contributor observation always stays staged in `metadata_versions` for a human reviewer, regardless of confidence.
- `manifestation_genres`, `manifestation_moods`, `manifestation_tags` junction rows and their `source_version_id`:
  rebuilt wholesale per field by `delete_vocabulary_rows` + `insert_vocabulary_rows`, from either `apply_version`'s
  vocabulary arm or `apply_vocabulary_patch`. The enrichment orchestrator has no writer for these three tables.
- `work_external_identifiers` and `manifestation_external_identifiers` registry rows: written by
  `upsert_{work,manifestation}_identifier` / `delete_{work,manifestation}_identifier`
  (`backend/src/models/external_identifier.rs`), called from this subject's `apply_version` and `clear_field`
  `identifiers.*` arms and also called directly by the enrichment orchestrator, which upserts the same rows itself.
- `metadata_versions` rows: this subject writes it two ways, `insert_manual_version` inserts or updates a row for every
  manual edit (PATCH set or clear, on any field family), and `reject_manifestation` updates a row's `status` to
  `rejected`. The enrichment orchestrator is a further writer, inserting the proposed rows this subject's
  `accept_manifestation` reads and promotes without inserting a row of its own.
- `field_locks` rows: written only by `lock_field`/`unlock_field`, through `field_lock::{lock_tx,unlock_tx}`. The
  enrichment orchestrator reads this table, via `field_lock::is_locked_tx`, to produce the flag computed in advance that
  its automatic-apply decision consumes; neither the orchestrator nor the decision itself writes it.
- `writeback_jobs`: insert-only from this subject, via `enqueue_writeback`, once per pointer-move inside the same
  transaction as the move. The Design "Writeback pipeline" owns every other writer and every reader of this table.
- The manifestation's `enrichment_status`, `enrichment_rerun_requested`, `enrichment_attempt_count`,
  `enrichment_attempted_at` and `enrichment_error` columns: conditionally reset by `apply_identifier_patches` when an
  identifier map is touched (see Runtime behaviour); the enrichment worker is the other writer of these columns, out of
  this subject's scope.
- `works.first_author_sort_name`: recomputed by `work::refresh_first_author_sort`, called whenever an `author`-role edit
  touches `work_authors` (both `apply_version`'s contributors arm and `apply_contributors_patch`, gated on
  `author_touched`).

Every write above that this subject makes against `works` or `manifestations` columns runs inside the transaction that
took the `FOR UPDATE OF m, w` lock at the top of the handler (`accept_manifestation`, `revert_manifestation`,
`update_book_metadata`); the enrichment orchestrator takes its own `SELECT ... FOR UPDATE` on the same row before its
own scalar-apply writes. Both are ordinary Postgres row locks on the same physical row, so a concurrent write from
either subject against the same manifestation or work still serialises against the other, even though the two lock
statements are not identical. `lock_field` and `unlock_field` also open an RLS-scoped transaction, lock the visible
manifestation row, and mutate `field_locks` before committing, so a missing or hidden manifestation returns `404` and a
concurrent manifestation deletion cannot leave the field-lock foreign-key write as an internal error.

### Component relationships

- `router()` makes eight `.routes(...)` calls, the last of which registers a `GET` and a `PATCH` together, for nine
  operations total on `AppState`: the two review-queue `GET`s, `accept`, `reject`, `revert`, `lock`, `unlock`, and the
  `get`/`update` pair at `/api/v1/books/{id}/metadata`.
- `apply_version` and `clear_field` are the two field-dispatch cores every promotion or clear routes through: one
  `match` arm per supported canonical field, each doing the type-specific parse (ISBN checksum, `content_rating` enum,
  `pub_date` via `parse_iso_date`), the `UPDATE ... RETURNING` that swaps the pointer and returns what it replaced, and,
  for every field but an `identifiers.*` one, a call to `enqueue_writeback`. `is_work_scoped_field` decides whether a
  version journaled under one manifestation is a legitimate revert target for a sibling manifestation of the same work
  (`title`/`subtitle`/`description`/`language`, every `contributors.*` role, and `identifiers.work.*`); the
  per-manifestation fields (`genres`/`moods`/`tags`, `identifiers.manifestation.*`) are not. `accept_manifestation`
  never extends this sibling allowance: its row lookup matches only `mv.manifestation_id = $2`, so accepting a
  work-scoped draft is possible only from the exact manifestation that observed it, while reverting to the same version
  id from a sibling manifestation of the same work succeeds.
- `apply_contributors_patch` and `apply_vocabulary_patch` are the manual-`PATCH`-only counterparts of the
  `contributors.*` and vocabulary arms inside `apply_version`/`clear_field`: they additionally call
  `insert_manual_version` themselves (accept/revert apply a version that was already journaled) and return a
  `FieldVersionChange` per touched key for the response body.
- `apply_identifier_patch(es)` is the manual-`PATCH` counterpart for `identifiers.<level>.<scheme>`: it validates the
  scheme against the level (`external_id::validate_scheme_level`) before writing anything to the journal, so an unknown
  scheme or a wrong-level address produces no journal row on either the set or the clear path.
- `load_book_metadata` is the one assembly function `get_book_metadata`, `update_book_metadata`'s precondition check,
  and its post-write re-hash all share, so the `ETag` a `GET` returns, the tag a `PATCH`'s `If-Match` is checked
  against, and the tag a successful `PATCH` echoes are all hashes of the identical representation.
- On the client, `frontend/src/api/metadata.ts` wraps `accept`/`reject`/`revert` under the legacy
  `/api/v1/manifestations/` prefix; `frontend/src/api/books.ts` wraps `getBookMetadata`/`updateBookMetadata` under
  `/api/v1/books/`. Both prefixes address the same `manifestations.id`. `VersionsTab.tsx` groups
  `book.metadata_versions` by `field_name`, renders each pending row with Accept/Reject, and a per-field Clear action
  that calls `revertField` with a `null` version id; `EditMetadataDialog.tsx` is the manual-edit sheet it opens,
  tracking a per-field `touched` flag so only edited keys reach the `PATCH` body.

### Three distinct review-queue reads

- `GET /api/v1/manifestations/{id}/metadata` (`load_versions`) selects every `metadata_versions` row for the given
  `manifestation_id` at any status (`pending` or `rejected`), with no join to `manifestations` at all.
- `GET /api/v1/works/{id}/metadata` selects across every manifestation of a work, also at any status, but its query
  joins only `manifestations` to reach `m.work_id = $1` (a column on `manifestations` itself, so no join to `works` is
  needed): a join that puts that table's own row-level-security policies in the query plan, unlike the
  manifestation-scoped read above.
- The book-detail Versions tab draws from neither of these: it reads `book.metadata_versions`, populated by
  `load_pending_versions` in `backend/src/routes/library/mod.rs`, which filters to `status = 'pending'`, excludes any
  row already wired as a canonical pointer (via the canonical-pointer and junction-pointer id sets), excludes
  null-valued audit rows (`new_value != 'null'::jsonb`), and caps the result at `MAX_PENDING_VERSIONS` (200). The
  client's `api/metadata.ts` module has no wrapper for either `GET` review-queue route above; `VersionsTab.tsx` sources
  its list from `GET /api/v1/books/{id}` instead.

## Interfaces and dependencies

- `GET /api/v1/manifestations/{id}/metadata`, `GET /api/v1/works/{id}/metadata`: review-queue reads, `MetadataRow`
  shaped, `read` scope, `require_not_child`.
- `POST /api/v1/manifestations/{id}/metadata/accept` (`VersionPayload`), `.../reject` (`VersionPayload`), `.../revert`
  (`RevertPayload`): `write` scope, `require_not_child`.
- `POST /api/v1/manifestations/{id}/metadata/lock`, `.../unlock` (`LockPayload`: `field_name`, `entity_type`): `write`
  scope, `require_not_child`.
- `GET /api/v1/books/{id}/metadata` (`read` scope) / `PATCH /api/v1/books/{id}/metadata` (`UpdateMetadataFields` in,
  `UpdateMetadataResponse` out, `write` scope): the matched editable-metadata pair; `PATCH` additionally requires an
  `If-Match` header whose grammar and status contract belong to "Conditional requests and optimistic concurrency".
- `frontend/src/api/metadata.ts` (`acceptVersion`, `rejectVersion`, `revertField`) and the metadata exports of
  `frontend/src/api/books.ts` (`getBookMetadata`, `updateBookMetadata`, `UpdateBookMetadataFieldsSchema`,
  `BookMetadataSchema`) are the typed client boundary; both route through the shared `apiFetch`, so CSRF-token
  attachment and Problem Details parsing (owned elsewhere) apply uniformly.
- `enqueue_writeback` is this subject's one call into the writeback pipeline's interface: an
  `INSERT INTO writeback_jobs (manifestation_id, reason)`, `reason` being `"cover"` or `"metadata"`.

## Data and state

- **`metadata_versions`.** One row per observed (manifestation, source, field, value) combination, with duplicates
  suppressed on `(manifestation_id, source, field_name, value_hash)`. `status` is a two-value enum, `pending` or
  `rejected`; there is no `applied`/`accepted` value. A row promoted to canonical keeps `status = 'pending'`: promotion
  is recorded solely by a `*_version_id` pointer column (or a junction row's `source_version_id`) referencing the row's
  id, never by the enum. Rows accumulate without limit; nothing in this subject deletes one. The table carries no
  row-level-security policy of its own (see Security and operations).
- **`field_locks`.** One row per `(manifestation_id, entity_type, field_name)` (its primary key), recording the locking
  user (`locked_by`, nullable) and timestamp. A duplicate lock is a no-op (`ON CONFLICT DO NOTHING`); an unlock of a
  non-existent lock reports `false` back to the route, which the route turns into `404`. Also carries no
  row-level-security policy.
- **Canonical pointers.** Each editable scalar field on `works` or `manifestations` has a matching nullable
  `*_version_id` column; `NULL` means the field has never been set through this journal (or has been cleared). Each
  vocabulary junction row and each `work_authors` row carries its own `source_version_id` instead of a shared column,
  because a field with a set-valued canonical form has no single pointer to hang off `works`/`manifestations`.
- **`enrichment_rerun_requested`/`enrichment_status` etc.** Touched only when a manual identifier edit lands
  (`apply_identifier_patches`): if the manifestation's enrichment is not `in_progress`, its status resets to `pending`
  with the attempt count, attempted-at, and error columns cleared, so the edit's new identifier is available to the next
  enrichment run without an artificial backoff wait; if a run is `in_progress`, only the `enrichment_rerun_requested`
  flag is set (see Runtime behaviour for why the row itself is left alone).

## Runtime behaviour

**A manual `PATCH`: an operator edits publisher, clears pub_date, and sets a work identifier from the edit dialog.**

1. `EditMetadataForm` reads `GET /api/v1/books/{id}/metadata` through a `useQuery` on the `["books","metadata",id]` key
   (30-second default staleness; a reopen inside that window can serve the cached body without a new request rather than
   guaranteeing a fresh one). `apiFetch` captures the response's `ETag`.
2. On submit, `updateBookMetadata` sends the touched-only body; `apiFetch` auto-echoes the captured tag as `If-Match`
   since the caller set none.
3. `update_book_metadata` parses `If-Match` (`parse_if_match`), requiring it present (`428` otherwise), then opens
   `acquire_with_rls` and takes `SELECT ... FOR UPDATE OF m, w` on the manifestation and its work: `404` if RLS hides or
   the id does not exist.
4. Under that lock, `load_book_metadata` + `hash_etag` recompute the current tag and compare it to `If-Match`; a
   mismatch returns `412` with the current tag before anything else is checked, per RFC 9110 §13.2.1 precondition
   ordering.
5. Only once the precondition holds does the handler reject an entirely-empty body (`422`). It then dispatches each
   touched family in turn: scalars through `apply_scalar_patch_field` (ISBN fields normalise and checksum-validate
   first), vocabularies through `apply_vocabulary_patch`, contributors through `apply_contributors_patch`, and
   identifiers through `apply_identifier_patches`. Every family journals via `insert_manual_version` before it applies
   or clears, so the journal row exists even if a subsequent family in the same request fails and rolls the whole
   transaction back. Writeback enqueue granularity differs by family: each scalar and each vocabulary field enqueues its
   own job as it applies, `apply_contributors_patch` journals one `contributors.<role>` version per role touched but
   calls `enqueue_writeback` once for `contributors` after its role loop, and the identifier family never enqueues.
6. If an ISBN field was touched, `work::rematch_on_isbn_change` runs before the response is assembled.
7. `load_book_metadata` + `hash_etag` run again, inside the same transaction, to compute the tag the response's `ETag`
   header carries; the transaction commits; the response carries the applied value, new version id, and previous version
   id per touched field.

**Accepting a pending enrichment draft (the Versions tab's Accept button) for `isbn_13`.**

1. Accept and reject open Read Committed RLS transactions and acquire a transaction advisory lock keyed by the version
   ID in the metadata-review namespace before accessing the version. Acceptance reads eligibility in a subsequent
   statement, so a rejection committed while it waits is visible. The lock lasts through commit or rollback; it
   serialises these two endpoints for one version, not metadata updates generally. If acceptance commits first,
   subsequent rejection still succeeds without undoing the canonical value or its queued writeback.
2. `accept_manifestation` selects only a `pending` version for the requested manifestation and locks the manifestation
   and work (`FOR UPDATE OF m, w`) in the same query. A missing, mismatched, or already-rejected version returns `404`:
   none identifies an eligible draft for this operation. The Versions tab refreshes book details after failed acceptance
   to reconcile stale drafts. Acceptance leaves review status `pending`; canonical pointers record promotion. Repeated
   acceptance remains allowed and enqueues another writeback job for file-backed fields.
3. `apply_version` rejects the row outright if its `new_value` is JSON `null` (a manual-clear audit row, never a draft
   eligible for promotion), `422`. Otherwise it normalises the string, runs the `isbn_13` `UPDATE ... RETURNING`, and
   swaps the pointer.
4. Because the field is an ISBN, `work::rematch_on_isbn_change` runs before commit: accepting an ISBN draft can regroup
   the manifestation under a different work.
5. `enqueue_writeback` inserts a `writeback_jobs` row in the same transaction the pointer moved in.

**Reverting a field, to a specific version versus to null.**

- **Revert-to-version** (`RevertPayload.version_id = Some(vid)`): the same-manifestation match is used unless
  `is_work_scoped_field` says the field is work-scoped, in which case a version journaled under any sibling
  manifestation of the same work is also accepted; `apply_version` then runs exactly as the accept path's step 2-4.
- **Revert-to-null** (`version_id: None`): `clear_field` nulls the canonical column and its pointer, returning what the
  pointer held before. `title` and `contributors.author` refuse this (`422`), a work's title and its author list are
  never allowed to become empty through this path. `enqueue_writeback` still runs for every field that can be cleared
  but `identifiers.*`.

Either revert branch leaves the field's canonical column and pointer as the sole record of what is current:
revert-to-version copies the targeted `metadata_versions` row's value onto the canonical column and moves the pointer of
`*_version_id` (or the junction rows' `source_version_id`) to that row's id, whatever that row's own `status` reads;
revert-to-null sets both the canonical column and its pointer to `NULL`. Neither branch changes any `metadata_versions`
row's `status`, so the version that was canonical before the revert, and every other pending or rejected draft for the
field, stays in the journal unchanged. Pending drafts remain available to a subsequent accept or revert; rejected drafts
remain available to a subsequent revert.

**A manual identifier edit while enrichment is `in_progress`, for the same manifestation.**

`apply_identifier_patches` inspects the manifestation's current `enrichment_status` in the same `UPDATE` that resets it.
An `in_progress` row is never flipped back to `pending` here: the active worker still owns its claim, and making the row
newly eligible would let a second worker start against the same manifestation concurrently. Instead only
`enrichment_rerun_requested` is set; the worker's own completion bookkeeping (owned by the Design "Enrichment pipeline")
is what turns that flag into a fresh eligible row once the in-flight run finishes, since that run's lookup keys, read
once at the start, may already be stale against the edit.

**Locking or unlocking a field** (`POST .../metadata/lock` or `POST .../metadata/unlock`) opens an `acquire_with_rls`
transaction, takes `SELECT id FROM manifestations WHERE id = $1 FOR UPDATE`, and returns `404` when the manifestation is
missing or hidden by the caller's RLS policy. It then calls `field_lock::{lock_tx,unlock_tx}` on the same transaction
before committing. Lock insertion remains idempotent (`ON CONFLICT DO NOTHING`); unlock reports `404` when the visible
manifestation has no matching lock.

## Failure and recovery

- `AppError::IfMatchRequired` (`428`) when `PATCH /api/v1/books/{id}/metadata` carries no `If-Match`; a malformed or
  policy-refused header (wildcard, weak tag, tag list, repeated header) is `AppError::MalformedHeader` (`400`); a
  non-matching tag is `AppError::IfMatchMismatch` (`412`, carrying the current tag). All three are evaluated before
  request-body validation.
- `AppError::Validation` (`422`) covers: an entirely empty `PATCH` body; a malformed or out-of-range `pub_date`
  (`PubDateError::Malformed`/`PubDateError::YearOutOfRange`, both wrapped with their message); an invalid ISBN checksum,
  length, or non-numeric ISBN; an attempt to clear `title` or `contributors.author`; an author-role rewrite that would
  leave a previously-authored work with zero authors; an over-cap or duplicate contributor/vocabulary/identifier list;
  an unknown vocabulary/role/scheme, or an identifier whose scheme does not belong to its stated level; a malformed
  identifier value; and accepting a `null`-valued audit row directly.
- The zero-authors guard is not one rule but two, with different comparisons. `apply_version`'s `contributors.author`
  arm (accept and revert-to-version) rejects whenever the role's post-write count is zero, with no reference to what it
  was before, so it also blocks re-promoting an old author list onto a work that has no author by design.
  `apply_contributors_patch` (the manual `PATCH`) rejects only when the post-write count is zero *and* the pre-write
  count was greater than zero, so a manual edit may leave a stub that already has no author still without one (clearing
  it, or patching only `editor`/`translator`, both succeed); accept/revert cannot.
- `AppError::NotFound` (`404`) for a missing or RLS-hidden manifestation or work on any route that joins one, for a
  version id that does not belong to the manifestation or eligible sibling named in the request, and for `unlock` when
  no matching `field_locks` row exists.
- `accept_manifestation` filters the target version at `status = 'pending'`; `revert_manifestation` accepts a target
  version at either status so it can restore a prior journal entry. `reject_manifestation` does not check whether the
  row it is marking `rejected` is the field's current canonical pointer. Canonical state is carried entirely by the
  pointer columns and junction `source_version_id`s, never by this enum, so a pointer can reference a
  `metadata_versions` row whose `status` reads `rejected`.
- A `field_locks` row constrains only the enrichment policy engine's automatic apply decision; the manual `PATCH`
  surface in this subject never reads `field_locks`, so a manual edit overwrites a locked field the same as an unlocked
  one.

## Security and operations

Every mutating route (`accept`, `reject`, `revert`, `lock`, `unlock`, `PATCH`) calls
`current_user.require_scope(Scope::Write)` and `current_user.require_not_child()` before touching state; the three
`GET`s call only `require_not_child()`, since authentication already refuses a credential with no scope at all. No route
in this subject distinguishes `admin` from `adult`: either role passes `require_not_child`. A child account therefore
cannot reach any of this subject's routes that mutate it regardless of the scopes its credential carries, because
`require_not_child` (`backend/src/auth/middleware.rs`) is checked independently of `require_scope` and rejects a child
caller with `Forbidden` before a handler writes anything; the three review-queue `GET`s refuse a child caller on the
same basis, and the book-detail read outside this subject returns a child caller an empty pending list rather than the
proposals it cannot act on.

`metadata_versions` and `field_locks` carry no row-level-security policy of their own: neither table appears in either
migration that enables row-level security. This subject's visibility rests on two things instead: `require_not_child` on
every route, and, for `GET /api/v1/works/{id}/metadata`, the join through `manifestations`, whose own policies (Design
"Row-level security and database context") filter which manifestations the join can reach. The manifestation- scoped
review read (`GET /api/v1/manifestations/{id}/metadata`) queries `metadata_versions` directly with no join, so no policy
filters it there; because the `manifestations` policies already admit every row to an `adult` or `admin` caller, and
this subject refuses every other caller outright, that absence of a join-side filter selects the same rows a filtered
join would have.

`lock_field` and `unlock_field` use `acquire_with_rls`, resolve the manifestation through its RLS policy, and hold its
row lock while `field_lock::{lock_tx,unlock_tx}` mutates the unprotected `field_locks` table. The visibility check and
mutation share one transaction, so the foreign-key write cannot race a concurrent manifestation deletion.

Every field-dispatch `match` in `apply_version` and `clear_field` compares the caller-influenced field name against
fixed string literals (or, for `identifiers.*`, against the registry's own scheme list via
`external_id::validate_scheme_level`); no field name, scheme, or value is ever interpolated into SQL text. Contributor
names, vocabulary terms, and identifier values are bounded (`MAX_CONTRIBUTOR_NAME_CHARS` = 500,
`MAX_CONTRIBUTORS_PER_ROLE` = 100, `MAX_VOCABULARY_TERM_CHARS` = 100, `MAX_VOCABULARY_TERMS_PER_FIELD` = 50,
`MAX_IDENTIFIERS_PER_LEVEL` = 64) before any database round trip that writes them.
