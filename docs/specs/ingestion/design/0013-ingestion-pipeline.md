---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0013"
title: "Ingestion pipeline"
satisfies:
  - "REV-REQ-0043"
governed-by:
  - "REV-ADR-0020"
---

# Ingestion pipeline

This Design covers how a file dropped into the watched ingestion directory becomes a manifestation in the library: the
filesystem watcher and its settle window, the one-shot scan that discovers and filters candidate files, the per-file
duplicate check, atomic SHA-256-verified copy into the library tree, the transaction that creates or updates the work
and manifestation rows, quarantine of a file the pipeline cannot ingest, post-batch source cleanup, the Postgres
advisory lock that serialises concurrent scans, and the admin-only HTTP trigger that starts a scan on demand.

## Purpose and boundaries

This subject owns: the filesystem watcher and its settle window (`backend/src/services/ingestion/watcher.rs`); the
one-shot scan orchestration, including the advisory lock, the duplicate check, the per-file state machine, and the
atomic database commit for a newly ingested file (`backend/src/services/ingestion/orchestrator.rs`); format-priority
selection among files that share a directory and stem (`format_filter.rs`); library path template rendering, path
sanitisation, and collision resolution (`path_template.rs`); the atomic, hash-verified copy from the drop zone into the
library (`copier.rs`); quarantine of a file the pipeline rejects (`quarantine.rs`); post-batch source cleanup
(`cleanup.rs`); the `ingestion_jobs` table and its model (`models/ingestion_job.rs`); and the admin-only
`POST /api/v1/ingestion/scan` trigger (`routes/ingestion.rs`).

It does not own EPUB structural validation and repair: this pipeline calls `epub::validate_and_repair` on the file it
has already copied into the library and branches on the returned outcome, but the five-layer check itself, its issue
vocabulary, and its repair behaviour belong to the Design "EPUB validation and repair". It does not own cover extraction
or rasterization: a successful commit fires a best-effort thumbnail pre-warm on the cache the Design "Covers" owns, and
this pipeline neither waits for that work nor inspects its result; this pipeline owns only the call site and the gate
predicate that decide whether that pre-warm fires, not the pre-warm mechanism itself. It does not own the
`works`/`manifestations` schema, the foreign-key graph, or the version-pointer pattern those tables carry, which is the
Design "Works and manifestations data model"; this pipeline is the only production writer of a new `manifestations` row,
but every subsequent change to a row it created belongs to another subject (the Enrichment pipeline subject, the
Writeback pipeline subject, or the Metadata review and editing subject). It does not own row-level security or the
grants on the tables it writes, which is the Design "Row-level security and database context". It does not own OPF
parsing or the metadata-draft journal mechanism (`backend/src/services/metadata/extractor.rs`, `draft.rs`); this
pipeline calls both and commits their output. Neither module is specific to ingestion by its placement in the module
tree, and each has exactly one production caller, this pipeline: `extractor::extract` and `draft::write_drafts`, both
from `orchestrator.rs`.

Depends on: the `validate_and_repair` entry point the Design "EPUB validation and repair" owns, called against the
copied library file for every `epub`-extension candidate; the Works and manifestations data model's
`work::match_existing`, `work::create_stub`, and `work::upgrade_stub`, and the unique constraints on
`manifestations.file_path` and `ingestion_file_hash` as a database-level backstop behind this pipeline's own duplicate
check; the metadata extractor and draft writer that turn a validated `OpfData` (or a heuristic fallback) into the
`ExtractedMetadata` and `metadata_versions` rows this pipeline's transaction points its canonical columns at; a Postgres
session-level advisory lock keyed to a fixed integer id; and the operator-set filesystem paths, format-priority order,
and cleanup mode the Configuration loading subject's `Config` struct carries.

