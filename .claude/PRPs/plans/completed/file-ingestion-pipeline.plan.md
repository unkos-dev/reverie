# Plan: File Ingestion Pipeline

## Summary

Implement the core file management pipeline: a filesystem watcher on the ingestion
directory, format filtering with priority selection, copy to managed library with path
templates, SHA-256 integrity verification, post-ingestion cleanup, and quarantine for
failures. This is the foundational I/O pipeline that all subsequent steps (validation,
metadata extraction, enrichment) build on.

## User Story

As a self-hosted ebook library administrator,
I want new files dropped into an ingestion folder to be automatically detected, copied
into my managed library with correct paths, and verified for integrity,
so that my library is populated without manual intervention and no data is ever lost.

## Problem -> Solution

**Current state:** The app skeleton exists with auth, config, DB pool, and health
endpoints. Files placed in the ingestion directory are ignored.

**Desired state:** A background watcher detects new files, selects the highest-priority
format per title, copies them to the managed library using path templates, verifies
integrity via SHA-256, cleans up the ingestion folder on success, and quarantines
failures with structured error context. The pipeline is tracked via `ingestion_jobs`
rows in the database.

## Metadata

- **Complexity**: Large
- **Source PRD**: `/home/coder/Tome/plans/DESIGN_BRIEF.md` (Section 3)
- **Blueprint Step**: Step 4 — File Ingestion Pipeline
- **Estimated Files**: 16-18

---

## UX Design

N/A -- internal change. No user-facing UI in this step. The only user touchpoint is
dropping files into the ingestion directory and (optionally) triggering a manual scan
via `POST /api/ingestion/scan`.

---

## Mandatory Reading

| Priority | File | Lines | Why |
|---|---|---|---|
| P0 | `backend/src/config.rs` | all | Config pattern, existing paths (library, ingestion, quarantine) |
| P0 | `backend/src/state.rs` | all | AppState struct -- needs ingestion_pool added |
| P0 | `backend/src/main.rs` | all | Router assembly, shutdown signal, background task wiring |
| P0 | `backend/src/error.rs` | all | AppError variants, IntoResponse pattern |
| P0 | `backend/src/models/mod.rs` | all | Model registration pattern |
| P0 | `backend/src/models/device_token.rs` | all | Transaction + FOR UPDATE pattern for create_with_limit |
| P0 | `backend/src/routes/tokens.rs` | all | Route handler pattern with auth, error mapping |
| P0 | `backend/src/test_support.rs` | all | Test helpers, test_config(), test_state() |
| P1 | `backend/migrations/20260412150005_system_tables.up.sql` | all | ingestion_jobs table schema |
| P1 | `backend/migrations/20260412150002_core_tables.up.sql` | all | manifestations table (file_path, file_hash, ingestion_status) |
| P1 | `backend/migrations/20260412150001_extensions_enums_and_roles.up.sql` | all | Enums: manifestation_format, ingestion_status, job_status |
| P1 | `backend/src/db.rs` | all | Pool init, acquire_with_rls pattern |
| P2 | `plans/DESIGN_BRIEF.md` | 73-141 | File Management Architecture section |

## External Documentation

| Topic | Source | Key Takeaway |
|---|---|---|
| `notify` crate | docs.rs/notify/7 | Filesystem watcher. Use `RecommendedWatcher` with `EventKind::Create`. Debounce via `notify-debouncer-full` or manual tokio delay. |
| `sha2` crate | docs.rs/sha2 | `Sha256::new()`, `.update(buf)`, `.finalize()` -> hex string via `format!("{:x}", hash)`. |
| `walkdir` crate | docs.rs/walkdir | Recursive directory traversal. `WalkDir::new(path).into_iter().filter_map(Result::ok)`. |
| `tempfile` crate | docs.rs/tempfile | `NamedTempFile::new_in(dir)` for atomic writes -- write to temp, then `persist()` to final path. |

---

## Patterns to Mirror

### NAMING_CONVENTION

```rust
// SOURCE: backend/src/models/device_token.rs:1-10
// Models: snake_case module, PascalCase struct, derive Debug/Clone/Serialize
// Functions: verb_noun (find_by_id, create_with_limit, list_for_user)
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct DeviceToken { ... }
```

### ERROR_HANDLING

```rust
// SOURCE: backend/src/error.rs:7-15
// Custom error enum with thiserror, IntoResponse for Axum
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
```

### MODEL_WITH_CUSTOM_ERROR

```rust
// SOURCE: backend/src/models/device_token.rs:25-30
// Domain-specific error enums in model layer, mapped in route handlers
pub enum CreateError {
    LimitExceeded,
    Db(sqlx::Error),
}
```

### ROUTE_HANDLER_PATTERN

