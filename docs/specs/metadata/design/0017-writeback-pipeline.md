---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0017"
title: "Writeback pipeline"
satisfies:
  - "REV-REQ-0052"
  - "REV-REQ-0053"
  - "REV-REQ-0054"
governed-by:
  - "REV-ADR-0018"
  - "REV-ADR-0020"
  - "REV-ADR-0051"
---

# Writeback pipeline

This Design covers the background mechanism that flushes an accepted or manually-applied canonical metadata change into
the on-disk `EPUB` file it describes: the `writeback_jobs` durable queue and its one-in-progress-per-manifestation
index, the bounded worker and dedicated database pool, OPF and cover planning, validated candidate publication,
contained relocation and terminal-event deduplication. Source locations select their owning library capability.

## Purpose and boundaries

This subject owns everything between a canonical-pointer move landing in Postgres and the corresponding `EPUB` file on
disk coming to reflect it: the `writeback_jobs` table and its partial unique index
(`idx_writeback_jobs_in_progress_unique`), the worker loop and claim logic (`backend/src/services/writeback/queue.rs`),
the dedicated pool it runs on (`db::init_writeback_pool`, `backend/src/db.rs`), the per-job orchestrator
(`backend/src/services/writeback/orchestrator.rs`), the pure routine that rewrites the `OPF` and the cover-embed planner
(`backend/src/services/writeback/opf_rewrite.rs`, `backend/src/services/writeback/cover_embed.rs`), the contained
relocation helpers (`backend/src/services/writeback/path_rename.rs`), and the terminal-event duplicate-suppression gate
(`backend/src/services/writeback/events.rs`, the `webhook_event_dedupe` table). It also owns `WritebackConfig`
(`backend/src/config/writeback.rs`), the worker's own runtime knobs.

It does not own the `app.system_context` row-level-security mechanism the writeback pool relies on to reach the
`manifestations_*_system` policies; that is the Design "Row-level security and database context", and this subject only
states how it attaches to that mechanism. It does not own `EPUB` structural validation and repair
(`backend/src/services/epub/mod.rs::validate_and_repair`, `backend/src/services/epub/repack.rs`,
`backend/src/services/epub/repair.rs`), which is the Design "EPUB validation and repair"; this subject checks the opened
source and final candidate before publication and reacts to their reports without describing its internals. It does not
own the pipeline that decides which fields auto-apply and calls into this subject's enqueue path
(`backend/src/services/enrichment/orchestrator.rs`), which is the Design "Enrichment pipeline". It does not own the
metadata review and editing routes that also enqueue writeback jobs on manual accept, revert, or `PATCH`
(`backend/src/routes/metadata.rs`), which is the Design "Metadata review and editing". It does not own the path-template
renderer this subject reuses for post-writeback file relocation (`backend/src/services/ingestion/path_template.rs`),
which is the Design "Ingestion pipeline". It does not own the enrichment-downloaded sidecar cover at
`manifestations.cover_path`, the `SSRF`-guarded remote-cover `HTTP` client, or the `_covers/pending` and
`_covers/accepted` staging directories: the Design "Covers" assigns that download and staging area to the Design
"Enrichment pipeline". This subject only reads the sidecar bytes at the path a job's row carries and, on a successful
cover-reason job, promotes the file from `_covers/pending/` to `_covers/accepted/` with a bare rename. No code path
anywhere in `backend/src` writes `manifestations.cover_path`, so this subject's cover-embed step never runs against real
sidecar bytes in a live system; see Interfaces and dependencies and Runtime behaviour for the fuller picture, including
why a cover-reason job cannot be created at all today. `webhooks` and `webhook_deliveries` have tables and row-level
security enabled but carry no policy and no handler anywhere in `backend/src`; this subject's only connection to
webhooks is the duplicate-suppression table it shares a name prefix with and the `tracing`-emit stub that stands in for
delivery, both described below as what exists today.

