---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0016"
title: "Enrichment pipeline"
satisfies:
  - "REV-REQ-0051"
  - "REV-REQ-0052"
governed-by:
  - "REV-ADR-0007"
  - "REV-ADR-0018"
---

# Enrichment pipeline

This Design covers the background mechanism that fills a manifestation's bibliographic metadata from external providers:
the enrichment queue embedded on `manifestations`, the per-manifestation fan-out across Open Library, Google Books and
Hardcover, the field-level merge policy that decides whether an observation applies, stages or is discarded, the
`metadata_versions` journal, the canonical-column apply through version-id pointers, the ISBN-triggered work rematch,
the same-transaction writeback enqueue, and the three control endpoints including the dry run.

## Purpose and boundaries

This subject owns everything from "a manifestation is due for enrichment" to "a canonical field either changed or a
candidate is waiting for review": the row-embedded queue state machine on `manifestations` and its worker loop
(`backend/src/services/enrichment/queue.rs`), the per-manifestation orchestrator (`orchestrator.rs`: `run_once`,
`load_snapshot`, `derive_lookup_keys`, `fan_out_with_fallback`, `apply_journal_batch`, `apply_canonical_batch`,
`apply_field`), the field-level policy engine (`policy.rs`), the field-lock CRUD module the policy engine and the
manual-edit routes both call (`field_lock.rs`), confidence scoring (`confidence.rs`), lookup-key derivation and
canonical-JSON value hashing (`lookup_key.rs`, `value_hash.rs`), the `api_cache` read/write helper (`cache.rs`), the
SSRF-guarded HTTP client factory (`http.rs`), the three provider adapters (`sources/open_library.rs`,
`sources/google_books.rs`, `sources/hardcover.rs`, and the shared `sources/mod.rs` contract), the dry-run preview
(`dry_run.rs`), and the three control endpoints (`backend/src/routes/enrichment.rs`: trigger, dry-run, status). It also
owns `EnrichmentConfig` (`backend/src/config/enrichment.rs`), the worker's runtime knobs.

It does not own the background flush of an applied canonical value into the on-disk `EPUB` file: that is the Design
"Writeback pipeline"; this subject only enqueues a `writeback_jobs` row in the same transaction as the canonical apply
and never touches a file. It does not own the review queue, the routes that accept, reject or revert it, the manual
`PATCH` surface, or the HTTP `lock`/`unlock` endpoints: that is the Design "Metadata review and editing"
(`backend/src/routes/metadata.rs`); this subject's `field_lock.rs` supplies the lock helpers those endpoints call, but
the endpoints themselves, and every manual write to `metadata_versions.status`, `resolved_at` and `resolved_by`, belong
there. It does not own the two-level external-identifier registry or the per-provider rating cache: their storage model,
validation and cross-surface visibility filtering (`backend/src/models/external_identifier.rs`,
`backend/src/models/external_rating.rs`); this subject is one of two fillers of that registry (a manual edit through the
metadata routes is the other) and calls its `get_*`/`upsert_*` functions without describing their internals. It does not
own the FRBR data model, `works`/`manifestations` themselves, or the ISBN-triggered rematch algorithm
(`backend/src/models/work.rs::rematch_on_isbn_change`); this subject calls that one hook and reacts to its outcome. It
does not own the EPUB-ingestion pipeline's metadata extraction or its `metadata_versions` writes
(`backend/src/services/metadata/draft.rs::write_drafts`, called from `backend/src/services/ingestion/orchestrator.rs` at
ingestion time with `source = 'opf'`); those rows land in the same table this subject's policy engine reads, so an
OPF-sourced pending row can itself be the row an enrichment observation is checked against for agreement or disagreement
in a subsequent run (see State-writer census). It does not own `content_rating` as a value type
(`backend/src/models/content_rating.rs`) or its child-safety consumption; this subject only pins the field's merge
policy to `Propose` and never writes the column itself. The cover-download and staging module (`cover_download.rs`) has
no caller outside its own test module, and `http.rs`'s `cover_client` factory is constructed only there. Provider cover
URLs are recorded as `cover_url` observations like any other field; nothing in the production path fetches, downloads or
stages the image such an observation points to.