```rust
// SOURCE: backend/src/routes/tokens.rs:12-16
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tokens", post(create_token))
        .route("/api/tokens", get(list_tokens))
        .route("/api/tokens/{id}", delete(revoke_token))
}

// Handler signature with auth + state + body
async fn create_token(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<impl IntoResponse, AppError> { ... }
```

### CONFIG_PATTERN

```rust
// SOURCE: backend/src/config.rs:28-35
// Required vars via map_err, optional vars via unwrap_or_else
let database_url =
    env::var("DATABASE_URL").map_err(|_| ConfigError::MissingVar("DATABASE_URL".into()))?;
let port = env::var("TOME_PORT")
    .unwrap_or_else(|_| "3000".into())
    .parse::<u16>()
    .map_err(|e| ConfigError::Invalid { var: "TOME_PORT".into(), reason: e.to_string() })?;
```

### TEST_STRUCTURE

```rust
// SOURCE: backend/src/test_support.rs:1-20
// Shared test helpers: test_config(), test_state(), test_server()
// Unit tests in #[cfg(test)] mod tests { ... } alongside code
// Integration tests use #[ignore] when requiring running postgres
```

### TRANSACTION_PATTERN

```rust
// SOURCE: backend/src/models/device_token.rs:55-75
// Transaction with SELECT FOR UPDATE for atomic operations
let mut tx = pool.begin().await.map_err(CreateError::Db)?;
let row: (i64,) = sqlx::query_as(
    "SELECT count(*) FROM device_tokens WHERE user_id = $1 AND revoked_at IS NULL FOR UPDATE"
)
.bind(user_id)
.fetch_one(&mut *tx)
.await
.map_err(CreateError::Db)?;
// ... check + insert ...
tx.commit().await.map_err(CreateError::Db)?;
```

### ROUTER_ASSEMBLY

```rust
// SOURCE: backend/src/main.rs:20-35
// Routes merged into root router, layers applied after
Router::new()
    .merge(routes::health::router())
    .merge(routes::auth::router())
    .merge(routes::tokens::router())
    .layer(auth_layer)
    .layer(tower_http::trace::TraceLayer::new_for_http())
    .with_state(state)
```

### FIRE_AND_FORGET

```rust
// SOURCE: backend/src/auth/middleware.rs:70-73
let pool = state.pool.clone();
tokio::spawn(async move {
    let _ = device_token::update_last_used(&pool, token_id).await;
});
```

---

## Architectural Decisions

### Dual Database Pools

The ingestion pipeline runs as a background task and needs the `tome_ingestion` DB
role (permissive RLS on manifestations, access to ingestion_jobs). The web app uses
`tome_app` (user-scoped RLS). These are intentionally separate roles.

**Decision:** Add `ingestion_database_url` to Config (from `DATABASE_URL_INGESTION`
env var). Add `ingestion_pool` to AppState. The background watcher task uses the
ingestion pool; route handlers continue using the app pool.

**Fallback:** If `DATABASE_URL_INGESTION` is not set, fall back to `DATABASE_URL`.
This lets dev setups work with a single pool while production uses separate roles.

### Format Priority Config

**Decision:** Add `TOME_FORMAT_PRIORITY` env var, parsed as a comma-separated ordered
list of format strings. Default: `epub,pdf,mobi,azw3,cbz,cbr`.
Stored as `Vec<String>` in Config. The format_filter module maps these to the DB
`manifestation_format` enum at runtime.

### Path Template Inputs

Step 4 uses **filename heuristics** for path template variables (parse author/title
from filename patterns like `Author - Title.epub`). Real OPF metadata extraction is
Step 6. The path template engine must be ready for richer inputs but will receive
stub/heuristic values for now.

### Cleanup Contract

The DESIGN_BRIEF says "Tome never writes to, modifies, or deletes files in the
ingestion source path **during the ingestion process**." Cleanup is a **separate
phase** that runs only after the **entire batch** has been successfully processed.
If any file in a batch fails, no cleanup occurs for that batch -- the user must
resolve the failure first.

### Watcher Shutdown

The watcher runs as a background tokio task spawned from `main()`. It receives a
`tokio_util::sync::CancellationToken` that is triggered by the existing
`shutdown_signal()`. The watcher's event loop checks `token.is_cancelled()` to
exit cleanly.

---

## Files to Change