Depends on: the two enqueue call sites named above, each of which inserts a `writeback_jobs` row inside the same
transaction that moves a canonical pointer; the `EPUB` validation and repack service the Design "EPUB validation and
repair" owns, for pure source and final-candidate checks; the path-template renderer the Design "Ingestion pipeline"
owns, for post-writeback file relocation; and the `manifestations_update_system` and `manifestations_select_system`
row-level-security policies the writeback pool's connections are built to satisfy.

Depended on by: nothing inside the request path. The worker is a background task started once at process startup
(`backend/src/lib.rs::run`) and is never reachable from a handler; its externally visible effects are the
`writeback_jobs` row it claims, the `manifestations` columns it updates, the on-disk file it rewrites (and, on a
cover-reason job, the cover sidecar it moves), and the `webhook_event_dedupe` rows it writes and opportunistically
purges.

## Structure

### State-writer census

| State item | Writer(s) |
| ---------- | --------- |
| `writeback_jobs` row creation | Two sites, both named `enqueue_writeback` (below) |
| `writeback_jobs` status/attempt columns | `claim_next`, the three `mark_*` functions, `revert_in_progress` |
| `manifestations.current_file_hash`/`.has_embedded_cover`/`.file_size_bytes` | `orchestrator::run_once`'s post-publication `UPDATE`, before relocation (ingestion also writes `current_file_hash`, once, at row creation) |
| `manifestations.file_path` | `orchestrator::path_rename_step` |
| `webhook_event_dedupe` rows | `events::dispatch`, called only from `queue::finish` |
| The on-disk `EPUB` file's bytes | `epub::repack::publish` / `path_rename::move_existing` (orchestrator only) |