Depends on: the row-level-security mechanism and the ingestion role's unconditional policies the queue and the dry-run's
fan-out rely on to reach `manifestations` without a per-user session context (the Design "Row-level security and
database context"); `crate::models::work::rematch_on_isbn_change` for the ISBN-change hook; the external-identifier and
rating registry's model functions for reading and writing identifier and rating slots; `crate::services::metadata::isbn`
and `crate::services::metadata::external_id` for ISBN parsing and canonical identifier-field addressing.

Depended on by: the Design "Writeback pipeline", which drains the `writeback_jobs` rows this subject enqueues; the
Design "Metadata review and editing", which reads and resolves the `metadata_versions` rows this subject journals and
calls `field_lock.rs`'s write functions from its own HTTP endpoints; the identifier and rating registry's other filler
(a manual edit) and its readers; and the book-detail and library surfaces that display `enrichment_status` and pending
metadata counts.

## Structure

### State-writer census

- `manifestations.enrichment_status`, `_attempt_count`, `_attempted_at`, `_error`, `_rerun_requested`: written by
  `queue.rs`'s `claim_next`/`mark_complete`/`mark_failed`/`revert_in_progress` (the queue's own lifecycle), plus two
  external re-queue writers sharing the identical CASE-guarded `UPDATE` shape: `routes/enrichment.rs::trigger` and
  `routes/metadata.rs::apply_identifier_patches` (a manual identifier edit implicitly re-queues enrichment).
- `metadata_versions` rows have three writer classes. This subject's own automated observations: written by
  `orchestrator::upsert_journal_row` (insert, or an `observation_count` bump on conflict) and by
  `apply_canonical_batch`'s own `confidence_score` update. A second writer with no overlap in code path: ingestion-time
  OPF extraction, `backend/src/services/metadata/draft.rs::write_drafts` (called from
  `backend/src/services/ingestion/orchestrator.rs`), inserts `source = 'opf'` rows through the same upsert shape;
  `orchestrator::load_existing_pending`'s disagreement read
  (`WHERE manifestation_id = $1 AND field_name = $2 AND status = 'pending'`) carries no `source` filter, so an OPF row
  from ingestion can be the pending row an enrichment observation is checked against for agreement or disagreement.
  Manual-source rows and every `status`/`resolved_at`/`resolved_by` transition are the third class, and belong to the
  Design "Metadata review and editing".
- Canonical `works`/`manifestations` columns and their `*_version_id` pointers (`title`, `description`, `language`,
  `subtitle`, `publisher`, `pub_date`, `isbn_10`, `isbn_13`, `pages`, `identifiers.*`): written by
  `orchestrator::apply_field`, gated by `policy::decide`. `content_rating` and `cover` are never written by this path
  (see Runtime behaviour).
- `writeback_jobs` rows from an automated apply: written by `orchestrator::enqueue_writeback` (`INSERT` only, same
  transaction as the pointer move). A second, independent `enqueue_writeback` function in `routes/metadata.rs` writes
  rows from the manual-edit path and belongs to the Design "Metadata review and editing".
- `api_cache` rows: written by `cache::write`, called only from `orchestrator::cache_all`, itself called from both the
  production run and the dry-run's fan-out.
- `field_locks` rows: the write side is `field_lock::lock_tx`/`unlock_tx`, called only from `routes/metadata.rs`'s
  `lock_field`/`unlock_field` (the Design "Metadata review and editing"). This subject only reads them, through
  `field_lock::is_locked`/`is_locked_tx`.

Every item above other than `metadata_versions` has exactly one writer once the manual-edit paths are attributed to
their owning Design; the two identical re-queue call sites for `enrichment_rerun_requested` are a deliberate pair (one
HTTP-triggered, one edit-triggered), not an accidental second writer of the same intent.

### Component relationships