Depended on by: the Design "Covers", whose thumbnail pre-warm this pipeline triggers directly from a successful commit;
the Enrichment pipeline subject, which discovers a newly committed manifestation only because this pipeline leaves
`enrichment_status` at its schema default rather than setting it explicitly; the Writeback pipeline subject, which
reuses this pipeline's path-template renderer and collision resolver when it relocates a file after a metadata-driven
rewrite, and which overwrites the `has_embedded_cover` column this pipeline set at ingestion with its own post-writeback
validation's finding; the Dashboard subject, which reads `ingestion_jobs` directly, grouping `batch_id`, `status`,
`created_at`, and `completed_at` into per-batch activity rows rather than going through this pipeline's own public
surface; and an administrator, through the scan trigger this subject exposes as its only interactive surface.

## Structure

### State-writer census

| State item | Where it lives | Single owner |
| ---------- | -------------- | ------------ |
| `ingestion_jobs` row lifecycle (`queued` to `running` to one terminal state) | `ingestion_jobs` table | `models::ingestion_job::{create, mark_running, mark_complete, mark_skipped, mark_failed}`, called only from the orchestrator's per-file loop |
| Advisory lock id `0x5265_7665_0000_0004` | Postgres session-level advisory lock (not a Reverie table) | `orchestrator::scan_once`; acquired and released around every call |
| A new `manifestations`/`works` row for one ingested file | `manifestations`, `works` tables | `orchestrator::commit_ingest`, the only production site that inserts a `manifestations` row |
| `work_authors` and `authors` rows for a newly created work's extracted creators | `work_authors`, `authors` tables | `work::upgrade_stub` (via `find_or_create_author`), called from `orchestrator::commit_ingest` only when a new work stub is created and OPF metadata names at least one creator |
| Library file at the rendered destination path | Filesystem under `library_path` | Created by `copier::copy_verified`; removed on a subsequent-step failure by four separate sites inside `process_file` |
| Drop-zone source file and its parent directories | Filesystem under `ingestion_path` | Deleted in bulk by `cleanup::cleanup_batch` after a batch with no failures; moved individually by `quarantine::quarantine_file` on the four failure paths that quarantine |

No item above has more than one writer inside this subject. Two of them are shared more broadly. A `manifestations` row
this pipeline creates is mutated afterwards by the Enrichment pipeline subject, the Writeback pipeline subject, and the
Metadata review and editing subject, each owning its own writes to that row; this subject's write authority is limited
to the row's creation and the canonical columns it sets from a validated or heuristic source at that moment. The library
directory tree is similarly shared with the Writeback pipeline subject, which renames a file after a rewrite using the
same `path_template::render` and `resolve_collision` functions this subject defines; the two never write to the same
manifestation's file at the same time, because a manifestation reaches the Writeback pipeline only once this subject's
own commit has completed.

Call sites that dispatch to those owners:

- `ingestion_jobs` writes: `create` and `mark_running` run back-to-back for every file
  `format_filter::select_by_priority` selects, before `process_file` starts; exactly one of `mark_complete`,
  `mark_skipped`, or `mark_failed` runs afterwards, chosen by the returned `ProcessResult`. Each of these five calls
  carries `?`, so a database error on any one of them aborts the whole scan rather than failing only the file in
  progress (see Failure and recovery).
- The advisory lock: acquired once per `scan_once` call, on a connection drawn from the pool the caller supplies, and
  released with an explicit unlock call once the scan's inner work returns, success or handled error alike; a failed
  unlock is logged and not retried.
- `manifestations`/`works` row creation: `commit_ingest` runs exactly once per file that reaches its database step,
  inside one transaction; that transaction is the only place either table is written in production code (every other
  `INSERT` naming either table under `backend/src` is confined to a `#[cfg(test)]` module).
- `work_authors`/`authors` row creation: `work::upgrade_stub` calls `find_or_create_author` once per extracted creator
  and inserts a `work_authors` row per creator (`ON CONFLICT (work_id, author_id, role) DO NOTHING`), but only on the
  branch where `commit_ingest` just created a new work stub and OPF extraction named at least one creator; a matched
  existing work, or an extraction with no creators, writes neither table.
- Library file removal: four distinct sites inside `process_file`, each a best-effort `remove_file` logged at `warn` on
  its own failure rather than propagated: an unsupported extension detected after copy, an EPUB the structural validator
  quarantines, a repaired EPUB whose re-hash fails, and a `commit_ingest` error.