| File | Action | Justification |
|---|---|---|
| `backend/migrations/YYYYMMDD_add_skipped_job_status.up.sql` | CREATE | Add `skipped` value to `job_status` enum |
| `backend/migrations/YYYYMMDD_add_skipped_job_status.down.sql` | CREATE | No-op (PG can't remove enum values) |
| `backend/Cargo.toml` | UPDATE | Add dependencies: notify, sha2, walkdir, tempfile, tokio-util |
| `backend/src/config.rs` | UPDATE | Add `ingestion_database_url`, `format_priority` fields |
| `backend/src/state.rs` | UPDATE | Add `ingestion_pool: PgPool` |
| `backend/src/main.rs` | UPDATE | Init ingestion pool, spawn watcher task with cancellation token |
| `backend/src/test_support.rs` | UPDATE | Add `ingestion_pool` to test_state() |
| `backend/src/models/mod.rs` | UPDATE | Add `pub mod ingestion_job;` |
| `backend/src/models/ingestion_job.rs` | CREATE | CRUD for ingestion_jobs table |
| `backend/src/services/mod.rs` | UPDATE | Add `pub mod ingestion;` |
| `backend/src/services/ingestion/mod.rs` | CREATE | Pipeline orchestrator |
| `backend/src/services/ingestion/watcher.rs` | CREATE | notify-based filesystem watcher |
| `backend/src/services/ingestion/format_filter.rs` | CREATE | Priority-ordered format selection |
| `backend/src/services/ingestion/path_template.rs` | CREATE | Path template rendering + sanitization |
| `backend/src/services/ingestion/copier.rs` | CREATE | Atomic copy with SHA-256 verification |
| `backend/src/services/ingestion/cleanup.rs` | CREATE | Post-ingestion source file removal |
| `backend/src/services/ingestion/quarantine.rs` | CREATE | Failed file quarantine with sidecar |
| `backend/src/routes/mod.rs` | UPDATE | Add `pub mod ingestion;` |
| `backend/src/routes/ingestion.rs` | CREATE | POST /api/ingestion/scan endpoint |
| `.env.example` | UPDATE | Add DATABASE_URL_INGESTION, TOME_FORMAT_PRIORITY |

## NOT Building

- OPF metadata extraction (Step 6) -- path templates use filename heuristics only
- EPUB structural validation (Step 5) -- files are copied as-is
- Metadata enrichment (Step 7) -- no API calls to Open Library / Google Books
- Auth on the scan endpoint -- will be wired when auth middleware is production-ready
- Web UI for ingestion status -- Step 10
- Webhook notifications on ingestion events -- Step 12
- Hardlink detection or management -- outside Tome's scope per design brief

---

## Step-by-Step Tasks

### Task 0: Add `skipped` to `job_status` Enum

- **ACTION**: Create a new migration to add `skipped` to the `job_status` enum
- **IMPLEMENT**: New migration file (next sequence number after existing migrations):
  ```sql
  -- up
  ALTER TYPE job_status ADD VALUE 'skipped';
  -- down is not possible (PG doesn't support removing enum values)
  ```
- **MIRROR**: Existing migration naming pattern: `YYYYMMDDHHMMSS_description.{up,down}.sql`
- **GOTCHA**: `ALTER TYPE ... ADD VALUE` cannot run inside a transaction in older PG
  versions. sqlx runs each migration in a transaction by default. For PG 12+, `ADD VALUE`
  inside a transaction is supported. The dev docker-compose uses PG 16, so this is fine.
  The down migration should be a no-op comment explaining PG doesn't support removing
  enum values.
- **VALIDATE**: `sqlx migrate run` succeeds. Query `SELECT enum_range(NULL::job_status)`
  shows the new value.

### Task 1: Add Dependencies

- **ACTION**: Add new crates to `backend/Cargo.toml`
- **IMPLEMENT**:
  ```toml
  notify = "7"
  sha2 = "0.10"
  walkdir = "2"
  tempfile = "3"
  tokio-util = "0.7"
  ```
- **MIRROR**: Existing dependency style in Cargo.toml (version strings, feature lists)
- **GOTCHA**: `RecommendedWatcher` auto-selects the right backend per platform
  (inotify on Linux, kqueue on macOS). No feature flags needed. Verify the major
  version with `cargo add notify` at implementation time.
- **VALIDATE**: `cargo check` succeeds

### Task 2: Extend Config

- **ACTION**: Add `ingestion_database_url` and `format_priority` to Config
- **IMPLEMENT**: In `config.rs`:
  - Add `pub ingestion_database_url: String` -- defaults to `database_url.clone()` if
    `DATABASE_URL_INGESTION` is not set
  - Add `pub format_priority: Vec<String>` -- parsed from `TOME_FORMAT_PRIORITY` env var,
    default `"epub,pdf,mobi,azw3,cbz,cbr"`, split on commas, trimmed, lowercased
- **MIRROR**: CONFIG_PATTERN -- unwrap_or_else for defaults, map_err for parse errors
- **IMPORTS**: None new (std::env already imported)
- **GOTCHA**: Store as `Vec<String>` not enum -- the format_filter module will map to
  `manifestation_format` enum. Keep config parsing simple.
- **VALIDATE**: Existing config tests still pass. Add test for format_priority parsing
  and ingestion_database_url fallback. Also update `.env.example` with
  `DATABASE_URL_INGESTION` and `TOME_FORMAT_PRIORITY` entries (commented, with defaults).

### Task 3: Extend AppState and Test Support

- **ACTION**: Add `ingestion_pool` to AppState and update test helpers
- **IMPLEMENT**:
  - `state.rs`: Add `pub ingestion_pool: PgPool`
  - `test_support.rs`: Add `ingestion_pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap()`
    to `test_state()`
- **MIRROR**: Existing state.rs pattern (derive Clone, pub fields)
- **GOTCHA**: `test_support.rs` must also add `ingestion_database_url` and
  `format_priority` to `test_config()`.
- **VALIDATE**: `cargo check` succeeds

### Task 4: Update main.rs -- Ingestion Pool and Background Task

- **ACTION**: Initialize the ingestion pool and spawn the watcher as a background task
- **IMPLEMENT**:
  ```rust
  // After app pool init:
  let ingestion_pool = db::init_pool(
      &config.ingestion_database_url,
      config.db_max_connections,
  ).await.expect("failed to connect ingestion pool");

  // Before server bind:
  let cancel_token = tokio_util::sync::CancellationToken::new();
  let watcher_token = cancel_token.clone();
  let watcher_config = config.clone();
  let watcher_pool = ingestion_pool.clone();
  tokio::spawn(async move {
      if let Err(e) = services::ingestion::run_watcher(
          watcher_config, watcher_pool, watcher_token
      ).await {
          tracing::error!(error = %e, "ingestion watcher exited with error");
      }
  });

  // In shutdown_signal, cancel the token:
  cancel_token.cancel();
  ```
- **MIRROR**: ROUTER_ASSEMBLY, existing pool init pattern
- **IMPORTS**: `tokio_util::sync::CancellationToken`, `crate::services`
- **GOTCHA**: The cancellation token must be cancelled BEFORE the graceful shutdown
  completes. Wire it into `shutdown_signal()` or cancel it immediately after the
  signal fires.
- **VALIDATE**: `cargo check` succeeds. Server starts and shuts down cleanly.

### Task 5: Create ingestion_job Model

- **ACTION**: Create `backend/src/models/ingestion_job.rs`
- **IMPLEMENT**:
  - Struct `IngestionJob` matching the `ingestion_jobs` table schema:
    `id, batch_id, source_path, status, error_message, started_at, completed_at, created_at`
  - `create(pool, batch_id, source_path)` -- INSERT with status='queued'
  - `mark_running(pool, id)` -- UPDATE status='running', started_at=now()
  - `mark_complete(pool, id)` -- UPDATE status='complete', completed_at=now()
  - `mark_skipped(pool, id)` -- UPDATE status='skipped', completed_at=now()
  - `mark_failed(pool, id, error_message)` -- UPDATE status='failed', error_message, completed_at=now()
  - `find_by_batch(pool, batch_id)` -- SELECT all jobs in a batch
  - Status stored as `String` (cast from enum in SQL: `status::text`)
  - NOTE: `source_path` stores the full file path for each job (not the batch
    directory). The `batch_id` groups jobs into a batch; the directory is implicitly
    the common parent of all `source_path` values in a batch.
- **MIRROR**: MODEL_WITH_CUSTOM_ERROR, TRANSACTION_PATTERN (user.rs pattern for sqlx queries)
- **IMPORTS**: `sqlx::PgPool`, `uuid::Uuid`, `time::OffsetDateTime`, `serde::Serialize`
- **GOTCHA**: Use `status::text` in SELECT to avoid sqlx enum mapping complexity.
  The `job_status` enum is already defined in the DB -- use string casts like user.rs
  does for `role::text`.
- **VALIDATE**: Unit tests for each status transition. Integration test with `#[ignore]`
  for actual DB round-trip.

### Task 6: Create Format Filter

- **ACTION**: Create `backend/src/services/ingestion/format_filter.rs`
- **IMPLEMENT**:
  - `select_by_priority(files: &[PathBuf], priority: &[String]) -> Vec<PathBuf>`
  - Group files by stem (filename without extension) as a proxy for "same title"
  - For each group, select the file whose extension matches the highest-priority format
  - Return the selected files in priority order
  - Unknown extensions are silently ignored (not selected, not quarantined)
- **MIRROR**: Pure function, no DB or IO
- **IMPORTS**: `std::path::PathBuf`, `std::collections::HashMap`
- **GOTCHA**: Extensions should be compared case-insensitively (`.EPUB` == `.epub`).
  Grouping by stem is a rough heuristic -- `Author - Title.epub` and
  `Author - Title.pdf` share stem `Author - Title`. This is intentionally simple;
  real title matching comes in Step 6.
- **VALIDATE**: Unit tests for: single format, multiple formats same stem, no matching
  formats, mixed case extensions, files with no extension.

### Task 7: Create Path Template Engine

- **ACTION**: Create `backend/src/services/ingestion/path_template.rs`
- **IMPLEMENT**:
  - `render(template: &str, vars: &HashMap<String, String>) -> PathBuf`
  - Replaces `{Author}`, `{Title}`, `{Series}` etc. with values from vars map
  - `sanitize_path_component(s: &str) -> String` -- replace path-unsafe chars
    (`/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`, null) with `_`. Trim leading/
    trailing whitespace and dots. Collapse consecutive underscores.
  - `resolve_collision(path: &Path) -> PathBuf` -- if `path` exists, try
    `stem (2).ext`, `stem (3).ext`, etc. up to 999 then error.
  - `heuristic_vars_from_filename(filename: &str) -> HashMap<String, String>` --
    parse `Author - Title.ext` pattern. If no ` - ` separator, use "Unknown" for
    author and the whole stem as title.
- **MIRROR**: Pure functions except `resolve_collision` which checks filesystem
- **IMPORTS**: `std::path::{Path, PathBuf}`, `std::collections::HashMap`
- **GOTCHA**: Path template default is `{Author}/{Title}.{ext}`. The template itself
  is not yet configurable (hardcoded default). Making it a config option is fine but
  not required.
- **VALIDATE**: Unit tests for template rendering, sanitization edge cases (unicode,
  empty strings, all-dots filenames), collision resolution with tempdir.

### Task 8: Create Copier (Atomic Copy with Verification)

- **ACTION**: Create `backend/src/services/ingestion/copier.rs`
- **IMPLEMENT**:
  - `copy_verified(source: &Path, dest_dir: &Path, dest_relative: &Path) -> Result<CopyResult, CopyError>`
  - `CopyResult { dest_path: PathBuf, sha256: String, file_size: u64 }`
  - `CopyError` enum: `Io(std::io::Error)`, `HashMismatch { source: String, dest: String }`,
    `DestExists(PathBuf)`
  - Algorithm:
    1. Create parent directories for dest
    2. Hash source file (streaming SHA-256)
    3. Create `NamedTempFile::new_in(dest_dir)` (same filesystem for atomic rename)
    4. Copy bytes from source to temp, hashing dest as we write
    5. Compare hashes -- if mismatch, return `CopyError::HashMismatch`
    6. `temp.persist(final_path)` -- atomic rename
  - `hash_file(path: &Path) -> Result<String, io::Error>` -- streaming SHA-256,
    returns lowercase hex string. 64KB read buffer.
- **MIRROR**: Pure I/O, no DB interaction. Error enum follows MODEL_WITH_CUSTOM_ERROR.
- **IMPORTS**: `sha2::{Sha256, Digest}`, `tempfile::NamedTempFile`,
  `std::io::{self, Read, Write, BufReader, BufWriter}`, `tokio::fs`
- **GOTCHA**: Use synchronous I/O in a `tokio::task::spawn_blocking` block for file
  operations. The `notify` + `sha2` + `tempfile` crates are all sync. Wrap the entire
  copy+hash operation in `spawn_blocking` rather than mixing sync/async.
- **VALIDATE**: Integration test using `tempfile::tempdir()`: copy a file, verify hash
  matches, verify dest contents match source. Test hash mismatch detection (hard to
  trigger naturally -- test `hash_file` independently).

### Task 9: Create Quarantine Handler

- **ACTION**: Create `backend/src/services/ingestion/quarantine.rs`
- **IMPLEMENT**:
  - `quarantine_file(source: &Path, quarantine_dir: &Path, reason: &str) -> Result<PathBuf, io::Error>`
  - Moves (rename or copy+delete) the file to `quarantine_dir/` preserving the
    original filename. If collision, append timestamp.
  - Writes a JSON sidecar file alongside: `{filename}.quarantine.json` containing:
    ```json
    {
      "original_path": "/ingestion/Author - Title.epub",
      "reason": "SHA-256 mismatch after copy",
      "quarantined_at": "2026-04-14T16:00:00Z"
    }
    ```
  - Uses `time::OffsetDateTime::now_utc()` for timestamp (matches crate already in deps).
- **MIRROR**: Error handling follows CopyError pattern
- **IMPORTS**: `std::path::{Path, PathBuf}`, `serde_json`, `time::OffsetDateTime`
- **GOTCHA**: Source and quarantine may be on different filesystems, so `fs::rename`
  may fail. Fall back to copy + delete. Use `spawn_blocking` for the I/O.
- **VALIDATE**: Test with tempdir: quarantine a file, verify sidecar JSON, verify
  collision handling.

### Task 10: Create Cleanup Handler

- **ACTION**: Create `backend/src/services/ingestion/cleanup.rs`
- **IMPLEMENT**:
  - `cleanup_batch(paths: &[PathBuf]) -> Result<CleanupResult, io::Error>`
  - `CleanupResult { removed_files: usize, removed_dirs: usize }`
  - Deletes each file in `paths`
  - After all files removed, walk parent directories bottom-up and remove empty ones
    (stop at the ingestion root -- never delete the ingestion directory itself)
  - Takes `ingestion_root: &Path` parameter to bound directory removal
- **MIRROR**: Pure I/O, no DB
- **IMPORTS**: `std::path::{Path, PathBuf}`, `std::fs`
- **GOTCHA**: **Only called after entire batch succeeds.** The orchestrator is
  responsible for the "all succeeded" check. This module just does the deletion.
  Must handle TOCTOU: a file might be gone (another process deleted it) -- treat
  `NotFound` on delete as success, not error.
- **VALIDATE**: Test with tempdir: create files in nested dirs, cleanup, verify files
  gone and empty parent dirs removed, verify ingestion root dir itself preserved.

### Task 11: Create Filesystem Watcher

- **ACTION**: Create `backend/src/services/ingestion/watcher.rs`
- **IMPLEMENT**:
  - `watch(ingestion_path: PathBuf, tx: tokio::sync::mpsc::Sender<Vec<PathBuf>>, cancel: CancellationToken) -> Result<(), anyhow::Error>`
  - Uses `notify::RecommendedWatcher` with `notify::Config::default()`
  - Watches `ingestion_path` recursively
  - On `EventKind::Create` or `EventKind::Modify`, collect paths and debounce:
    accumulate events for 2 seconds of quiet, then send batch via channel
  - Respects `cancel.is_cancelled()` to exit the loop
  - Does NOT process files -- only detects and forwards paths
- **MIRROR**: Cancellation token pattern from Task 4
- **IMPORTS**: `notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind}`,
  `tokio::sync::mpsc`, `tokio_util::sync::CancellationToken`, `std::path::PathBuf`
- **GOTCHA**: `notify` sends events on a background thread. Use a `std::sync::mpsc`
  channel from notify -> tokio, then bridge into a `tokio::sync::mpsc` channel.
  Alternatively, use `notify`'s async features if available in v7. Debouncing is
  critical -- filesystem events fire multiple times per file (create, modify, close).
  A 2-second quiet period after the last event is sufficient.
- **VALIDATE**: Integration test with tempdir: start watcher, create a file, verify
  batch received on channel. Test graceful shutdown via cancel token.

### Task 12: Create Pipeline Orchestrator

- **ACTION**: Create `backend/src/services/ingestion/mod.rs`
- **IMPLEMENT**:
  - `pub async fn run_watcher(config: Config, pool: PgPool, cancel: CancellationToken) -> Result<(), anyhow::Error>`
  - Main loop:
    1. Start filesystem watcher, receive batches on channel
    2. On batch received:
       a. Create batch_id (Uuid::new_v4())
       b. Scan with `walkdir` to find all files — retain this full list as
          `all_source_files` (needed for cleanup later)
       c. Run format_filter to select priority files from `all_source_files`
       d. For each selected file, create ingestion_job (status=queued)
       e. For each job:
          - Mark running
          - **Duplicate check**: compute source file hash, then check if a
            manifestation with the same `file_hash` or target `file_path`
            already exists. If so, mark job as `skipped` and continue to
            next file. This handles re-ingestion after partial batch failure,
            watcher re-firing, and manual scan during watcher processing.
          - Parse heuristic vars from filename
          - Render path template
          - Resolve collision
          - Copy with verification
          - On success: mark complete, create work + manifestation rows
          - On failure: mark failed, quarantine file
       f. If ALL jobs in batch succeeded or were skipped (none failed),
          run cleanup on `all_source_files` (the full walkdir list,
          including non-selected formats)
       g. If any job failed, skip cleanup (source files preserved)
  - `pub async fn scan_once(config: &Config, pool: &PgPool) -> Result<ScanResult, anyhow::Error>`
    - One-shot version for the manual scan endpoint
    - Same logic as step 2 above but without the watcher loop
    - Returns `ScanResult { processed: usize, failed: usize, skipped: usize }`
- **MIRROR**: FIRE_AND_FORGET for non-critical updates, TRANSACTION_PATTERN for
  job status transitions
- **IMPORTS**: All submodules, `uuid::Uuid`, `sqlx::PgPool`, `tracing`
- **GOTCHA**: The orchestrator creates manifestation rows with placeholder work_id.
  Step 6 will populate real work/author associations. For now, create one work row
  per file using heuristic title from `heuristic_vars_from_filename` as both `title`
  and `sort_title`. INSERT into works + manifestations in a single transaction.
  This is NOT a shared singleton "Unknown" work -- each file gets its own work row.
  Step 6 will reconcile duplicates and merge works when real metadata arrives.
- **VALIDATE**: Full integration test: place files in temp ingestion dir, run
  `scan_once`, verify manifestation rows exist, verify files in library dir,
  verify cleanup ran, verify quarantine for a corrupt file.

### Task 13: Create Scan API Endpoint

- **ACTION**: Create `backend/src/routes/ingestion.rs`
- **IMPLEMENT**:
  ```rust
  pub fn router() -> Router<AppState> {
      Router::new()
          .route("/api/ingestion/scan", post(scan))
  }

  async fn scan(
      current_user: CurrentUser,
      State(state): State<AppState>,
  ) -> Result<impl IntoResponse, AppError> {
      if current_user.role != "admin" {
          return Err(AppError::Forbidden);
      }
      let result = services::ingestion::scan_once(
          &state.config, &state.ingestion_pool
      ).await.map_err(|e| AppError::Internal(e))?;
      Ok(Json(serde_json::json!({
          "processed": result.processed,
          "failed": result.failed,
          "skipped": result.skipped,
      })))
  }
  ```
- **MIRROR**: ROUTE_HANDLER_PATTERN (same as routes/tokens.rs with CurrentUser)
- **IMPORTS**: `crate::services`, `crate::state::AppState`, `crate::error::AppError`,
  `crate::auth::middleware::CurrentUser`
- **GOTCHA**: Requires `CurrentUser` (admin-only). The scan triggers `walkdir`
  traversal, file hashing, and copying — CPU and I/O intensive. Without auth, this
  is a DoS vector. The pipeline itself correctly uses `ingestion_pool` (no user
  context needed for RLS), but the HTTP entry point must be gated.
- **VALIDATE**: Unit test with test_server: POST /api/ingestion/scan returns 401
  without auth. Integration test (with DB): returns 200 for admin user.

### Task 14: Wire Routes and Models into Main

- **ACTION**: Register the new route module and model module
- **IMPLEMENT**:
  - `routes/mod.rs`: Add `pub mod ingestion;`
  - `models/mod.rs`: Add `pub mod ingestion_job;`
  - `main.rs`: Add `.merge(routes::ingestion::router())` to build_router
- **MIRROR**: ROUTER_ASSEMBLY
- **GOTCHA**: The ingestion route goes INSIDE the auth_layer (after .merge), even
  though it doesn't currently require auth. This is consistent with other routes
  and makes adding auth later trivial.
- **VALIDATE**: `cargo check`, `cargo test`, `cargo clippy -- -D warnings`

---

## Testing Strategy

### Unit Tests

| Test | File | Input | Expected Output | Edge Case? |
|---|---|---|---|---|
| format_filter: single epub | format_filter.rs | `[a.epub]`, priority `[epub]` | `[a.epub]` | No |
| format_filter: epub beats pdf | format_filter.rs | `[a.epub, a.pdf]`, priority `[epub,pdf]` | `[a.epub]` | No |
| format_filter: no match | format_filter.rs | `[a.docx]`, priority `[epub]` | `[]` | Yes |
| format_filter: case insensitive | format_filter.rs | `[a.EPUB]`, priority `[epub]` | `[a.EPUB]` | Yes |
| format_filter: multiple titles | format_filter.rs | `[a.epub, b.pdf]`, priority `[epub,pdf]` | `[a.epub, b.pdf]` | No |
| path_template: basic render | path_template.rs | `{Author}/{Title}`, vars | `Author Name/Book Title` | No |
| path_template: missing var | path_template.rs | `{Author}/{Series}/{Title}`, no series | `Author Name/Unknown/Book Title` | Yes |
| path_template: sanitize unsafe chars | path_template.rs | `Author: Name` | `Author_ Name` | Yes |
| path_template: collision resolve | path_template.rs | existing `a.epub` | `a (2).epub` | No |
| path_template: filename heuristic | path_template.rs | `Author - Title.epub` | `{Author: "Author", Title: "Title"}` | No |
| path_template: no separator | path_template.rs | `JustATitle.epub` | `{Author: "Unknown", Title: "JustATitle"}` | Yes |
| copier: successful copy | copier.rs | source file, dest dir | CopyResult with matching hash | No |
| copier: hash_file correctness | copier.rs | known content | expected SHA-256 | No |
| quarantine: move + sidecar | quarantine.rs | file, reason | file in quarantine + JSON sidecar | No |
| quarantine: collision | quarantine.rs | duplicate filename | timestamped rename | Yes |
| cleanup: removes files and dirs | cleanup.rs | nested files | all removed, root preserved | No |
| cleanup: missing file is ok | cleanup.rs | already-deleted path | no error | Yes |
| ingestion_job: status transitions | ingestion_job.rs | create -> running -> complete | correct status values | No |
| orchestrator: duplicate file skipped | mod.rs | file already ingested | job marked skipped, no error | Yes |

### Integration Tests (require DB, marked `#[ignore]`)

| Test | Input | Expected |
|---|---|---|
| Full pipeline scan | EPUBs in temp ingestion dir | Files in library dir, manifestation rows, jobs complete, cleanup done |
| Quarantine on corrupt file | Invalid zip in ingestion dir | File in quarantine, sidecar JSON, job marked failed |
| Format priority selection | `a.epub` + `a.pdf` in ingestion dir | Only `a.epub` copied, `a.pdf` cleaned up |
| Re-ingestion skip | same file ingested twice via scan_once | second run: job skipped, no duplicate manifestation, no error |

### Edge Cases Checklist

- [x] Empty ingestion directory (no files)
- [x] Files with no extension
- [x] Files with unknown extension
- [x] Duplicate filenames after path template rendering
- [x] Path-unsafe characters in filenames (unicode, special chars)
- [x] Source file disappears mid-copy (TOCTOU)
- [x] Quarantine directory doesn't exist (auto-create)
- [x] Library directory doesn't exist (auto-create)
- [x] Concurrent batches (watcher fires while processing)
- [x] Re-ingestion of already-processed files (duplicate hash/path)

---

## Validation Commands

### Static Analysis

```bash
cd backend && cargo clippy -- -D warnings
```

EXPECT: Zero warnings

### Type Check

```bash
cd backend && cargo check
```

EXPECT: Zero errors

### Unit Tests

```bash
cd backend && cargo test
```

EXPECT: All tests pass (non-ignored)

### Integration Tests (require running postgres)

```bash
cd backend && DATABASE_URL=postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev cargo test -- --ignored
```

EXPECT: All integration tests pass

### Format

```bash
cd backend && cargo fmt --check
```

EXPECT: No formatting issues

### Database Validation

```bash
cd backend && DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev sqlx migrate run
```

EXPECT: Migrations applied (new: add_skipped_job_status)

---

## Acceptance Criteria

- [ ] Filesystem watcher detects new files within 2 seconds
- [ ] Format priority correctly selects EPUB over PDF when both exist
- [ ] Path template renders correctly with filename-heuristic metadata
- [ ] SHA-256 verification passes on successful copy
- [ ] Cleanup removes source files only after verified copy of entire batch
- [ ] Quarantine captures failed files with structured JSON sidecar
- [ ] No data loss scenario possible (source preserved on any failure)
- [ ] ingestion_jobs table tracks batch status accurately
- [ ] Manual scan endpoint returns correct counts
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] cargo clippy clean, cargo fmt clean