- `queue.rs` is the worker: `spawn_queue` runs a `tokio::select!` loop between a cancellation token and a poll interval,
  draining as many `pending`/`failed` rows as a `Semaphore`-gated concurrency limit allows on each tick via
  `claim_next`, spawning one task per claim that calls `orchestrator::run_once` and then `finish` for bookkeeping.
  `claim_next` is one `FOR UPDATE SKIP LOCKED` CTE with the retry-backoff window folded into its `WHERE` clause as a
  `CASE` expression (5m, 30m, 2h, 8h, then 24h): the sole, authoritative copy of that schedule. `revert_in_progress`
  runs only when `cancel` fires.
- `orchestrator.rs` is the per-manifestation flow. `load_snapshot` reads the current canonical state, the active
  identifier-registry slots and the `metadata_sources.base_priority` values, and derives the ordered `lookup_keys` list
  via `derive_lookup_keys`. `fan_out_with_fallback` tries each key in order under one shared wall-clock budget,
  delegating to `fan_out` (a `FuturesUnordered` join with a `sleep` deadline) and to `cache_all` after each attempt.
  `run_once` then opens one transaction: `apply_journal_batch` upserts a `metadata_versions` row per field observation
  and buckets rows by field; `apply_canonical_batch` locks the manifestation row, then per field locks the work row when
  the field needs a work-level identifier re-read, computes confidence, calls `policy::decide`, and on `Decision::Apply`
  calls `apply_field`, the ISBN rematch hook and `enqueue_writeback`; `upsert_ratings` routes rating signals straight to
  the ratings cache outside the journal.
- `policy.rs` is a pure decision function: `default_policy(field)` maps a field name to `AutoFill`, `Propose` or `Lock`,
  and `decide(...)` applies the lock check first, then downgrades `AutoFill` to `Propose` on disagreement with any
  pending observation (from an earlier run or from another source in the same run), then dispatches on emptiness.
- `field_lock.rs` provides connection-based `lock_tx`/`unlock_tx`, called within the metadata endpoints'
  visibility-checked transactions. `is_locked`/`is_locked_tx` read locks for `dry_run::preview` and
  `apply_canonical_batch` respectively.
- `confidence.rs`, `lookup_key.rs` and `value_hash.rs` are small pure helpers: a source/match-type/quorum scoring
  formula, ISBN and title/author key normalisation (so ISBN-10 and ISBN-13 of the same book, or title strings that
  differ only in case, whitespace or punctuation, converge on one cache key), and a canonical-JSON `SHA-256` hash that
  ignores list-item order for the vocabulary and contributor fields and normalises `pub_date`/`publisher` whitespace.
- `cache.rs` is a thin `api_cache` read/write helper with a Rust-computed per-kind (`Hit`/`Miss`/`Error`) expiry.
- `http.rs` builds two `SSRF`-guarded `reqwest::Client` factories through one shared constructor (`ssrf_client`):
  `api_client` (fixed 5-hop redirect limit, 10-second timeout; the one this subject's fan-out uses) and `cover_client`
  (the same construction with a caller-supplied redirect limit and timeout, reachable only from `cover_download.rs`'s
  own test module). Both share one process-wide custom DNS resolver that drops any resolved address in a denied range
  before `reqwest` ever dials it, and both re-validate every redirect hop against the same denied ranges via
  `validate_hop` before following it.
- `sources/mod.rs` defines the `MetadataSource` trait, `LookupKey`/`LookupOutcome`/`SourceResult`/`SourceError`, and
  `RatingSignal`, the four-way signal (`Unknown`/`Absent`/`Reported`/`Unusable`) that keeps a lookup that says nothing
  about ratings from erasing a previously cached one. Each of the three adapters is a uniform translation shim:

  | Provider | `id()` | Enabled when | Rate limit | Protocol / auth |
  | -------- | ------ | ------------ | ---------- | --------------- |
  | Open Library | `openlibrary` | Always | 3/s | REST JSON, no key |
  | Google Books | `googlebooks` | Always | 1/s | REST JSON, optional `api_key` query parameter |
  | Hardcover | `hardcover` | `hardcover_api_token` configured | 1/s | GraphQL over `POST`, bearer token |

  Each adapter's base URL is a Rust constant (`open_library::DEFAULT_BASE_URL`, `google_books::DEFAULT_BASE_URL`,
  `hardcover::DEFAULT_BASE_URL`); `orchestrator::build_sources` constructs every adapter from its own constant, and no
  persisted setting or environment value overrides it. `openlibrary` and `googlebooks` seed at the same
  `metadata_sources.base_priority` (100); `hardcover` seeds lower (90), so a native-id lookup prefers the first two when
  more than one identifier is available, even though `confidence.rs` weights a Hardcover observation's
  *field confidence* higher (0.85) than either (0.80, 0.75) once a value is actually fetched. The two weightings answer
  different questions and are not meant to agree.