- Source cleanup and quarantine: `cleanup_batch` runs at most once, after the whole per-file loop, gated on zero
  failures in that batch; `quarantine_file` runs per file, at up to four sites (preparation failure, copy failure, an
  EPUB quarantine outcome, and a repaired EPUB whose re-hash fails), moving only that one file immediately rather than
  waiting for the batch to finish.

### Component relationships

- `orchestrator.rs` wires every other module together. `run_watcher` spawns `watcher::watch` as a background task and,
  for every settled batch it receives, calls `scan_once` without reading the batch's own paths; an inline comment in the
  `rx.recv()` arm records the reason, that a full directory walk picks up a file that arrived after the watcher's own
  event fired. `routes/ingestion.rs::scan` is the only other production caller of `scan_once`.
- `scan_once` is a thin wrapper: it acquires the advisory lock, calls `scan_once_inner`, and releases the lock
  regardless of the inner call's outcome.
- `scan_once_inner` walks the ingestion directory with `WalkDir` (symbolic links not followed), narrows the result to
  one file per directory-and-stem group through `format_filter::select_by_priority`, drives each selected file through
  `ingestion_job` and `process_file`, and finishes with the batch-level `cleanup::cleanup_batch` when the batch has no
  failures.
- `process_file` is the per-file pipeline. `path_template` (a filename heuristic, template rendering, and collision
  resolution) and `copier` (hashing, then a hash-verified copy) run first; the duplicate check against `manifestations`
  sits between the hash and the copy; `epub::validate_and_repair`, owned by the Design "EPUB validation and repair",
  runs only for an `epub` extension, against the file this pipeline has already copied into the library, not the
  drop-zone original; the metadata extractor turns a validated `OpfData` into `ExtractedMetadata`, which can trigger a
  second path-template render and an in-place rename when the metadata-derived path disagrees with the filename
  heuristic; `commit_ingest` performs the database write; and `quarantine_file` is the exit for the failure classes
  described in the state-writer census above.
- `covers::spawn_warm_thumb`, owned by the Design "Covers", is fired without being awaited after a successful
  `commit_ingest`, keyed on the copy's verified hash, a hand-off this pipeline does not follow further.

## Interfaces and dependencies

- The module's public surface (`backend/src/services/ingestion/mod.rs`): `ScanResult { processed, failed, skipped }`,
  `run_watcher(config: Config, pool: PgPool, cancel: CancellationToken) -> Result<(), anyhow::Error>`, and
  `scan_once(config: &Config, pool: &PgPool) -> Result<ScanResult, anyhow::Error>`. Every other item the child modules
  export (`copier::{hash_file, copy_verified}`, `quarantine::quarantine_file`, `cleanup::cleanup_batch`, and
  `format_filter::select_by_priority`) has no caller outside this module in production code; `path_template::render` and
  `path_template::resolve_collision` are the one exception, reused by the Writeback pipeline subject.
- `run_watcher` is spawned exactly once, at startup (`crate::run` in `backend/src/lib.rs`), sharing the process-wide
  shutdown `CancellationToken` and the same drain budget as the other background workers (see Failure and recovery).
- `scan_once` has two production callers: `run_watcher`'s per-batch trigger, and `POST /api/v1/ingestion/scan`
  (`routes/ingestion.rs::scan`), gated by `CurrentUser::require_scope(Scope::Admin)` and `require_admin`. The route's
  response (`ScanResponse`: `processed`, `failed`, `skipped`) carries no batch identifier, so nothing outside this
  pipeline's own logs can correlate a scan with the `ingestion_jobs` rows it wrote.
- `models::ingestion_job` exports `create`, `mark_running`, `mark_complete`, `mark_skipped`, `mark_failed`, and
  `find_by_batch`; every one of them is called only from this pipeline's own orchestrator or its own test module.
  `find_by_batch` specifically has no caller anywhere outside `models/ingestion_job.rs`'s own tests.