The two enqueue sites are `enqueue_writeback` in `backend/src/routes/metadata.rs` (manual accept, revert, and `PATCH`,
inside the caller's `acquire_with_rls` transaction on the request pool) and `enqueue_writeback` in
`backend/src/services/enrichment/orchestrator.rs` (auto-apply, inside a transaction on the ingestion pool). Each row's
`reason` is `CHECK`-constrained to `'metadata'` or `'cover'`. `writeback_jobs.status` moves
`pending → in_progress → {complete, failed, skipped}`; `queue::claim_next` is the only writer of `in_progress`,
`revert_in_progress` the only writer that moves a row back to `pending`, and `mark_complete`/`mark_skipped`/
`mark_failed` the only writers of the three terminal states. `manifestations.current_file_hash` and
`.has_embedded_cover` and `.file_size_bytes` come from accepted candidate evidence; `manifestations.file_path` is
rewritten only when the rendered library path differs from the current one. `webhook_event_dedupe` holds one row per
`(job_id, outcome)` event id, written once on first delivery and refreshed only when a subsequent dispatch of that same
id is itself delivered past the TTL; a dispatch suppressed as a duplicate returns before touching the table at all. The
on-disk `EPUB` file's bytes are mutated only through an atomic commit, never in place, as described below.

`writeback_jobs` and `webhook_event_dedupe` carry no row-level-security policy: neither table appears in an
`ALTER TABLE ... ENABLE ROW LEVEL SECURITY` statement in the migrations, so what scopes which database role may touch
them is the ordinary table grant, not a policy predicate. `reverie_app` holds `SELECT`, `INSERT`, `DELETE` and `UPDATE`
on both; `reverie_ingestion` holds `SELECT` and `INSERT` on `writeback_jobs` only (the grant the enrichment
orchestrator's enqueue call relies on); `reverie_readonly` holds `SELECT` on both. Neither enqueue call site performs an
ownership or visibility check of its own on the `writeback_jobs` insert; the visibility check, when one applies, already
happened on the caller's read of the manifestation before the pointer move.

One state item has no single owner in the sense the census above otherwise holds to: the on-disk cover sidecar under
`_covers/pending/`. On a successful cover-reason job, `orchestrator::move_cover_sidecar` promotes it to
`_covers/accepted/` with a bare `std::fs::rename`, independently of managed EPUB publication and relocation. This is the
one file mutation in the pipeline that bypasses the atomic-commit helpers; it is deliberately best-effort (a failure is
logged at `warn!` and does not fail the job), and the file it moves is a sidecar, never the managed `EPUB` itself. As
noted in Purpose and boundaries, nothing in the current system writes the sidecar this function reads, so the function
has no live input to act on outside its own tests.

### Component relationships

- `backend/src/services/writeback/mod.rs` is the module root; it re-exports `queue::spawn_worker`, the only symbol
  anything outside this module calls (`backend/src/lib.rs::run` is the sole caller).
- `backend/src/services/writeback/queue.rs` owns the worker loop (`spawn_worker`), the claim `CTE` (`claim_next`), and
  the terminal-state bookkeeping (`finish`, `mark_complete`, `mark_skipped`, `mark_failed`, `revert_in_progress`).
  `spawn_worker` calls `orchestrator::run_once` for each claimed job inside a `tokio::spawn`ed task, gated by a
  `tokio::sync::Semaphore` sized to `WritebackConfig.concurrency`.
- `backend/src/services/writeback/orchestrator.rs` owns the per-job pipeline: `run_once` loads a `JobSnapshot`
  (`load_snapshot`), skips early for an unsupported format or a missing file, rewrites the `OPF`
  (`opf_rewrite::transform`), optionally plans a cover embed (`cover_embed::plan_embed`), repacks
  (`epub::repack::with_modifications`), publishes the accepted candidate (`epub::repack::publish`), renders a checked
  relative destination (`path_rename_step`, `render_target_path`), and writes the accepted hash, cover flag and size.
  ZIP, filesystem and sidecar phases run in `spawn_blocking`; SQL stays on the async job.
- `backend/src/services/writeback/opf_rewrite.rs` (`transform`) is a pure function: given `OPF` bytes and a `Target`
  struct naming the desired per-field values, it streams the source through `quick_xml` events, replaces the targeted
  Dublin Core and `<meta>` elements that already exist, and inserts one when the target value is present but no matching
  element does (exercised today only for the `ISBN` identifier; the routine that rewrites the `OPF` supports the same
  insertion for series metadata, though no caller reaches it). Every other element is preserved byte-for-byte. The
  orchestrator always passes `series: None` today; no caller populates `Target::series`, so series membership is never
  written back through this path even though the routine that rewrites the `OPF` supports the field.
- `backend/src/services/writeback/cover_embed.rs` (`plan_embed`) inspects the `OPF` manifest to decide whether a new
  cover replaces an existing entry in place, is added under a fresh name because the format changed (the old entry then
  becomes orphaned in the archive, an accepted trade-off with nothing reclaiming it afterwards), or is inserted where no
  cover existed. It returns `OPF`-relative paths; the orchestrator translates them to `ZIP`-absolute paths by joining
  with the `OPF`'s own directory before passing them to the repack helper.
- `backend/src/services/writeback/path_rename.rs` opens actual source/destination parents, uses contained `Dir::rename`
  and falls back only on EXDEV. The fallback streams through a 64 KiB buffer into `cap-tempfile`, verifies the published
  destination independently and then removes the source.
- `backend/src/services/writeback/events.rs` (`dispatch`, `event_id`) is the terminal-event duplicate-suppression gate
  described in Runtime behaviour and Failure and recovery.
- `backend/src/config/writeback.rs` defines `WritebackConfig` (`enabled`, `concurrency`, `poll_idle_secs`,
  `max_attempts`), consumed only by `queue::spawn_worker` and `queue::mark_failed`.
- `backend/src/db.rs::init_writeback_pool` builds the dedicated pool this subject's worker runs on; `backend/src/lib.rs`
  constructs it once in `run`, before any worker is spawned, and passes it directly to `spawn_worker`. It is not a field
  on `AppState`, so no request handler can obtain it.

## Interfaces and dependencies

- **Enqueue.** Two independent call sites insert a `writeback_jobs` row: `enqueue_writeback` in
  `backend/src/routes/metadata.rs`, called from the manual accept, revert, and `PATCH` paths inside the caller's
  `acquire_with_rls` transaction on the request pool; and `enqueue_writeback` in
  `backend/src/services/enrichment/orchestrator.rs`, called from `apply_field`'s auto-apply path inside a transaction on
  the ingestion pool. The enrichment orchestrator's single call site carries an explicit
  `!field.starts_with("identifiers.")` guard. `backend/src/routes/metadata.rs` calls `enqueue_writeback` from four
  places: the version-apply and field-clear paths carry the identical guard; the contributors-patch and vocabulary-patch
  paths call it unconditionally, but the field name they pass (`"contributors"`, or a vocabulary field key) is never an
  `identifiers.*` name, so no call site in either module ever enqueues a job for an identifier field, and external
  identifiers are never written back to the source file. Each insert commits or rolls back with the pointer move that
  triggered it, in the same transaction. Both `enqueue_writeback` functions derive `reason` from the field name the same
  way: `'cover'` when the field is `"cover"` or `"cover_url"`, `'metadata'` otherwise. Neither call site can produce a
  `'cover'` reason in a running system today: `metadata.rs::apply_version`'s match has no arm for `"cover"` or
  `"cover_url"`, so accepting or reverting either field returns the handler's `unsupported auto-apply field` validation
  error before `enqueue_writeback` runs; and the enrichment orchestrator's policy engine
  (`services::enrichment::policy`) defaults `"cover_url"`, the only cover-shaped field any source emits, to `Propose`,
  which resolves to `Decision::Stage`, never `Decision::Apply`, and no source emits a field literally named `"cover"`,
  the one name `default_policy` marks `AutoFill`. Every `writeback_jobs` row a running system creates today therefore
  carries `reason = 'metadata'`; the cover-reason branch of this pipeline (`cover_embed::plan_embed`,
  `move_cover_sidecar`) is exercised only by this module's own tests, which insert a `'cover'`-reason row and a
  `cover_path` value directly.
- **Opened EPUB interfaces.** `epub::inspect(File)` returns the admitted source and pure baseline report. `RepairPlan`
  applies OPF repairs before `opf_rewrite::transform`. `with_modifications` writes repair, metadata and cover changes
  into one file-backed candidate. `repack::publish` validates final bytes, compares unresolved severity, and returns
  final report/hash/size only after durable replacement.
- **`services::ingestion::path_template::{render, DEFAULT_TEMPLATE}`** is reused for `path_rename_step`'s post-writeback
  file relocation, rendering the same template the Design "Ingestion pipeline" renders at ingestion time, from the job's
  `Title`/`Author` variables.
- **`db::init_writeback_pool(database_url, max_connections) -> Result<PgPool, sqlx::Error>`** is the pool constructor;
  every connection it opens runs `SELECT set_config('app.system_context', 'writeback', false)` once, in `after_connect`,
  before the pool hands it out.
- **Terminal events.** `events::dispatch(pool, &TerminalEvent) -> Dispatch` is the interface `queue::finish` calls,
  through its own private `dispatch_terminal` wrapper, after every terminal transition; delivery today is
  `events::deliver`, a `tracing::info!`/`warn!` emit with no HTTP transport, so `webhooks` and `webhook_deliveries` are
  not read or written anywhere this subject's code runs.

## Data and state

- **`writeback_jobs`.** One row per enqueue. `reason` is a `CHECK`-constrained `text` column (`'metadata'` or
  `'cover'`); `status` is the `writeback_status` enum (`pending`, `in_progress`, `complete`, `failed`, `skipped`).
  `attempt_count` and `last_attempted_at` drive the retry backoff described in Runtime behaviour. The partial unique
  index `idx_writeback_jobs_in_progress_unique` on `(manifestation_id) WHERE status = 'in_progress'` is the sole
  correctness guarantee behind "at most one in-flight job per manifestation"; nothing in application code enforces it
  independently.
- **`webhook_event_dedupe`.** `(event_id, seen_at)`, primary-keyed on `event_id`. `event_id` is the stable string
  `writeback:{job_id}:{outcome}`; `seen_at` is written on first delivery and refreshed only on a subsequent delivery of
  that same id, through its `ON CONFLICT DO UPDATE`. A call suppressed as a duplicate returns before reaching that
  write. Every delivered call also opportunistically deletes up to 100 rows past the 48-hour TTL, so the table needs no
  scheduled purge job; a call suppressed as a duplicate skips the purge too.
- **`manifestations.current_file_hash`, `.file_size_bytes` and `.has_embedded_cover`.** Ingestion writes these at row
  creation, using accepted repair evidence when available. Writeback is their other writer: it stores the accepted
  candidate hash, handle-derived size and cover flag in one async SQL update. Neither writer changes the immutable
  `ingestion_file_hash` after insertion.
- **`manifestations.file_path`.** Rewritten by `path_rename_step` only when `config.library_path` is non-empty and the
  rendered path differs from the job's current `file_path`. The location retains its `LibraryId` and a checked
  `RelativeFilePath`. Async SQL records a visible move; on SQL failure the same claim attempts contained move-back.
- **The on-disk `EPUB` file.** `epub::repack::publish` replaces its bytes using a finished, validated candidate.
  `path_rename::move_existing` relocates it beneath the owning library through same-filesystem rename or a bounded
  copy-sync-verify-unlink fallback on EXDEV. The cover sidecar's bare rename remains a separate, best-effort mutation.
- **`WritebackConfig`.** `enabled` (default `true`) gates whether `spawn_worker` ever claims a job; when `false`, the
  worker parks on the cancellation token and returns without calling `revert_in_progress`, so rows already `pending` or
  `failed` simply accumulate unclaimed, and a row left `in_progress` by an earlier run stays `in_progress`; only
  enabling and restarting the worker clears it. `concurrency` (default `2`, validated `1`-`10`) sizes the claim
  semaphore. `poll_idle_secs` (default `5`) is the interval between empty-queue polls. `max_attempts` (default `10`,
  validated `≥1`) is the threshold `mark_failed` compares `attempt_count` against to decide between `failed` (eligible
  for another attempt) and `skipped` (exhausted).

## Runtime behaviour

**A manual metadata edit enqueues and completes a writeback job**, for example accepting a proposed title through
`PATCH /api/v1/books/{id}/metadata`:

1. The metadata route's `apply_version` moves the canonical pointer and calls `enqueue_writeback`, inserting a
   `writeback_jobs` row with `reason = 'metadata'`, inside the same `acquire_with_rls` transaction; both commit or roll
   back together.
2. On its next poll tick (or immediately, if idle), `queue::spawn_worker`'s inner loop acquires a semaphore permit and
   calls `claim_next`. The claim `CTE` selects the row (its `NOT EXISTS` clause finds no in-progress sibling), marks it
   `in_progress`, increments `attempt_count`, and returns its id.
3. A `tokio::spawn`ed task calls `orchestrator::run_once` on the writeback pool. `load_snapshot` joins `writeback_jobs`,
   `manifestations`, and `works` for the canonical field values, then runs a second query joining `work_authors` and
   `authors` for the primary author's sort name.
4. For an EPUB, the job opens its typed `LibraryId` and `RelativeFilePath` through `LibraryFiles`. Unknown identities
   fail without fallback; a missing file skips the job.
5. A bounded blocking phase checks the source without repair, applies existing OPF repairs and metadata changes, and
   plans any requested cover embed.
6. The same phase writes and finishes one candidate inside `repack::publish`. Pure candidate validation rejects
   irrecoverable content, increased remaining severity or unresolved repair instructions before candidate publication.
   Successfully applied repair remains separate from degraded findings; equal degraded severity stays admissible.
7. The callback hashes and measures final candidate bytes. The maintained operation syncs, replaces the source basename
   and syncs its actual opened parent. Uncertainty returns a failure before relocation or row-success bookkeeping.
8. Async SQL writes the returned hash, size and cover flag, preserving `ingestion_file_hash`. This content evidence
   remains current if optional relocation fails.
9. A blocking phase renders a checked relative destination in the same library, probes numeric collision suffixes and
   performs a contained move. Async SQL records its actual relative location; a failed path update attempts contained
   move-back under the same claim and permit. A visible relocation with unconfirmed directory durability records its
   location but fails the job.
10. `queue::finish` calls `events::dispatch` with a `Complete` terminal event (delivered as a `tracing::info!` emit and
    recorded in `webhook_event_dedupe`), then `mark_complete` sets `status = 'complete'` and clears `error`.

**A cover-reason job** follows the same shape with one difference at step 5: `reason == "cover"` routes through
`cover_embed::plan_embed` against the pending cover sidecar's bytes, producing binary replacements and/or new `ZIP`
entries that the orchestrator translates to `ZIP`-absolute paths (joining with the `OPF`'s directory) before repack. On
success, `move_cover_sidecar` promotes the sidecar from `_covers/pending/` to `_covers/accepted/` with a bare rename, as
described in the state-writer census; a failure there is logged and does not affect the job's outcome. As Interfaces and
dependencies notes, no enqueue call site reaches this shape in a running system: this paragraph describes what the code
does when a `'cover'`-reason row and a `cover_path` exist, the state this module's own tests construct directly.

### No-overwrite relocation

Numeric suffix probing selects a proposed destination after the accepted-content UPDATE. The final commit enforces
refusal independently of that probe: an occupied file or dangling link is never replaced. Missing destination
directories are created through contained operations, with each new entry's owning parent synced before relocation.

`path_rename` passes actual opened parent directories and individual base names to
`rustix::fs::renameat_with(NOREPLACE)`. Only EINVAL or ENOSYS enables the cap-std hard-link fallback. EEXIST, permission
errors and ambiguous network errors fail without a replacing fallback. EXDEV from the source move enables bounded
copying.

The hard-link path creates the destination, syncs its parent, removes the source, then syncs the source parent. Link or
destination-sync failure retains the source. A failed source removal can leave both names; neither is removed as
compensation for that failure. Successful removal followed by source-parent sync failure returns a visible relocation
with uncertain durability, so the orchestrator records the destination and fails the job.

EXDEV copying creates an owned cap-tempfile directory inside the actual opened destination parent, including a parent on
a nested mount. It copies with a 64 KiB buffer and checks the copied hash. The candidate is synced and materialised at a
fixed basename by `TempFile::replace` only inside that owned directory. Publication to the final name uses the same
no-overwrite helper. An independent hash of the published destination must match before the original source is removed.
Refusal, failed sync, corruption or an unreadable destination preserves the original. Normal completion explicitly
closes the staging directory; cleanup errors propagate or are logged alongside the primary failure.

After a failure leaves the destination visible and the source location recorded, each retry selects a new numeric
suffix, so repeated failures can accumulate duplicate names without automatic reconciliation.

Abrupt exit may leave bare UUID staging directories visible on a NAS share; no cleanup sweep runs. Mounted storage needs
contained access, atomic content replacement, useful sync/error semantics and either no-replace rename or hard links for
relocation. A successful sync after a reported failure does not prove that failed writes became durable. Filesystem and
SQL updates remain separate: an interruption between relocation and the location UPDATE is not automatically reconciled.
Claim reset alone does not close that gap. SQL-failure move-back remains under the same claim and permit, and also
refuses occupied destinations.

**Two workers claim the same manifestation concurrently** (two jobs queued for one manifestation, or the poll interval
overlapping a long-running job's completion):

1. Both transactions' `NOT EXISTS` check runs under `READ COMMITTED`, which cannot see a peer's uncommitted `UPDATE`, so
   both survive the soft filter and both attempt to set their row `in_progress`.
2. The second `UPDATE` to reach the partial unique index blocks on the first transaction's uncommitted index entry, then
   fails with `SQLSTATE 23505` once the first commits.
3. `claim_next` matches `sqlx::Error::Database` where `is_unique_violation()` is true and returns `Ok(None)` for that
   attempt; the losing worker's semaphore permit is dropped and the next poll tick tries again, by which point the first
   job has typically finished and released the `in_progress` slot.

**A crash mid-job, then recovery:**

1. The process is killed while a job is `in_progress` (a `SIGKILL`, a power loss, or any abrupt exit that skips the
   graceful-shutdown path).
2. On the next process startup, `run` builds the writeback pool and calls `spawn_worker`, which calls
   `revert_in_progress` before it begins polling (guarded by `WritebackConfig.enabled`, as noted in Data and state).
   Every row still `in_progress` is set back to `pending` in one `UPDATE`. The revert distinguishes nothing: it treats
   every `in_progress` row as an orphan, which is exact only while one instance runs, the deployment the governing
   decision records. A second instance starting while the first is mid-rewrite reverts the first's live row, and a
   pending sibling for that manifestation then passes the `NOT EXISTS` filter and the partial unique index.
3. The row becomes eligible for `claim_next` again on the worker's first poll, subject to the retry-backoff window for
   its `attempt_count` (Failure and recovery).
4. `queue::finish`'s webhook-before-bookkeeping ordering means the crashed attempt's terminal event, if it reached
   `finish` before the crash, may already be recorded in `webhook_event_dedupe`; the re-run's own terminal event shares
   the same `(job_id, outcome)` id and is suppressed as a duplicate against it within the 48-hour window (Failure and
   recovery).

## Failure and recovery

- **Unsupported format or missing file.** A job whose manifestation's format is not `Epub`, or whose `file_path` does
  not exist on disk, terminates as `RunOutcome::Skipped` before any read or mutation; `queue::finish` routes this
  straight to `mark_skipped`, bypassing the retry budget entirely.
- **`JobNotFound`.** When the `writeback_jobs` row has vanished by the time `run_once` tries to load it (a `CASCADE`
  from a deleted manifestation, or manual row deletion), `load_snapshot` returns `Err(WritebackError::JobNotFound)`.
  `queue::finish` treats this the same as the format/missing-file case: straight to `mark_skipped`, since there is no
  row left to retry against.
- **Candidate rejection or validator/repair error.** The callback fails before replacement. Source bytes and stored
  hash/location remain unchanged; the queue uses its failed/retry path. There is no whole-file snapshot or content
  rollback.
- **Publication uncertainty.** An error after callback acceptance may represent replacement or durability failure. The
  error carries the accepted hash; the job logs the manifestation and error, performs no relocation or row-success
  update, and fails. A retry opens the recorded file afresh to reconcile its evidence.
- **Relocation failure.** Non-EXDEV errors never copy. EXDEV copy, publication, verification or source-removal failure
  retains the source where it still exists, leaves its recorded location unchanged, and logs any possible destination. A
  visibly completed move followed by sync failure records the destination then fails with unconfirmed durability.
- **Retry backoff and exhaustion.** `mark_failed` compares `attempt_count` against `WritebackConfig.max_attempts`
  (default `10`): below the threshold the row becomes `failed` and is retried once its backoff window elapses; at or
  above it the row becomes `skipped`, a terminal exhaustion label logged at `warn!`. The claim `CTE`'s backoff window
  escalates by `attempt_count`: no wait at `0`, `5` minutes at `1`, `30` minutes at `2`, `2` hours at `3`, `8` hours at
  `4`, and `24` hours from `5` onward.
- **A per-job task panic while the process stays alive.** `spawn_worker`'s tracked job body carries no panic guard, and
  `run_once` runs under no per-job timeout. If the spawned task panics, dropping its semaphore permit still frees a
  concurrency slot, but the claimed row itself stays `in_progress`: nothing inside a live process reclaims it. Only a
  process restart (crash or an intentional restart) or a graceful shutdown, which call `revert_in_progress` only after
  live jobs have ended, frees the row again.
- **Content metadata update failure after publication.** If the `UPDATE manifestations` for hash, size and cover flag
  fails, the file has already been rewritten at its recorded location. Relocation does not run, and stored content
  evidence remains stale until a successful retry. `run_once` logs this divergence at `error!` with the attempted hash
  and recorded path, and returns `Err(WritebackError::Db)`, which `queue::finish` routes through `mark_failed` for
  another attempt.
- **Path-rename database update failure.** If `path_rename_step`'s `UPDATE manifestations SET file_path` fails after the
  file has already moved on disk, the step attempts a compensating move back to the original location. If that
  compensating move also fails, the divergence between the on-disk location and the database's `file_path` is logged at
  `error!`; nothing reconciles it automatically afterwards.
- **Terminal-event double dispatch.** `queue::finish` emits the terminal webhook event before the `mark_*` bookkeeping
  `UPDATE`, deliberately: a transient database failure on the bookkeeping write must not silently drop the event. The
  cost is that a bookkeeping failure followed by a crash-recovery re-run re-fires the same `(job_id, outcome)` event;
  `events::dispatch`'s duplicate-suppression check, keyed on that stable id within the 48-hour TTL, absorbs the re-fire.
  The TTL exceeds the maximum claim backoff (24 hours), so this re-fire, bounded by the reclaimed job's own backoff
  window, is always caught by the duplicate-suppression check within its TTL. Duplicate-suppression bookkeeping itself
  is fail-open: a read or write error against `webhook_event_dedupe` is logged and delivery proceeds regardless,
  preserving the same "never silently drop the event" property the emit-before-bookkeeping ordering exists for.
- **Worker disabled with an orphaned row.** As noted in Data and state, `WritebackConfig.enabled = false` skips
  `revert_in_progress` entirely. A row left `in_progress` by a crash that occurred while the worker was last enabled
  stays `in_progress`, blocking every other job for that manifestation behind the partial unique index; only an operator
  re-enabling the worker clears it, on its next startup or next graceful shutdown.

## Security and operations

The writeback pool's connections carry `app.system_context = 'writeback'` for their entire connection lifetime, set once
in `after_connect` rather than per-transaction. The `manifestations_update_system` and `manifestations_select_system`
row-level-security policies (owned by the Design "Row-level security and database context") match only that setting; a
user-facing pool's connections never set it, so a handler that forgets `acquire_with_rls` cannot reach these policies as
a fallback, whatever its role or scope. Without the setting, the orchestrator's `UPDATE manifestations` statements
affect zero rows rather than erroring, which is why `init_pool` (used for `AppState::pool` and
`AppState::ingestion_pool`) must never be handed to this subject's worker; the pool this subject uses is built once in
`backend/src/lib.rs::run`, kept off `AppState`, and passed directly to `spawn_worker`, so no request handler holds a
reference to it.

Two defensive path checks guard against a crafted or malformed input reaching the filesystem, each described as
belt-and-braces given that upstream `EPUB` validation is expected to reject the same shapes first: `orchestrator.rs`'s
`resolve_opf_relative` rejects any `OPF`-relative cover href containing a `..` segment before translating it to a
`ZIP`-absolute path, and `path_rename::normalise_relative` rejects a rendered library path containing a `..` component
or an absolute prefix before any file move is attempted, since a crafted title or author string feeds the path template
that produces that candidate path.

`epub::repack::publish` replaces content beneath the actual opened parent through `atomic_replace_with`. The maintained
crate owns candidate and parent sync. `path_rename::move_existing` opens both parents in the recorded library and syncs
both after rename. Its EXDEV fallback streams and hashes a temporary destination copy, syncs and publishes it,
independently verifies the final destination hash, then removes the source and syncs its parent. Corruption or an
unreadable destination preserves the source and leaves any published destination for investigation. Retry suffix
selection uses capability-relative metadata probes.

The worker stops claiming on cancellation and drains its `JoinSet` within the application's existing shared 30-second
budget. Shutdown recovery resets claims only after all tracked jobs have ended. If the outer drain aborts, unfinished
rows stay `in_progress` for startup recovery; active blocking work retains its semaphore permit across async
cancellation.

This subject is one of the two attachment points for the ingestion-and-writeback row-level-security exemption the Design
"Row-level security and database context" owns generally; the other is the ingestion pool's unconditional policies,
outside this subject's boundary. `writeback_jobs` and `webhook_event_dedupe` are not part of that exemption model at
all, since neither table carries row-level security; what limits access to them is the table grant alone, as described
in the state-writer census.

## More information

- [Configuration reference](../../../../website/src/content/docs/reference/configuration.mdx): the generated
  `REVERIE_WRITEBACK_*` entries for `WritebackConfig`.
- [`backend/schema.sql`](../../../../backend/schema.sql): the `writeback_jobs` table and the `writeback_status` enum.