## Completion Checklist

- [ ] Code follows discovered patterns (route/model/service structure)
- [ ] Error handling matches codebase style (thiserror enums, AppError mapping)
- [ ] Logging follows codebase conventions (tracing with structured fields)
- [ ] Tests follow test patterns (#[cfg(test)] modules, #[ignore] for DB tests)
- [ ] No hardcoded values (paths from config, priority from config)
- [ ] Dual pool architecture implemented (ingestion_pool for background, pool for web)
- [ ] Graceful shutdown stops the watcher cleanly
- [ ] No unnecessary scope additions (no metadata extraction, no validation, no enrichment)
- [ ] Self-contained -- no questions needed during implementation

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| notify v7 API instability | Low | Medium | Pin to specific version, test on Linux (inotify) |
| Cross-filesystem rename fails | Medium | Low | Copier already uses copy+verify; quarantine falls back to copy+delete |
| Large file causes memory pressure during hashing | Low | Medium | Streaming hash with 64KB buffer, never load full file into memory |
| Concurrent watcher events during processing | Medium | Low | Debounce + sequential batch processing. Concurrent batches deferred to later optimization. |
| manifestation.work_id placeholder coupling | Medium | Medium | Create minimal "Unknown" work per file. Step 6 will reconcile. Document the contract clearly. |

## Notes

- The `tome_ingestion` DB role has permissive RLS on manifestations (no user context
  needed) and full access to ingestion_jobs. This is by design -- the pipeline operates
  without user identity.
- Path template configurability (user-defined templates) is a natural follow-up but
  not in scope. The default `{Author}/{Title}.{ext}` template is hardcoded.
- The watcher processes batches sequentially (one at a time). Parallel batch processing
  is a future optimization if ingestion volume warrants it.
- Step 5 (EPUB Validation) will hook into the pipeline between copy and "mark complete"
  -- the orchestrator's per-file processing loop is the integration point.