- The Postgres role this pipeline connects as is `reverie_ingestion`, which carries an unconditional row-level-security
  policy (`USING (true) WITH CHECK (true)`) on `manifestations`; the other tables its commit touches (`works`,
  `work_authors`, `authors`, `metadata_versions`) have no row-level security and grant both roles the same access. When
  `DATABASE_URL_INGESTION` is unset the pipeline connects as `reverie_app` instead (see Security and operations).

## Data and state

- **`ingestion_jobs`.** One row per file a scan selects, keyed by `id`, grouped by `batch_id` (one value per scan).
  `status` is a Postgres `job_status` enum (`queued`, `running`, `complete`, `failed`, `skipped`); `error_message` is
  populated only on `failed`; `started_at` and `completed_at` mark the `running` and terminal transitions respectively.
  The table carries no row-level security; `reverie_app` and `reverie_ingestion` hold full DML, `reverie_readonly` holds
  `SELECT`.
- **`manifestations.ingestion_status`.** A separate Postgres enum (`pending`, `processing`, `complete`, `failed`,
  `skipped`) applied to the manifestation row itself, distinct from `ingestion_jobs.status`. This pipeline writes only
  `complete`, on the row's initial insert; no production code path under `backend/src` writes any of the other four
  values to this column, so a manifestation row that exists at all always reads `complete` here regardless of how its
  own ingestion went.
- **The advisory lock id.** A fixed 64-bit constant, acquired and released with `pg_advisory_lock`/`pg_advisory_unlock`
  around every `scan_once` call. Session-level, not transaction-level: it outlives the individual transactions
  `commit_ingest` opens and closes during the scan it guards.