- `dry_run.rs`'s `preview` reuses `orchestrator::fan_out_for_dry_run` (the same snapshot load and fan-out, including
  `api_cache` writes) and replays `policy::decide` per field with no journal, canonical or `writeback_jobs` write.
- `routes/enrichment.rs` mounts `trigger`, `dry_run` and `status` on `AppState`.

## Interfaces and dependencies

- `POST /api/v1/manifestations/{id}/enrichment/trigger`: requires write scope and a non-child caller
  (`CurrentUser::require_scope(Scope::Write)`, `require_not_child`); opens `db::acquire_with_rls` on `state.pool` before
  its one `UPDATE`, so a manifestation the caller cannot see resolves to `AppError::NotFound` rather than leaking
  existence. Re-queues an idle row to `pending` immediately; a row already `in_progress` keeps its status and only sets
  `enrichment_rerun_requested`.
- `POST /api/v1/manifestations/{id}/enrichment/dry-run`: requires a non-child caller and declares read scope. Opens a
  short `acquire_with_rls` transaction on `state.pool` purely to check `SELECT id FROM manifestations WHERE id = $1`
  (`AppError::NotFound` on a miss), drops that transaction, then calls `dry_run::preview` on `state.ingestion_pool`, the
  same pool the queue itself runs on, chosen because the preview reads several joined tables without re-checking
  row-level security at each step. Returns `DryRunDiff` (`would_apply`/`would_stage`/`locked`/`source_failures`).
- `GET /api/v1/enrichment/status`: requires a non-child caller and read scope; aggregates
  `manifestations.enrichment_status` counts under `acquire_with_rls`, so a child sees counts scoped to its own visible
  manifestations like any other row-level-security-gated read.
- `crate::models::work::rematch_on_isbn_change(tx, manifestation_id)`: called from `apply_canonical_batch` inside the
  same transaction whenever an `isbn_10` or `isbn_13` apply succeeds; its `RematchOutcome` (`NoOp`/`AutoMerged`/
  `Suspected`) is logged but not otherwise acted on here.
- `crate::models::external_identifier`'s `get_work_identifier`/`get_manifestation_identifier`/`upsert_work_identifier`/
  `upsert_manifestation_identifier`, and `crate::models::external_rating`'s `upsert_rating`/`delete_rating`: the
  registry's model functions this subject fills through on an identifier or rating observation.
- `crate::services::metadata::external_id::parse_canonical_field`/`parse_external_id`: the typed gate a provider's
  identifier string passes through before it reaches the registry; the database `CHECK` is only a backstop.
- The website guide `website/src/content/docs/guides/external-identifiers.md` narrates the native-id lookup and
  identifier-editing behaviour from a user's perspective; this Design links it rather than restating its prose.

## Data and state

- `manifestations.enrichment_status` is a five-value enum (`Pending`, `InProgress`, `Complete`, `Failed`, `Skipped`),
  backed by the partial index `idx_manifestations_enrichment_queue` that `claim_next` reads (on `enrichment_status` and
  `enrichment_attempted_at`, `NULLS FIRST`). `Skipped` is terminal: `claim_next`'s eligibility check only ever selects
  `pending` or `failed` rows, so only an external re-queue write (`trigger`, or a manual identifier edit) can return a
  skipped row to circulation.