- **The library file-identity trio.** `file_path` (the rendered, sanitised, collision-resolved destination, relative to
  `library_path`), `ingestion_file_hash` (the source `SHA-256`, computed once and reused for both the duplicate check
  and the copy's own integrity verification), and `current_file_hash` (set equal to `ingestion_file_hash` at this
  pipeline's insert; a different value belongs to the Writeback pipeline subject). Both `file_path` and
  `ingestion_file_hash` carry a `UNIQUE` constraint at the database, a backstop behind this pipeline's own pre-copy
  duplicate check.
- **`has_embedded_cover`.** Set from the EPUB structural validator's cover finding for an `epub` extension; `NULL` for
  every non-`epub` format (no validator exists to check it), for a manifestation ingested before this column existed,
  and for a manifestation whose validator failed to run. The dashboard's cover-coverage metric and the Design "Covers"
  both treat `NULL` the same as a checked-and-absent cover. The Writeback pipeline subject overwrites this column from
  its own post-writeback EPUB validation after every writeback run whose validation produces a value
  (`UPDATE manifestations SET ... has_embedded_cover = COALESCE($3, has_embedded_cover)`), including a row that carries
  `NULL`; only a run whose post-writeback validation itself errors leaves the existing value untouched.
- **The quarantine sidecar.** A `<filename>.quarantine.json` file written beside every quarantined file, recording the
  original drop-zone path, a free-text reason, and an RFC 3339 timestamp. Neither the quarantined file nor its sidecar
  is ever removed by this pipeline; the quarantine directory accumulates, and only an operator manually clearing it
  reduces it.

## Runtime behaviour

**Discovering and selecting candidates.** A scan (`scan_once`) begins by walking the whole ingestion directory with
`WalkDir`, not following symbolic links, collecting every regular file regardless of extension.
`format_filter::select_by_priority` groups the results by parent directory and lowercase filename stem, and, within each
group, keeps only the file whose extension both parses as a `ManifestationFormat` and ranks earliest in the
operator-configured priority order; a file with no parseable extension, or one that loses to a higher-priority sibling
in its group, is dropped from the selection silently: it receives no `ingestion_jobs` row and is not counted in the
scan's `processed`, `failed`, or `skipped` totals.

**Per file, in the order below** (`process_file`), once `ingestion_job::create` and `mark_running` have written that
file's row:

1. A filename heuristic (`Author - Title.ext`) supplies path-template variables; the template renders to a relative
   library path, and `resolve_collision` appends a numeric suffix, `(N)` with a leading space and parentheses before the
   extension, if a file already sits at that exact path on disk. The source file is hashed (`SHA-256`) in the same step.
2. A single query checks `manifestations` for an existing row whose `ingestion_file_hash` matches the freshly computed
   hash, or whose `file_path` matches the rendered destination from step 1. Either match skips the file: no copy, no
   database write for it. The path half can only match when nothing occupies that exact path on disk (step 1's collision
   check already ran against the filesystem, not the database), so it fires only when a manifestation's recorded file is
   absent from the library directory while its row still exists. The hash half is the only one available for a
   manifestation whose stored path was itself the product of a metadata-driven rename, because this comparison always
   uses the heuristic path from step 1, never a metadata-derived path a subsequent step in this same run might compute.
3. The file is copied into the library with `copier::copy_verified`: written to a temporary file in the destination
   directory, hashed inline during the write, and renamed into place only if the inline hash matches the value from step
   1; a mismatch leaves the temporary file to drop automatically, so no partial file appears at the destination path (an
   already created empty parent directory, however, is not itself removed on this path).
4. The extension is re-parsed into a `ManifestationFormat`, a defence against a bypass of step 1's format-priority
   selection; on failure, the just-copied library file is removed and the file fails without quarantine (see Failure and
   recovery).
5. For an `epub` extension only, the copied library file, not the drop-zone original, is handed to
   `epub::validate_and_repair`, owned by the Design "EPUB validation and repair". A `Quarantined` outcome removes the
   library file and moves the drop-zone original to quarantine with a sidecar; `Repaired` and `Degraded` outcomes carry
   accessibility metadata and parsed `OPF` data alongside their status. A `Repaired` outcome also re-reads the rewritten
   library file for its hash and size, so `current_file_hash` and `file_size_bytes` describe the repaired bytes while
   the ingestion hash keeps the original's; a failure to re-read it removes the library file and quarantines the
   drop-zone original like a `Quarantined` outcome. A validator that fails to run (an I/O or internal error, not a
   structural finding) stores `validation_status = failed` and still proceeds to commit, so the file is ingested and
   served, with the failure surfaced only as a status value an operator can watch. A non-`epub` format leaves
   `validation_status` at its `pending` default, since no validator exists for it.
6. Any `OpfData` recovered in step 5 is extracted into `ExtractedMetadata`. If the extracted title or an author differs
   from the filename heuristic enough to render a different library path, the file is renamed on disk (with its own
   collision resolution) to the metadata-derived path; a rename failure is logged, and the heuristic path is kept rather
   than failing the file at this step.
7. `commit_ingest` runs the database write in one transaction: match an existing work by the extracted metadata, or
   create a placeholder work; insert the `manifestations` row (canonical fields still `NULL`, `ingestion_status` set to
   `complete`); write `metadata_versions` draft rows from the extracted metadata, or a synthetic low-confidence draft
   built from the filename heuristic when no `OPF` metadata was recovered; on a newly created work, populate its
   canonical columns and version pointers from those drafts; then update the manifestation's ISBN, publisher,
   publication date, and page-count columns and their own version pointers from the extracted metadata. The whole
   sequence commits together or not at all.
8. On a successful commit, and only for an `epub` extension whose validator did not positively rule out a usable
   embedded cover, a cover thumbnail pre-warm is fired on the cache the Design "Covers" owns, keyed by the manifestation
   id and the copy's verified hash; this pipeline does not wait for it.

**Cleanup, once every selected file in the batch has reached a terminal outcome.** If no file in the batch failed, and
the operator-configured cleanup mode is not `none`, the drop-zone files are deleted: every file the initial walk found,
under the `all` mode, or every file the format-priority selection chose, under the `ingested` mode, which still includes
a file the duplicate check skipped, since the selection list predates that check. Any file failing in a batch leaves
every source file in the batch untouched, selected or not, so a subsequent scan attempt can re-process the files that
did not fail alongside the ones that did.

## Failure and recovery

An irrecoverable EPUB never reaches step 7: the `Quarantined` outcome from step 5 removes the copied library file, moves
the drop-zone original to quarantine with its sidecar, and returns before `commit_ingest` runs, so no `manifestations`
row is created for it.

- **Preparation, copy, EPUB-quarantine, or post-repair re-hash failure.** These four failure classes move the drop-zone
  source file to quarantine with a JSON sidecar recording the failure reason and a timestamp; a filename collision
  inside the quarantine directory appends a Unix-timestamp suffix rather than overwriting. The fourth arises when the
  library file cannot be re-read to compute its post-repair hash; the just-repaired library file is removed as well.
  Every other failure class below leaves the source file exactly where the walk found it.
- **A duplicate-check query failure.** Treated as a failure of the file, not a silent pass-through: the pipeline does
  not proceed to copy a file whose duplicate status it could not determine, so a transient database error cannot disable
  deduplication for that file. The source is left in place, not quarantined.
- **An unsupported-format failure after copy.** Removes the just-copied library file and fails the file without
  quarantine; a bypass of the format-priority selection is the only way this path is reached, since every file that
  selection admits already carries a parseable extension.
- **A database commit failure.** `commit_ingest` returning an error removes the just-copied library file (a best-effort
  `remove_file`, logged rather than propagated on its own failure) and fails the file; the source is neither quarantined
  nor deleted, so it remains a candidate for a subsequent scan, which recomputes its hash and re-attempts it from the
  start, since no `manifestations` row was ever committed for it. This handled-error path is distinct from an abrupt
  termination of the process itself: nothing in this pipeline detects or compensates for a kill between a successful
  copy and a transaction that never gets the chance to commit or roll back; the transaction's own atomicity, not this
  pipeline's own error handling, decides what such a kill leaves behind.
- **An `ingestion_jobs` write failure.** Every call into that model carries `?`, so a database error writing a job's
  `queued`, `running`, or terminal state propagates out of the whole scan immediately: the scan stops, files already
  committed in that scan stay committed, files the loop had not reached receive no job row, and cleanup for the batch
  does not run. This is a stricter failure mode than a per-file failure; one job-row write failure ends the batch, not
  just the file it was recording.
- **A stuck job row.** An `ingestion_jobs` row left at `running`, by a scan that aborts mid-file or by an aborted
  process, is never reverted, retried, or otherwise reclaimed by this pipeline. A subsequent scan's own duplicate and
  processing decisions are driven entirely by `manifestations` and never consult `ingestion_jobs.status`, so a stuck row
  does not change what a subsequent scan does. The Dashboard subject reads `ingestion_jobs` directly, though: it groups
  jobs by `batch_id` and counts them by `status`, and nulls a batch's `ended_at` while any of its jobs sit at `queued`
  or `running`, so a stuck `running` row keeps that batch showing as in progress with no end time, indefinitely.
- **Shutdown mid-scan.** The filesystem watcher and every other background worker share one 30-second drain budget after
  the HTTP server stops; the watcher's own event loop checks the shutdown signal only between batches, not inside a
  `scan_once` call already under way, so a scan in progress keeps running to completion or to the shared budget's
  expiry, whichever comes first. A worker still running past that budget is aborted at its next suspension point, which
  can land inside a file copy or an open `commit_ingest` transaction; an uncommitted transaction contributes nothing on
  abort, consistent with the atomic-commit sequence described above, but a copy already renamed into place before that
  point is not itself rolled back.
- **Quarantine and its sidecars.** Nothing in this pipeline removes a quarantined file or its sidecar once written; the
  quarantine directory grows without bound, and only manual operator intervention reduces it, the same shape as the
  compensating control the CodeGuard deviation register records for this pipeline's cleanup containment guard (see
  Security and operations).
- **Cleanup's containment guard.** `cleanup_batch` resolves both the configured ingestion root and every path it is
  asked to touch to their real paths before deleting or pruning it, and refuses anything that does not resolve as a
  descendant of that root, including a path reached through a symlink, or a sibling directory whose name merely extends
  the root's as a string. A failure to resolve the root's own real path is treated as the least permissive outcome
  available (the guard compares against the unresolved root instead), which can only cause a legitimate in-tree path to
  be skipped, never an out-of-tree one to be admitted.

## Security and operations

- **Path sanitisation is this pipeline's traversal defence for user-supplied metadata.** Every path-template
  substitution passes through `sanitize_path_component`, which replaces the characters forbidden on POSIX, Windows NTFS,
  or common network file systems (including `/` and `\`) with `_`, trims leading and trailing whitespace and dots, and
  collapses repeated underscores; because `/` and `\` cannot survive substitution, a metadata field containing a
  directory-traversal sequence cannot escape the library root once its sanitised value is joined to an absolute base.
- **Cleanup's containment guard is a CodeGuard compensating control**, recorded against this pipeline in the CodeGuard
  deviation register's ZIP-processing entry: bounding `cleanup_batch`'s deletion and pruning authority to descendants of
  the resolved ingestion root, verified against both an out-of-tree symlink target and a sibling directory whose name
  shares a string prefix with the root without being a descendant of it.
- **Only `reverie_ingestion` holds unconditional, not per-user, access to `manifestations`.** That role's
  `manifestations_ingestion_full_access` policy (`USING (true) WITH CHECK (true)`) carries no per-request credential or
  caller-supplied role narrowing it; every write this pipeline makes over its dedicated connection reaches the table
  unconditionally. This is a deliberate contrast with the per-user policies the Design "Row-level security and database
  context" applies to the same table for an ordinary request. The other tables this pipeline's commit touches, `works`,
  `work_authors`, and `metadata_versions`, carry no row-level security at all: `reverie_app` and `reverie_ingestion`
  hold identical plain grants on each (`SELECT, INSERT, DELETE, UPDATE`), so access to them does not differ by role.
- **The DSN fallback breaks ingestion writes rather than merely widening who can make them.** A blank
  `DATABASE_URL_INGESTION` clones `database_url` into `ingestion_database_url` at configuration load and sets a flag
  this pipeline's caller (`crate::run`) turns into a startup warning; the server still starts, and the pipeline connects
  as `reverie_app` instead of `reverie_ingestion`. `reverie_app` carries `LOGIN` only, with no `BYPASSRLS` attribute,
  and its `manifestations_insert` policy's `WITH CHECK` clause requires `app.current_user_id` to resolve, through a join
  to `users`, to a user holding the `admin` or `adult` role; this pipeline sets neither `app.current_user_id` nor
  `app.system_context` on any connection it opens. Every manifestation insert on the fallback path is therefore refused
  by row-level security: `commit_ingest` returns an error, the just-copied library file is removed, and the file's job
  is marked failed. The startup warning is the only signal that ingestion is failing this way.
- **The admin gate is the only access control this subject's HTTP entry point applies for itself.** `scan` calls
  `CurrentUser::require_scope(Scope::Admin)` and `require_admin` before doing any work; like any other
  session-authenticated mutating request, a caller presenting a session cookie also needs a valid CSRF token to reach
  the handler at all, a gate the Design "CSRF protection" owns, not this subject.
- **`ingestion_jobs` carries no row-level security of its own.** `reverie_app` and `reverie_ingestion` hold full DML on
  the table; `reverie_readonly` holds `SELECT`. Any caller with a valid credential for one of the first two roles can,
  in principle, write a job row directly; only the admin gate on `scan` and this pipeline's own exclusive write sites
  decide in practice what actually reaches the table.
- **Nothing in this pipeline bounds a source file's size before it is hashed and copied.** The EPUB validator's own
  aggregate and per-entry size caps apply only after copying, against the library file, and belong to the Design "EPUB
  validation and repair"; this pipeline hashes and copies whatever bytes the drop zone holds.

## More information

- [CodeGuard deviation register](../../../security/codeguard/README.md), deviation 4: the compensating controls recorded
  for EPUB ingestion's use of ZIP archives, including `cleanup_batch`'s containment guard.
- [Technical debt register](../../../../debt/README.md): the pre-migration gap in `has_embedded_cover` this Design's
  Data and state section describes, where no data already stored can reconstruct the flag for a manifestation ingested
  before the column existed.