- `metadata_versions.status` is a two-value enum, `pending` or `rejected`; there is no `applied` value. An automated
  apply is represented purely by a `*_version_id` column on `works`/`manifestations` pointing at the journal row's `id`,
  never by a status transition: the row that supplied the canonical value stays `pending` alongside any sibling
  observation from another source that agreed or disagreed. A four-column unique constraint (manifestation, source,
  field name, value hash) is what lets a repeated identical observation bump `observation_count` instead of duplicating
  a row.
- `api_cache` rows carry a per-kind expiry (`hit`/`miss`/`error`, each with its own configured `TTL`) computed in Rust
  at write time. `cache::read` exists and is exercised by its own tests, but every production call into
  `MetadataSource::lookup` passes `LookupCtx { cached: None, .. }` (`orchestrator::fan_out`, the only production
  construction site): nothing in `run_once` or the dry-run ever consults a cache hit before dispatching a live provider
  call. The table is a write-only response log from the pipeline's own perspective, even though the `LookupCtx::cached`
  contract exists to let an adapter serve a cached payload.
- `field_locks` rows are an open `(manifestation_id, entity_type, field_name)` triple: the HTTP `lock` endpoint accepts
  any `field_name` string with no check against a known field set, so a lock can name a field this pipeline never emits.
- `writeback_jobs.reason` is `'metadata'` or `'cover'`; this subject's `enqueue_writeback` picks `'cover'` only when the
  applied field name is `"cover"` or `"cover_url"`. No adapter emits `"cover"`. Open Library is the only adapter that
  emits `"cover_url"`, and its default policy is `Propose` (not in `default_policy`'s `AutoFill` list), so it always
  stages for review and never reaches `Decision::Apply`; the `'cover'` reason and `apply_field`'s dedicated cover branch
  are consequently never reached from any automated apply.
- `EnrichmentConfig` (`enabled`, `concurrency`, `poll_idle_secs`, `fetch_budget_secs`, `http_timeout_secs`,
  `max_attempts`, the three `cache_ttl_*` fields) loads once from the environment through `figment` at process startup.
  `backend/src/lib.rs` clones the whole `Config` into the task it spawns for `spawn_queue`; the running worker never
  re-reads it afterwards. The `settings` table carries a parallel `enrichment_enabled`/
  `enrichment_concurrency`/`enrichment_poll_idle_secs`/`enrichment_fetch_budget_secs` group, validated and stored
  through `PUT /api/v1/settings`, but nothing in `queue.rs` reads `AppState::settings` (the live, revision-gated cache
  that group otherwise mirrors): a change to that group changes the validated row and nothing else; the running queue
  re-reads it only when the process restarts, picking up the new environment value if one was set to match.

## Runtime behaviour

**A background pass**, once `claim_next` claims a row:

1. `run_once` calls `load_snapshot`, which reads canonical fields, active identifier-registry slots and provider
   priorities, then derives `lookup_keys` in order: the ISBN key first (if either ISBN parses), then registry ids for
   the API-capable schemes ordered by `base_priority` then fixed provider precedence then level then value, then a
   title/author fuzzy key last. A manifestation with none of these returns an empty `RunOutcome` immediately.
2. `fan_out_with_fallback` tries each key under one shared deadline. An `ExternalId` key dispatches only to the adapter
   whose `id()` matches its scheme; every other key variant fans out to every enabled source. `fan_out` awaits a
   `FuturesUnordered` of the sources' `lookup` calls against a `sleep` deadline: whichever complete first are kept, and
   any adapter still pending when the budget expires is synthesised as `SourceError::Timeout` without discarding the
   sources that already answered. Each attempt's results are cached under that key's own cache key before the next key
   is tried; a hit stops the fallback chain.
3. `apply_journal_batch` upserts one `metadata_versions` row per field observation and groups the resulting
   `(source_id, PolicyInputRow)` pairs by field name.
4. `apply_canonical_batch` locks the manifestation row `FOR UPDATE` once, then, per field: locks the work row too when
   the field needs a work-level identifier re-read (identifier fields only), reads whether the field is locked, decides
   emptiness, and loads any pending rows from earlier runs. `canonical_empty_under_lock` re-reads the registry slot
   under the lock for an identifier field only; for every scalar field it returns the emptiness recorded in the
   `load_snapshot` state taken before the provider round trip, so a scalar value an operator sets between that snapshot
   and this apply is judged empty and overwritten. It then walks this run's sources for that field in fan-out completion
   order (not a fixed provider priority): for each it computes confidence, builds the disagreement set from the
   earlier-run pending rows plus every *other* source's observation in this same run, and calls `policy::decide`. On
   `Decision::Apply` it calls `apply_field`, on success triggers the ISBN rematch hook when the field is
   `isbn_10`/`isbn_13`, enqueues a writeback job for any non-identifier field, and `break`s out of the per-source loop,
   so when multiple sources agree in the same run, whichever completed the fan-out first is the one whose journal row
   becomes the canonical pointer, not a fixed tie-break.
5. The transaction commits once, carrying the journal writes, the canonical updates, the rematch outcome and every
   writeback enqueue together.
6. `finish` (in `queue.rs`) inspects the outcome: if every enabled source failed non-terminally and nothing applied or
   staged, the row is marked `Failed` (eligible for retry) with the longest reported `Retry-After` honoured; otherwise
   (anything applied or staged, or a terminal per-source failure alongside a live outcome) it is marked `Complete`.

**A manual trigger** (`POST .../enrichment/trigger`) on an idle row resets it straight to `pending` with a cleared
attempt count and error; on an `in_progress` row it leaves the claim untouched and sets `enrichment_rerun_requested`, so
a second worker can never pick up a row the first is still processing. The same CASE-guarded `UPDATE` shape appears in
`routes/metadata.rs::apply_identifier_patches`, fired whenever a manual identifier `PATCH` sets or clears at least one
identifier: an identifier edit re-queues enrichment exactly as a trigger would.

**A dry run** (`POST .../enrichment/dry-run`) repeats the fan-out and the cache writes but calls `policy::decide` with a
synthetic `PolicyInputRow` (`id: Uuid::nil()`) never written to the journal, so its `would_apply`/`would_stage` split is
a projection, not a preview of a specific journal row a subsequent real run would reuse.

## Failure and recovery

- A source-level failure is one of `SourceError::NotFound`/`RateLimited`/`Http`/`Timeout`/`Other`, summarised into a
  `SourceFailure` with a `terminal` flag (true only for a non-429 4xx) and, for `RateLimited`, a `retry_after`. `finish`
  only marks a row `Failed` when every enabled source failed non-terminally with nothing applied or staged; a terminal
  failure alongside a live result from another source still counts as `Complete`.
- After `max_attempts` failed attempts a row moves to `Skipped` and the claim query never selects it again; nothing
  short of `trigger` or a manual identifier edit returns it to circulation.
- A hard kill mid-run leaves the claimed row `in_progress` indefinitely: `spawn_queue` calls `revert_in_progress` only
  on its cancellation branch, never at startup, so nothing reclaims a row whose worker died with the process. `trigger`
  cannot release it either: its `UPDATE` only sets `enrichment_rerun_requested` when the row already reads
  `in_progress`, leaving the status itself untouched. Only a graceful shutdown-and-restart cycle (whose shutdown path
  runs the revert before the process exits) clears it. The Design "Writeback pipeline" describes the equivalent revert
  running both at startup and at shutdown for that queue; this queue runs only the shutdown half.
- `apply_field` rejects a journal value its target column cannot hold: a non-scalar JSON value for a text field, a
  non-positive `pages` value, or a `pub_date` string that fails a round-trip parse against the accepted `YYYY-MM-DD`
  spelling and a `[0001, 9999]` year bound, by returning `false` rather than applying. The journal row stays `pending`
  for a human reviewer, and the run does not count the field as applied or enqueue a writeback for it.
- `upsert_ratings` treats a `RatingSignal::Unusable` value (one that fails `RatingObservation::new`'s range check) as
  grounds to clear any previously cached rating for that `(manifestation, source)` rather than keep serving a stale one;
  a path that carries no rating information at all (`Unknown`) leaves the cache untouched.
- `dry_run::preview`'s field-lock entity-type inference is narrower than the real run's: it treats only `"title"`,
  `"description"` and `"language"` as work-level fields, everything else as manifestation-level, while
  `apply_canonical_batch` (via `is_work_field`) also treats `subtitle`, every `contributors.*` field and every
  `identifiers.work.*` field as work-level. All three adapters emit `subtitle` and `contributors.author` observations,
  so a lock recorded against one of those fields at the work level is honoured correctly by a real run but not reflected
  as `locked` in a dry-run preview of the same manifestation; the preview shows it as a would-apply or would-stage
  change instead.

## Security and operations

Two handlers cross from the caller's row-level-security context into the ingestion role's unconditional one, matching
the pattern the row-level-security Design records: `dry_run` checks visibility on `state.pool` under `acquire_with_rls`
first, then drops that transaction and reads through `state.ingestion_pool` for the actual fan-out; the queue's own
`run_once` never opens an `acquire_with_rls` transaction at all, since a background job has no per-request caller to
scope to. `trigger` and `status` stay on the caller's row-level-security-scoped pool throughout.

Both the queue and the dry run read and write `manifestations` over `state.ingestion_pool`, never over
`acquire_with_rls`, relying on `manifestations_ingestion_full_access`'s unconditional grant to the `reverie_ingestion`
role. Which role that pool actually connects as is an operator precondition, not a fact this subject controls: with
`DATABASE_URL_INGESTION` configured, it is `reverie_ingestion` and every claim, snapshot read and canonical apply
proceeds as designed; left unset, the pool falls back to the same connection string and role as the request-handling
pool (`reverie_app`) with no `app.current_user_id` ever set on it, so the ordinary per-user `manifestations` policies
apply instead and admit no row: `claim_next`'s claim, `load_snapshot`'s read and every canonical `UPDATE` then match
nothing, and the queue runs indefinitely without ever processing a manifestation, with no error surfaced.

The dry run declares read scope even though it calls every configured metadata provider live and writes their responses
to `api_cache` over the ingestion pool; it changes no manifestation, journal or writeback row. This is a deliberate
exception to the general rule that a state-changing operation needs write scope, not an oversight this Design leaves
unstated.

`content_rating`'s default policy is pinned to `Propose` and has no entry in `default_policy`'s `AutoFill` match arm; a
unit test (`policy::tests::content_rating_never_autofills`) pins the mapping, so promoting it to `AutoFill` fails that
test immediately. No adapter emits an observation for that field. `content_rating` is a manually-set, library-facing
field (`routes/library/mod.rs` reads and returns it); the pin keeps every automated source out of that column so the
value the child-account safety layer reads is never auto-applied.

Every outbound metadata request goes through `http::api_client`, built by the same `ssrf_client` constructor as
`http::cover_client`: a process-wide custom `DNS` resolver drops any resolved address in a denied range before `reqwest`
ever dials it, and the redirect policy re-validates every hop against the same denied ranges via `validate_hop` before
following it, closing both the DNS-rebinding gap a redirect-time-only check would leave open and the case where a
follow-on hop in a redirect chain resolves to a different, denied address. Nothing in the production path builds a
`cover_client`; only `cover_download.rs`'s own test module constructs one. Every client carries an explicit `User-Agent`
derived from `Config::user_agent()` (the installation's contact string, or `"unidentified"`), the convention every
outbound client follows.

A caller mints the request that eventually reaches an external provider (through `trigger`, or a manual identifier
edit's implicit re-queue), but chooses no target: `LookupKey`s are derived server-side from the manifestation's own
ISBN, registry identifiers and title/author, never from caller-supplied input, so this subject offers no path to make
`api_client` fetch an arbitrary URL.

## More information

- [External identifiers](../../../../website/src/content/docs/guides/external-identifiers.md): the user-facing guide to
  native-id lookup, identifier editing and provider visibility this subject implements the automated half of.
