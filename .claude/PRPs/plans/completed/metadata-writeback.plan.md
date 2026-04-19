# Plan: Metadata Writeback to Managed EPUBs (BLUEPRINT Step 8)

## Summary

Reflect canonical metadata changes (text fields, embedded cover, on-disk path)
into the managed EPUB file so OPDS clients and device-side readers see the
same state as Reverie's web UI. Pointer moves from Step 7's `Apply` and from
accept / revert / reject / manual routes enqueue `writeback_jobs`; a background
worker drains the queue, performs file-side mutations safely under
concurrency / cross-FS / collision / post-validation regression, and updates
`current_file_hash` on success.

## User Story

As a self-hosted Reverie user,
I want my EPUB files on disk to reflect the metadata I have accepted in Reverie,
So that when my Kobo / KOReader / OPDS reader opens the file directly,
the title, author, ISBN, series, and cover all match what I curated.

## Problem → Solution

**Current state:** Step 7 moves canonical pointers and updates DB columns
(`works.title`, `manifestations.publisher`, etc.). The physical EPUB at
`${library}/.../book.epub` still carries its original OPF and original
embedded cover. Devices that ingest the EPUB directly see stale metadata.

**Desired state:** every canonical pointer move on a manifestation or its
work is reflected in the managed EPUB file (OPF text + embedded cover +
on-disk path), serialised per manifestation, durable across crashes,
rolled back if writeback regresses validation.

## Metadata

- **Complexity**: Large (~25 files, ~1500–2000 LOC across Rust + SQL + tests)
- **Source PRD**: `plans/BLUEPRINT.md`
- **PRD Phase**: Step 8 of 12 (lines 929–1281 in BLUEPRINT.md, post-adversarial-review)
- **Estimated Files**: 25 (1 refactor, 1 migration up + 1 down, 11 new Rust modules, 3 updated existing modules, 1 config, 1 env example, 6 test files, 1 main.rs wiring)
- **Branch**: `feat/metadata-writeback`
- **Depends on**: Step 7 merged. Confirms `enrichment::orchestrator::apply_field`
  and `routes/metadata::apply_version` exist and use the canonical-pointer model.
- **Tier**: Strongest (file mutation + concurrent-write race + crash-recovery
  contract + cover-image handling).

---

## UX Design

### Before
```
┌─────────────────────────────────────────────────────────┐
│  Reverie web UI: "Title: The Way of Kings"             │
│  Reverie DB:     works.title = "The Way of Kings"      │
│  Managed EPUB:   <dc:title>The Way Of Kings</dc:title> │
│                  (lowercase 'o' — original from publisher)│
│                                                         │
│  Kobo reader sees: "The Way Of Kings"  ❌ stale         │
└─────────────────────────────────────────────────────────┘
```

### After
```
┌─────────────────────────────────────────────────────────┐
│  Pointer move (any source) → writeback_jobs row inserted│
│  Background worker drains, rewrites OPF + cover         │
│  Atomic temp+rename, validates output, updates hash     │
│                                                         │
│  Reverie web UI: "Title: The Way of Kings"             │
│  Reverie DB:     works.title = "The Way of Kings"      │
│  Managed EPUB:   <dc:title>The Way of Kings</dc:title> │
│  Kobo reader sees: "The Way of Kings"  ✓ in sync       │
└─────────────────────────────────────────────────────────┘
```

### Interaction Changes

| Touchpoint | Before | After | Notes |
|---|---|---|---|
| `POST /api/manifestations/:id/metadata/accept` | Updates DB only | Updates DB + enqueues `writeback_jobs` row in same tx | Response time unchanged; user sees confirmation immediately |
| Webhook subscribers | `enrichment_complete`, `enrichment_conflict` | + `writeback_complete`, `writeback_failed` | Sync agents (Syncthing, etc.) can wait for `writeback_complete` before pulling files |
| Library Health | (Step 11) | Surfaces `current_file_hash != sha256(on_disk)` divergence + terminal `failed` writeback jobs | Step 11 wires this; Step 8 publishes the columns/rows it consumes |
| File-system layout | `${library}/_covers/pending/{...}.{ext}` | + `${library}/_covers/accepted/{...}.{ext}` (after embed succeeds) | Sidecar moves only on successful embed |

---

## Mandatory Reading

| Priority | File | Lines | Why |
|---|---|---|---|
| P0 | `plans/BLUEPRINT.md` | 929–1281 | Step 8 spec — the contract this plan implements (post-adversarial-review) |
| P0 | `backend/src/services/enrichment/queue.rs` | 1–253 | Mirror exact queue shape: claim CTE, retry backoff CASE, `cancel_token`-driven `revert_in_progress`, `mark_complete` / `mark_failed` separation |
| P0 | `backend/src/services/enrichment/orchestrator.rs` | 240–298, 561–688 | `apply_field` is the in-pipeline emission site; `Decision::Apply` arm is where to insert the `writeback_jobs` row inside the existing tx |
| P0 | `backend/src/routes/metadata.rs` | 167–309, 360–454 | Accept / revert handlers; `apply_version` helper is the route-side emission site |
| P0 | `backend/src/services/epub/repair.rs` | 1–120 | `repackage()` — extract its mimetype-first / stored / NamedTempFile / atomic-rename loop into the shared `repack::with_modifications` helper |
| P0 | `backend/src/services/epub/mod.rs` | 90–170 | `Issue`, `ValidationReport`, `validate_and_repair`, `MAX_ENTRY_UNCOMPRESSED_BYTES` — re-validation hook + size cap |
| P1 | `backend/migrations/20260417000001_add_enrichment_pipeline.up.sql` | 1–209 | Migration shape: section-numbered comments, ENUM_REBUILD pattern (DROP DEFAULT before ALTER TYPE), per-role grants block, `CREATE INDEX ... WHERE status IN ('pending', 'failed')` partial-index pattern |
| P1 | `backend/migrations/20260416000001_remove_invalid_validation_status.up.sql` | all | Canonical ENUM_REBUILD example for the `current_file_hash` rename + backfill |
| P1 | `backend/src/services/ingestion/copier.rs` | 1–110 | `NamedTempFile::new_in(dest_dir)` + `temp.persist(&final_path)` — same-FS atomic rename idiom |
| P1 | `backend/src/main.rs` | 90–130 | Spawn-pattern: clone token + pool + config, `tokio::spawn`, wire `tokio::select!` against `signal::ctrl_c` |
| P1 | `backend/src/config.rs` | 60–110 | Env-var parsing + `ConfigError::Invalid` for out-of-range values; mirror for `WritebackConfig` substructure |
| P1 | `backend/src/error.rs` | 4–38 | `AppError` variants — never invent new ones; `Internal(anyhow)` for all background-task errors |
| P1 | `backend/CLAUDE.md` | all | Backend conventions: `thiserror` at lib boundaries, `anyhow` at app boundaries, `tracing::{info,warn,error}!` with structured fields, never `println!` |
| P1 | `CLAUDE.md` | all | Conventional Commits + TDD mandate; tests before implementation |
| P2 | `backend/src/models/work.rs` | 20–130 | REPOSITORY_PATTERN + UPSERT_WITH_RETURNING; `find_or_create` opens a tx the orchestrator passes through |
| P2 | `backend/src/services/enrichment/cache.rs` | all | Per-kind TTL + upsert pattern (cover sidecar move logic mirrors this stylistically) |
| P2 | `backend/src/auth/middleware.rs` | (require_admin / require_not_child) | Worker has no user-facing surface; route-side enqueue inherits Step 7's `require_not_child` — no new auth code |
| P2 | `backend/src/services/metadata/extractor.rs` | (parse_date, OpfData usage) | Reuse `OpfData` shape for OPF reading in `opf_rewrite.rs` |
| P2 | `backend/src/services/ingestion/path_template.rs` | (render fn) | Path renderer for the path-rename worker stage |

## External Documentation

| Topic | Source | Key Takeaway |
|---|---|---|
| EPUB 3.3 Packages | `https://www.w3.org/TR/epub-33/#sec-package-doc` | `<package>` carries `version="3.0"` etc. + `unique-identifier` attribute pointing at the `<dc:identifier id="...">` to never reassign |
| EPUB 3.3 metadata | `https://www.w3.org/TR/epub-33/#sec-metadata-elem` | `<meta property="belongs-to-collection">` for series; `<meta refines="#...">` to add role attributes to creators |
| EPUB 2.0.1 OPF | `https://idpf.org/epub/20/spec/OPF_2.0.1_draft.htm` | `<dc:identifier opf:scheme="ISBN">` form; `<meta name="cover" content="cover-image-id">` for cover discovery |
| Calibre metadata extensions | `https://manual.calibre-ebook.com/metadata.html` | Calibre custom: `<meta name="calibre:series" content="...">`, `<meta name="calibre:series_index" content="N">`. Preserve when present even on EPUB 3 files |
| `quick-xml` events API | `https://docs.rs/quick-xml/latest/quick_xml/` | `Reader::from_reader` + `Writer::new`; transform via `Event::Start`/`Text`/`End`; `Event::Empty` for self-closing; preserve unknown events with `writer.write_event(event)` to round-trip |
| `quick-xml` namespaces | `https://docs.rs/quick-xml/latest/quick_xml/reader/struct.NsReader.html` | `NsReader::read_resolved_event_into` resolves namespaces — `dc:title` matched by namespace URI not prefix string |
| ZIP `mimetype` requirement | `https://www.w3.org/TR/epub-33/#sec-zip-container-mime` | First entry MUST be `mimetype`, uncompressed (Stored), no extra fields. Already implemented in `repair.rs:92-96`; preserved by the shared repack helper |
| `tempfile::NamedTempFile` cross-FS | `https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#cross-device-renames` | `persist()` returns `PersistError` containing the original temp on EXDEV; surface this to fall back to copy + verify + unlink |
| PostgreSQL `pg_advisory_xact_lock` | `https://www.postgresql.org/docs/current/explicit-locking.html#ADVISORY-LOCKS` | Considered but not used — manifestation-aware claim CTE is sufficient. Documented as alternative under Risks. |
| `image::guess_format` | `https://docs.rs/image/latest/image/fn.guess_format.html` | Already imported in Step 7's `cover_download.rs`; reuse for cover MIME-type detection in `cover_embed.rs` |

> Fetch live `quick-xml` and `zip` crate docs via the `documentation-lookup`
> skill (Context7 MCP) at implementation time — the API surface for both
> changes between minor versions and the BLUEPRINT does not pin versions.

---

## Patterns to Mirror

> Every snippet below is copied from the live codebase. File:line references
> are anchors — do not invent alternatives.

### NAMING_CONVENTION
```rust
// SOURCE: backend/src/services/enrichment/queue.rs:1-21
//! Background <subsystem> worker.
//!
//! Two-line module purpose statement at the top, then `use` blocks grouped
//! std → external → crate. snake_case modules, snake_case fns, PascalCase
//! types/enums, SCREAMING_CASE consts.
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::Config;
use super::orchestrator::{self, RunOutcome};
```

### ERROR_HANDLING
```rust
// SOURCE: backend/src/error.rs:4-38
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
// Worker-side errors are anyhow::Error (no AppError at the worker boundary —
// AppError exists for IntoResponse). Wrap sqlx errors with `.context("...")`.
```

### LIBRARY_ERROR_PATTERN (writeback's per-module errors)
```rust
// SOURCE: backend/src/services/epub/mod.rs:60-90 (EpubError shape)
#[derive(Debug, thiserror::Error)]
pub enum WritebackError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("epub: {0}")]
    Epub(#[from] super::epub::EpubError),
    #[error("post-writeback validation regressed: {0}")]
    ValidationRegressed(String),
    #[error("path collision: {0}")]
    PathCollision(std::path::PathBuf),
    #[error("manifestation has no managed file")]
    NoManagedFile,
}
```

### LOGGING_PATTERN
```rust
// SOURCE: backend/src/services/enrichment/queue.rs:46-50, 165
tracing::info!(
    concurrency,
    poll_idle_secs = config.writeback.poll_idle_secs,
    "writeback queue started"
);
tracing::warn!(error = %e, %manifestation_id, "writeback: post-validation regressed");
// never println! / eprintln! — structured `field = value` pairs
```

### REPOSITORY_PATTERN
```rust
// SOURCE: backend/src/models/work.rs:20-39
pub async fn find_or_create(
    pool: &PgPool,
    metadata: &ExtractedMetadata,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // ... query work, then insert if missing ...
    tx.commit().await?;
    Ok(work_id)
}
```

### CLAIM_CTE (mirror exactly — `claim_next` shape)
```rust
// SOURCE: backend/src/services/enrichment/queue.rs:90-125
async fn claim_next(pool: &PgPool) -> sqlx::Result<Option<(Uuid, i32)>> {
    let row: Option<(Uuid, i32)> = sqlx::query_as(
        r#"WITH eligible AS (
             SELECT id, attempt_count
             FROM writeback_jobs wj
             WHERE status IN ('pending', 'failed')
               AND NOT EXISTS (
                 SELECT 1 FROM writeback_jobs
                 WHERE manifestation_id = wj.manifestation_id
                   AND status = 'in_progress'
               )
               AND (
                 last_attempted_at IS NULL
                 OR last_attempted_at <
                      now() - (CASE
                        WHEN attempt_count <= 0 THEN INTERVAL '0 minutes'
                        WHEN attempt_count = 1 THEN INTERVAL '5 minutes'
                        WHEN attempt_count = 2 THEN INTERVAL '30 minutes'
                        WHEN attempt_count = 3 THEN INTERVAL '2 hours'
                        WHEN attempt_count = 4 THEN INTERVAL '8 hours'
                        ELSE INTERVAL '24 hours'
                      END)
               )
             ORDER BY last_attempted_at NULLS FIRST, created_at
             LIMIT 1
             FOR UPDATE SKIP LOCKED
           )
           UPDATE writeback_jobs wj
              SET status = 'in_progress',
                  last_attempted_at = now(),
                  attempt_count = wj.attempt_count + 1
             FROM eligible
            WHERE wj.id = eligible.id
           RETURNING wj.id, wj.attempt_count"#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
// Manifestation-aware NOT EXISTS clause is the single change vs Step 7's queue.
// Memory instinct (global 90%): one CTE with FOR UPDATE SKIP LOCKED — never
// separate count + insert.
```

### REVERT_IN_PROGRESS_ON_SHUTDOWN
```rust
// SOURCE: backend/src/services/enrichment/queue.rs:236-253
async fn revert_in_progress(pool: &PgPool) -> sqlx::Result<()> {
    let res = sqlx::query(
        "UPDATE writeback_jobs SET status = 'pending' \
         WHERE status = 'in_progress'",
    )
    .execute(pool)
    .await?;
    if res.rows_affected() > 0 {
        tracing::info!(count = res.rows_affected(), "reverted in_progress jobs to pending");
    }
    Ok(())
}
```

### CANCELLATION_TOKEN_LOOP
```rust
// SOURCE: backend/src/services/enrichment/queue.rs:52-83
loop {
    tokio::select! {
        _ = cancel.cancelled() => {
            info!("writeback queue shutting down");
            revert_in_progress(&pool).await?;
            return Ok(());
        }
        _ = interval.tick() => {
            loop {
                let permit = match semaphore.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => break,
                };
                let claim = claim_next(&pool).await?;
                let Some((id, attempt_count)) = claim else { drop(permit); break; };
                let pool = pool.clone();
                let cfg = config.clone();
                tokio::spawn(async move {
                    let _p = permit;
                    let result = orchestrator::run_once(&pool, &cfg, id).await;
                    if let Err(e) = finish(&pool, &cfg, id, attempt_count, result).await {
                        warn!(error = %e, %id, "writeback: finish bookkeeping failed");
                    }
                });
            }
        }
    }
}
```

### ATOMIC_RENAME (same-FS happy path)
```rust
// SOURCE: backend/src/services/ingestion/copier.rs:75-105
let temp = NamedTempFile::new_in(dest_dir)?;
// ... write modifications into temp ...
temp.persist(&final_path)?; // atomic on same FS
```

### ATOMIC_RENAME_WITH_EXDEV_FALLBACK (new pattern Step 8 introduces)
```rust
// USED BY: src/services/writeback/path_rename.rs (task 9)
fn atomic_move_with_fallback(temp: NamedTempFile, dest: &Path) -> Result<(), WritebackError> {
    match temp.persist(dest) {
        Ok(_) => Ok(()),
        Err(persist_err) if persist_err.error.kind() == std::io::ErrorKind::CrossesDevices
            // Some kernels return Other instead — check raw_os_error == EXDEV (18) too
            || persist_err.error.raw_os_error() == Some(18) =>
        {
            // Recover the underlying NamedTempFile and copy + verify + unlink
            let temp = persist_err.file;
            let temp_path = temp.path().to_path_buf();
            let bytes = std::fs::read(&temp_path)?;
            let src_hash = sha2::Sha256::digest(&bytes);
            // Write into dest's directory via fresh tempfile, then rename
            let new_temp = NamedTempFile::new_in(dest.parent().unwrap_or(Path::new(".")))?;
            std::fs::write(new_temp.path(), &bytes)?;
            // fsync the new file before rename
            let f = std::fs::File::open(new_temp.path())?;
            f.sync_all()?;
            new_temp.persist(dest)?;
            // Verify the final file matches the temp we wrote
            let dest_bytes = std::fs::read(dest)?;
            let dest_hash = sha2::Sha256::digest(&dest_bytes);
            if dest_hash.as_slice() != src_hash.as_slice() {
                return Err(WritebackError::Io(std::io::Error::other("post-copy hash mismatch")));
            }
            // temp is dropped → original temp file is unlinked
            Ok(())
        }
        Err(e) => Err(WritebackError::Io(e.error)),
    }
}
```

### REPACK_HELPER (new pattern Step 8 extracts from repair.rs)
```rust
// CREATE: src/services/epub/repack.rs (task 1, refactor)
// Extracted from: backend/src/services/epub/repair.rs:86-128
//
// Both `repair::repackage` and `writeback::orchestrator` call this helper.
//
// `opf_replacement` — if Some, replaces the OPF entry's bytes.
// `binary_replacements` — entry-name → bytes overrides for any other entry
//   (e.g. cover image). New entries are appended.
//
// Mimetype is always written first as Stored. NamedTempFile in the same
// directory as `path`. Caller is responsible for the final atomic rename
// (so writeback can write into the *new* path's directory under path-rename).
pub fn with_modifications(
    src_path: &Path,
    dest_dir: &Path,
    opf_path: Option<&str>,
    opf_replacement: Option<&[u8]>,
    binary_replacements: &HashMap<String, Vec<u8>>,
    additions: &[(String, Vec<u8>, FileOptions<ExtendedFileOptions>)],
) -> Result<NamedTempFile, EpubError> {
    let bytes = std::fs::read(src_path)?;
    let temp = NamedTempFile::new_in(dest_dir)?;
    {
        let cursor = std::io::Cursor::new(&bytes[..]);
        let mut archive = ZipArchive::new(cursor)?;
        let mut writer = ZipWriter::new(&temp);

        // mimetype FIRST and stored — EPUB spec hard requirement.
        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file(MIMETYPE_ENTRY, stored)?;
        writer.write_all(MIMETYPE_CONTENT)?;

        // Copy / replace entries.
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_string();
            if name == MIMETYPE_ENTRY { continue; }

            let bytes_to_write: Vec<u8> = if Some(name.as_str()) == opf_path && opf_replacement.is_some() {
                opf_replacement.unwrap().to_vec()
            } else if let Some(b) = binary_replacements.get(&name) {
                b.clone()
            } else {
                let mut buf = Vec::new();
                file.take(super::MAX_ENTRY_UNCOMPRESSED_BYTES + 1).read_to_end(&mut buf)?;
                buf
            };

            // Preserve the original entry's compression method.
            let opts: FileOptions<ExtendedFileOptions> =
                FileOptions::default().compression_method(file.compression());
            writer.start_file(&name, opts)?;
            writer.write_all(&bytes_to_write)?;
        }

        // Append new entries (cover insertion case).
        for (name, bytes, opts) in additions {
            writer.start_file(name, opts.clone())?;
            writer.write_all(bytes)?;
        }
        writer.finish()?;
    }
    Ok(temp)
}
```

### MIGRATION_NAMING + STRUCTURE
```text
backend/migrations/20260419000001_add_writeback_pipeline.up.sql
backend/migrations/20260419000001_add_writeback_pipeline.down.sql
# UTC date today; six-digit serial higher than the last migration
# (20260417000002 → next free is 20260419000001).
```

### MIGRATION_SECTION_HEADERS
```sql
-- SOURCE: backend/migrations/20260417000001_add_enrichment_pipeline.up.sql:39-48
---------------------------------------------------------------------------
-- N. Section title
---------------------------------------------------------------------------
-- Free-form rationale, especially for irreversible operations
```

### PER_ROLE_GRANTS
```sql
-- SOURCE: backend/migrations/20260417000001_add_enrichment_pipeline.up.sql:200-206
GRANT SELECT, INSERT, UPDATE, DELETE ON writeback_jobs TO reverie_app;
GRANT SELECT                          ON writeback_jobs TO reverie_readonly;
-- Worker runs on reverie_app pool. reverie_ingestion has no writeback role —
-- ingestion never writes back to managed files (invariant 8).
```

### COLUMN_RENAME_WITH_BACKFILL
```sql
-- SOURCE PATTERN: backend/migrations/20260416000001_remove_invalid_validation_status.up.sql
ALTER TABLE manifestations RENAME COLUMN file_hash TO ingestion_file_hash;
ALTER TABLE manifestations
    ADD COLUMN current_file_hash BYTEA NOT NULL DEFAULT '\x'::bytea;
UPDATE manifestations SET current_file_hash = ingestion_file_hash;
ALTER TABLE manifestations ALTER COLUMN current_file_hash DROP DEFAULT;
-- Comment column meanings — Step 11 health depends on the distinction.
COMMENT ON COLUMN manifestations.ingestion_file_hash IS
  'SHA-256 of file at ingestion time. Immutable audit trail.';
COMMENT ON COLUMN manifestations.current_file_hash IS
  'SHA-256 of file as of last successful writeback. Equals ingestion_file_hash until first writeback.';
```

### TEST_STRUCTURE (DB integration — same as Step 7)
```rust
// SOURCE: backend/src/services/enrichment/queue.rs:255-281
fn db_url() -> String {
    std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
        "postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev".into()
    })
}

#[tokio::test]
#[ignore] // requires PostgreSQL with migrations applied
async fn writeback_job_emitted_on_apply() {
    let pool = PgPool::connect(&db_url()).await.unwrap();
    let (work_id, manifestation_id) = setup_manifestation(&pool).await;
    // ...
    cleanup(&pool, work_id, manifestation_id).await;
}

// Defuse stale rows from prior test runs.
async fn quiesce_writeback_jobs(pool: &PgPool) {
    let _ = sqlx::query(
        "UPDATE writeback_jobs SET status = 'complete' \
         WHERE status IN ('pending', 'failed', 'in_progress')",
    ).execute(pool).await;
}
```

### TEST_STRUCTURE (file-system integration)
```rust
// SOURCE: backend/src/services/epub/repair.rs:280-340
// Build a known-good EPUB in-memory using `zip::ZipWriter`, write to a
// `tempfile::NamedTempFile`, run the function under test against it,
// re-open with `zip::ZipArchive` to assert post-state.
#[tokio::test]
async fn writeback_replaces_dc_title() {
    let temp = NamedTempFile::new().unwrap();
    write_minimal_epub(temp.path(), |opf| {
        opf.set_title("Old Title").set_creator("Author");
    });
    // ... run writeback pointing at temp.path() ...
    let bytes = std::fs::read(temp.path()).unwrap();
    let mut ar = ZipArchive::new(Cursor::new(&bytes[..])).unwrap();
    let mut opf = String::new();
    ar.by_name("OEBPS/content.opf").unwrap().read_to_string(&mut opf).unwrap();
    assert!(opf.contains("<dc:title>New Title</dc:title>"));
}
```

### TWO_WORKER_RACE_HARNESS
```rust
// SOURCE PATTERN: backend/src/services/enrichment/queue.rs::tests (the
// two-worker race test from Step 7 task 35)
#[tokio::test]
#[ignore]
async fn two_workers_for_same_manifestation_serialise() {
    let pool = PgPool::connect(&db_url()).await.unwrap();
    let mid = setup_manifestation(&pool).await;
    // Insert 5 pending jobs for the same manifestation
    for _ in 0..5 {
        sqlx::query("INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata')")
            .bind(mid).execute(&pool).await.unwrap();
    }
    // Spawn two claim_next concurrently — only one should claim an in_progress
    let (a, b) = tokio::join!(claim_next(&pool), claim_next(&pool));
    let claims = [a.unwrap(), b.unwrap()];
    let claimed = claims.iter().filter(|x| x.is_some()).count();
    assert_eq!(claimed, 1, "manifestation-aware claim must serialise");
}
```

---

## Files to Change

| File | Action | Justification |
|---|---|---|
| `backend/Cargo.toml` | UPDATE | Add `quick-xml` (with `serialize` feature off; events API only) |
| `backend/migrations/20260419000001_add_writeback_pipeline.up.sql` | CREATE | `writeback_jobs` table + status enum + claim index + `ingestion_file_hash` rename + `current_file_hash` add + grants |
| `backend/migrations/20260419000001_add_writeback_pipeline.down.sql` | CREATE | Clean reversal — note `writeback_status` enum cannot be dropped if any rows reference it |
| `backend/src/services/epub/repack.rs` | CREATE | Shared `with_modifications` helper extracted from `repair.rs` (task 1) |
| `backend/src/services/epub/repair.rs` | UPDATE | Replace inlined ZIP loop with call to `repack::with_modifications` (task 1) |
| `backend/src/services/epub/mod.rs` | UPDATE | `pub mod repack;` |
| `backend/src/services/writeback/mod.rs` | CREATE | Module root; `pub use queue::spawn_worker; pub use orchestrator::run_once;` |
| `backend/src/services/writeback/queue.rs` | CREATE | Background worker — claim CTE, retry backoff, shutdown revert (task 5) |
| `backend/src/services/writeback/orchestrator.rs` | CREATE | Per-job flow: snapshot → OPF rewrite → cover embed → repack → atomic rename → re-validate → rollback if regressed → bookkeeping (task 6) |
| `backend/src/services/writeback/opf_rewrite.rs` | CREATE | `quick-xml` event-stream OPF transform (task 7) |
| `backend/src/services/writeback/cover_embed.rs` | CREATE | Locate-or-insert cover manifest item; binary entry replacement (task 8) |
| `backend/src/services/writeback/path_rename.rs` | CREATE | Render template, detect collision, EXDEV-aware atomic move (task 9) |
| `backend/src/services/writeback/error.rs` | CREATE | `WritebackError` thiserror enum (mirrors `EpubError` shape) |
| `backend/src/services/mod.rs` | UPDATE | `pub mod writeback;` (and `pub mod epub::repack;` re-export if needed) |
| `backend/src/services/enrichment/orchestrator.rs` | UPDATE | After `apply_field` returns Ok, INSERT a `writeback_jobs` row in the same `tx` (task 3a) |
| `backend/src/routes/metadata.rs` | UPDATE | After `apply_version` returns Ok, INSERT a `writeback_jobs` row in the same `tx`. Same for revert + cover accept routes (task 3b) |
| `backend/src/config.rs` | UPDATE | Add `WritebackConfig` substructure with 4 env vars (task 11) |
| `backend/src/main.rs` | UPDATE | Spawn `services::writeback::spawn_worker` alongside enrichment queue (task 10) |
| `backend/src/services/webhooks/...` | UPDATE | Add `writeback_complete` and `writeback_failed` to event enum (task 12). Step 12 owns the webhook subsystem; this task plugs in the new variants. |
| `.env.example` | UPDATE | Add `REVERIE_WRITEBACK_*` vars (task 11) |
| `backend/src/services/writeback/queue.rs` `#[cfg(test)]` | TEST | Two-worker race + retry backoff + shutdown revert + max-attempts (task 14, 22) |
| `backend/src/services/writeback/opf_rewrite.rs` `#[cfg(test)]` | TEST | EPUB 2 vs 3, multiple `<dc:identifier>`, custom `<meta>` preservation, OPF path discovery, ISBN insertion (tasks 15–18) |
| `backend/src/services/writeback/cover_embed.rs` `#[cfg(test)]` | TEST | Replace-existing, insert-when-absent EPUB 2 + EPUB 3, sidecar move (task 19) |
| `backend/src/services/writeback/path_rename.rs` `#[cfg(test)]` | TEST | Same-dir, cross-dir same-FS, cross-dir cross-FS (mocked), collision (task 20) |
| `backend/src/services/writeback/orchestrator.rs` `#[cfg(test)]` | TEST | Post-validation rollback; current_file_hash update; `ingestion_file_hash` immutability (tasks 21, 24) |

## NOT Building

- **Calibre cover-page XHTML rewrite** — when a new cover image replaces an
  old one, downstream readers re-render the cover from the manifest item.
  We do NOT modify any cover-page XHTML that may exist as a separate file
  for older readers — out of MVP scope.
- **Cover-cleanup sweep of `_covers/accepted/`** — orphaned accepted covers
  (covers whose manifestation was deleted) are left in place. Step 11 sweep.
- **Re-running enrichment after writeback** — writeback does not re-trigger
  the enrichment queue. The DB pointer is the source of truth post-writeback.
- **User-configurable writeback fields** — the field set written back is
  hard-coded (matches the per-field set Step 7's `apply_field` already handles
  + cover). Future "exclude this field from writeback" UI is Phase 2.
- **EPUB 3 alternate metadata refinements** (`<meta refines="#x" property="...">`
  for granular role / scheme / display annotations) — preserved if present
  but not authored by Step 8. Authoring refinements is Phase 2.
- **OPDS in-flight invalidation** — the brief "old content at new path"
  window during cross-FS rename is acceptable for MVP. OPDS clients re-request
  with ETag and get the new content; no server-push invalidation.
- **Multi-replica deploy testing** — the design is multi-replica safe
  (manifestation-aware claim CTE), but the MVP only ships a single replica.
- **Frontend writeback status surface** — Step 11 (Library Health) consumes
  `writeback_jobs` rows + `current_file_hash != on_disk_hash` divergence;
  Step 8 does not add UI.
- **Writeback for non-EPUB formats** — only `format = 'epub'` manifestations
  are processed. Other formats (CBZ, PDF, etc.) skip writeback with `status='skipped'`,
  `reason='format_unsupported'`. MVP only ingests EPUB anyway.

---

## Step-by-Step Tasks

### Task 1: Refactor — extract `epub::repack::with_modifications`
- **STATUS**: Pending.
- **ACTION**: Factor out `repair.rs:86-128`'s ZIP-rewriting loop into
  `src/services/epub/repack.rs::with_modifications`. Replace `repair::repackage`'s
  inlined loop with a call to the new helper. No behaviour change.
- **IMPLEMENT**:
  - New module `src/services/epub/repack.rs` exposing `with_modifications` per
    REPACK_HELPER pattern above.
  - Helper signature accepts `dest_dir: &Path` (where the temp file lives) so
    callers writing into the *new* path's directory under path-rename can
    pass that directly.
  - Helper RETURNS `NamedTempFile` (not `()`); caller is responsible for the
    final atomic rename so writeback can decide between persist-in-place vs
    persist-to-new-path.
  - `repair::repackage` becomes: build `opf_replacement: Option<&[u8]>` from
    its existing `rewritten_opf` calculation; build `binary_replacements:
    HashMap<String, Vec<u8>>` from `encoding_fixes`; call helper; persist
    returned tempfile over the original path.
- **MIRROR**: NAMING_CONVENTION; existing `repair.rs:86-128` is the source.
- **IMPORTS**: `std::collections::HashMap`, `tempfile::NamedTempFile`,
  `zip::write::{ExtendedFileOptions, FileOptions}`, `zip::{ZipArchive, ZipWriter}`.
- **GOTCHA**: The shared helper must preserve **per-entry compression method**
  (currently `repair::repackage` resets every entry to default deflate — that's
  a latent bug; preserve the original `file.compression()` instead). Step 5's
  pre-existing tests don't cover this; they pass either way. Add a regression
  test in `repack.rs` that verifies a Stored entry stays Stored after round-trip.
- **VALIDATE**: `cargo test services::epub::repair` (existing tests) and the
  new `cargo test services::epub::repack` (round-trip + compression-preserve)
  both pass.

### Task 2: Migration — `add_writeback_pipeline`
- **STATUS**: Pending.
- **ACTION**: Create up + down migrations.
- **IMPLEMENT**:
  - Section 1: `CREATE TYPE writeback_status AS ENUM ('pending','in_progress','complete','failed','skipped');`
  - Section 2: `CREATE TABLE writeback_jobs (...)` per BLUEPRINT lines 956-981 + the partial claim index `WHERE status IN ('pending','failed')`.
  - Section 3: COLUMN_RENAME_WITH_BACKFILL pattern for `manifestations.file_hash` → `ingestion_file_hash` + new `current_file_hash`. Add `COMMENT ON COLUMN` for both.
  - Section 4: PER_ROLE_GRANTS for `writeback_jobs` (`reverie_app` full, `reverie_readonly` SELECT). Note in a comment that `reverie_ingestion` has no grant — invariant 8.
  - `.down.sql`: drop the table and the partial index, drop the type, restore `file_hash` column name (best-effort: rename `ingestion_file_hash` back, drop `current_file_hash`). Leading comment notes the type cannot be dropped if any `writeback_jobs` rows reference it — same hazard pattern as Step 5's `'degraded'`.
- **MIRROR**: MIGRATION_NAMING; MIGRATION_SECTION_HEADERS; PER_ROLE_GRANTS; COLUMN_RENAME_WITH_BACKFILL.
- **GOTCHA**: Allocate the timestamp serial higher than `20260417000002` —
  next free is `20260419000001`. Test the down migration on a DB with one
  `complete` `writeback_jobs` row first to confirm the type-drop fails
  cleanly with a clear error message.
- **VALIDATE**: Migration round-trip per the Validation section.

### Task 3a: Job emission — enrichment orchestrator
- **STATUS**: Pending.
- **ACTION**: Extend `services::enrichment::orchestrator::run_once` to insert
  a `writeback_jobs` row whenever `apply_field` returns Ok inside the existing
  transaction. One job per Apply (deduplication is the worker's job, not the
  emitter's).
- **IMPLEMENT**:
  - Inside the `Decision::Apply(version_id) => { ... }` arm at orchestrator.rs:254-274,
    after the `if field == "isbn_10" || field == "isbn_13"` block and before
    the `break;`, INSERT:
    ```rust
    sqlx::query(
        "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, $2)"
    )
    .bind(manifestation_id)
    .bind(if is_cover_field(field) { "cover" } else { "metadata" })
    .execute(&mut *tx)
    .await?;
    ```
  - Add `fn is_cover_field(f: &str) -> bool { f == "cover" || f == "cover_url" }`.
  - The same tx already commits the canonical update — both succeed or both fail.
- **MIRROR**: REPOSITORY_PATTERN (write inside `tx`).
- **IMPORTS**: none new.
- **GOTCHA**: Don't insert for `Decision::Stage` or `Decision::NoOp`. Cover
  fields under Step 7 are `auto-fill` so they typically Apply on first observation
  → still need the job.
- **VALIDATE**: Task 14 (job-emission integration test): single `apply_field`
  call inserts exactly one `writeback_jobs` row referencing the same
  `manifestation_id`.

### Task 3b: Job emission — accept / revert routes
- **STATUS**: Pending.
- **ACTION**: Extend `routes/metadata::apply_version` (the helper called by
  both accept and revert) to insert a `writeback_jobs` row inside the
  caller's tx. Reject path does NOT enqueue (canonical didn't change).
- **IMPLEMENT**:
  - Add a final step inside `apply_version` (metadata.rs:360-454):
    ```rust
    sqlx::query(
        "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata')"
    )
    .bind(manifestation_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    ```
  - Same insert for `clear_field` (revert with `version_id=null`) — the canonical
    pointer cleared still requires writeback to reflect "field absent" in the OPF.
  - Cover-accept routes (added by Step 7's policy if they exist as separate
    routes; otherwise covered by `apply_version` for the `cover` field) use
    `reason='cover'`.
- **MIRROR**: same `tx` pattern as the accept handler (metadata.rs:178-211).
- **GOTCHA**: `apply_version` is also called from `revert_manifestation`
  (metadata.rs:290-298) — both call sites benefit from the single insert
  because it lives inside the helper. **Do not also insert at the call sites**
  — that would double-enqueue.
- **VALIDATE**: Task 14 — accept route ⇒ exactly one new `writeback_jobs` row;
  revert route ⇒ same; reject route ⇒ no new row.

### Task 4: Writeback module root
- **STATUS**: Pending.
- **ACTION**: Create `src/services/writeback/mod.rs`.
- **IMPLEMENT**:
  ```rust
  //! Background metadata writeback to managed EPUB files.
  //!
  //! Triggered by canonical pointer moves (Step 7's `apply_field` and the
  //! accept/revert routes) via the `writeback_jobs` queue. Worker processes
  //! jobs outside any user-facing transaction.

  pub mod cover_embed;
  pub mod error;
  pub mod opf_rewrite;
  pub mod orchestrator;
  pub mod path_rename;
  pub mod queue;

  pub use error::WritebackError;
  pub use orchestrator::run_once;
  pub use queue::spawn_worker;
  ```
- **MIRROR**: SERVICE_PATTERN (module root + public API) from
  `backend/src/services/enrichment/mod.rs`.
- **VALIDATE**: `cargo build`; module compiles after the per-submodule tasks land.

### Task 5: Worker queue (`writeback/queue.rs`)
- **STATUS**: Pending.
- **ACTION**: Implement the background worker.
- **IMPLEMENT**: Mirror `enrichment::queue.rs` end-to-end with two changes:
  1. Claim CTE adds the manifestation-aware `NOT EXISTS` clause per CLAIM_CTE.
  2. Calls `super::orchestrator::run_once(&pool, &cfg, job_id)` which returns
     `Result<RunOutcome, WritebackError>`. `RunOutcome` carries `manifestation_id`
     (looked up from the job row), `success: bool`, `reason: String`, and
     `webhook_event` (one of `writeback_complete | writeback_failed`).
  3. `mark_complete(pool, job_id)` updates `writeback_jobs SET status='complete', completed_at=now()`.
  4. `mark_failed(pool, job_id, attempt_count, config, error)` mirrors Step 7
     including max-attempts → `'skipped'` (rename to `'failed'` here per BLUEPRINT
     decision: terminal failure means "we tried" not "we chose not to").
     **Decision**: keep Step 7's `'skipped'` semantic for max-attempts to share
     the enum vocabulary; document in a code comment that here it means
     "exhausted retries".
  5. Webhook emission lives in `orchestrator::run_once` (so it has the success/
     failure context); the queue is purely lifecycle bookkeeping.
- **MIRROR**: CLAIM_CTE, CANCELLATION_TOKEN_LOOP, REVERT_IN_PROGRESS_ON_SHUTDOWN.
- **IMPORTS**: same as `enrichment/queue.rs`.
- **GOTCHA**: Polling cadence — Step 7 polls every `poll_idle_secs` (default 30s)
  but the user expects writeback to feel "near-real-time" after an accept.
  Trade-off: drop default to `REVERIE_WRITEBACK_POLL_IDLE_SECS=5`. Higher
  CPU on idle systems but the worker only runs the claim CTE + early-out on
  empty queue; cheap. Configurable so users can tune.
- **VALIDATE**: Task 22 (queue tests) — two-worker race, retry backoff,
  shutdown revert, max-attempts.

### Task 6: Worker orchestrator (`writeback/orchestrator.rs`)
- **STATUS**: Pending.
- **ACTION**: Per-job flow.
- **IMPLEMENT**:
  1. Load the job row and its manifestation:
     ```sql
     SELECT wj.id, wj.manifestation_id, wj.reason,
            m.file_path, m.format, m.work_id,
            m.cover_version_id, m.cover_path,
            m.publisher, m.publisher_version_id,
            m.pub_date, m.pub_date_version_id,
            m.isbn_10, m.isbn_10_version_id,
            m.isbn_13, m.isbn_13_version_id,
            w.title, w.title_version_id,
            w.description, w.description_version_id,
            w.language, w.language_version_id
       FROM writeback_jobs wj
       JOIN manifestations m ON m.id = wj.manifestation_id
       JOIN works w          ON w.id = m.work_id
      WHERE wj.id = $1
     ```
  2. Skip-with-`'skipped'` if `format != 'epub'` or `file_path` is NULL or the
     file does not exist.
  3. Snapshot pre-writeback bytes: `let original_bytes = std::fs::read(&file_path)?;`
  4. Snapshot pre-writeback validation: `let pre_report = epub::validate_and_repair(&file_path)?;`
  5. Discover OPF path via `META-INF/container.xml` parser (reuse Step 5's helper).
  6. Build OPF replacement bytes via `opf_rewrite::transform(opf_bytes, target)`
     where `target` carries the canonical scalars + their `*_version_id`s.
  7. Build cover replacement (if `reason ∈ {cover, metadata+cover}`) via
     `cover_embed::plan_embed(opf_bytes, cover_path, cover_format)` returning
     `(Option<HashMap<String, Vec<u8>>>, Option<additions>)` for the manifest
     entries to overwrite/insert.
  8. Compute new path via `path_rename::render(template, work, manifestation)`.
     If new path differs from current, the temp file is created in the new
     path's directory; otherwise in the current directory.
  9. Call `epub::repack::with_modifications(...)` → returns `NamedTempFile`.
  10. `path_rename::commit(temp, &new_path)` performs the EXDEV-aware atomic
      rename per ATOMIC_RENAME_WITH_EXDEV_FALLBACK; surfaces `PathCollision`
      if `new_path` already exists.
  11. If new path != original path, unlink the original file.
  12. Re-validate: `let post_report = epub::validate_and_repair(&new_path)?;`
  13. If `post_report.outcome == Quarantined`, or it carries an issue not
      present in `pre_report.issues`, ROLLBACK the file:
      - Write `original_bytes` to a temp at the original path's directory,
        atomic rename over the new path. If new path != original path, also
        atomic-rename back to the original path; we do NOT rename back from new path.
      - Mark the job `failed` with `error="post_writeback_validation_regressed: <issue>"`.
      - DO NOT touch the canonical pointers — they stay pointing at the new value.
      - Sidecar cover is NOT moved from `pending/` → `accepted/`.
  14. On success:
      - `UPDATE manifestations SET current_file_hash = sha256_of_new_file, file_path = new_path WHERE id = manifestation_id`.
      - If reason includes cover, move the sidecar from
        `_covers/pending/{mid}-{vid_short}.{ext}` to `_covers/accepted/{mid}-{vid_short}.{ext}`
        using std::fs::rename (or copy+unlink on EXDEV).
  15. Emit webhook event:
      - Success → `writeback_complete` with payload
        `{ manifestation_id, reason, attempt_count, current_file_hash }`.
      - Terminal failure → `writeback_failed` with `{ manifestation_id, reason, attempt_count, error }`.
  16. Return `RunOutcome { success, webhook_event, reason }` to the queue.
- **MIRROR**: REPOSITORY_PATTERN; LIBRARY_ERROR_PATTERN.
- **IMPORTS**: `crate::services::epub::{self, repack, validate_and_repair}`,
  `crate::services::ingestion::path_template`, `sha2::{Digest, Sha256}`,
  `tempfile::NamedTempFile`, `uuid::Uuid`.
- **GOTCHA 1**: The orchestrator does NOT take a tx — each step opens its
  own tx as needed. The job claim already advanced status to `in_progress`;
  finalisation happens in queue::finish.
- **GOTCHA 2**: `original_bytes` may be large (10-100 MB EPUBs are real).
  Memory-spike consideration accepted because the alternative — staging the
  rollback file on disk — adds another atomic-rename failure mode and most
  EPUBs are < 50 MB. Document in module-level comments.
- **GOTCHA 3**: The `current_file_hash` UPDATE happens AFTER the file is in
  place at `new_path`. If a crash happens between rename and UPDATE,
  `current_file_hash` is stale; Step 11 health surfaces the divergence.
  This is by design — the file-side state is the authoritative truth for
  "what the user sees on devices".
- **VALIDATE**: Tasks 21 (rollback) and 24 (hash columns).

### Task 7: OPF rewrite (`writeback/opf_rewrite.rs`)
- **STATUS**: Pending.
- **ACTION**: Pure function `transform(opf_bytes: &[u8], target: &Target) -> Result<Vec<u8>, WritebackError>`.
- **IMPLEMENT**:
  - `Target` carries: `title`, `description`, `language`, `publisher`, `pub_date`,
    `isbn_10`, `isbn_13`, `creators: Vec<Creator>`, `series: Option<SeriesRef>`,
    `subjects: Vec<String>`. Each Option indicates "leave the OPF's current
    value alone if None"; `Some` means write this exact value (or remove if "").
  - Read `<package version="...">` first to detect EPUB 2 vs 3; record the
    `unique-identifier` attribute so we never reassign it.
  - Stream events via `quick_xml::Reader::from_reader`. Emit unchanged events
    via `writer.write_event(event)`. For each `Event::Start("dc:title")`
    (or namespace-resolved equivalent), consume until matching `Event::End`,
    write the new value. Same for the other DC fields.
  - `<dc:identifier opf:scheme="ISBN">` (case-insensitive on `opf:scheme`):
    update if found; if absent and `target.isbn_13` is Some, INSERT a new
    element after the last existing `<dc:identifier>`. Never reassign the
    `unique-identifier` attribute on `<package>`.
  - Series:
    - EPUB 3 → `<meta property="belongs-to-collection" id="series-1">{name}</meta>`
      + refinement `<meta refines="#series-1" property="collection-type">series</meta>`
      + `<meta refines="#series-1" property="group-position">{index}</meta>`.
    - EPUB 2 → `<meta name="calibre:series" content="{name}"/>`
      + `<meta name="calibre:series_index" content="{index}"/>`.
    - If file is EPUB 3 but already has `calibre:series`, update both forms
      (preserve downstream-reader compatibility).
  - Preserve every other element (custom `<meta>`, namespaces, prologue,
    declaration order). Test 18 enforces this.
  - Output is canonical: same encoding declaration, same root element order.
- **MIRROR**: Pure-function pattern from `services/enrichment/value_hash.rs`.
- **IMPORTS**: `quick_xml::{Reader, Writer, events::Event}`,
  `quick_xml::name::ResolveResult`, `std::io::Cursor`.
- **GOTCHA 1**: `quick-xml`'s namespace handling: use `NsReader` to compare by
  namespace URI (`http://purl.org/dc/elements/1.1/` for `dc:`, not the prefix
  string — the prefix can be redefined per-document).
- **GOTCHA 2**: Self-closing vs paired tags. `Event::Empty(<dc:title/>)` is
  legal but rare for DC; if encountered, expand to paired `Event::Start` +
  `Event::Text` + `Event::End`.
- **GOTCHA 3**: ISBN insertion must happen INSIDE `<metadata>...</metadata>`,
  not anywhere else. Track the depth + element name when reading.
- **GOTCHA 4**: Memory instinct — never use string substitution / regex on XML.
  All transforms go through `quick-xml` events.
- **VALIDATE**: Tasks 15–18 (EPUB 2/3 matrix, multiple identifiers, custom
  meta preservation, OPF path discovery).

### Task 8: Cover embed (`writeback/cover_embed.rs`)
- **STATUS**: Pending.
- **ACTION**: Locate-or-insert cover manifest item; binary entry replacement plan.
- **IMPLEMENT**:
  - `plan_embed(opf_bytes: &[u8], new_cover_bytes: &[u8])` returns:
    - `binary_replacements: HashMap<String, Vec<u8>>` — entry-name → new bytes
      for the existing cover image entry, if found.
    - `additions: Vec<(String, Vec<u8>, FileOptions)>` — new ZIP entries to
      append for the insertion case (single image entry with deflate compression).
    - `opf_replacement: Vec<u8>` — OPF transformed to either update the existing
      cover entry's media-type (if it changed) or insert a new manifest item +
      `<meta name="cover" content="cover-image">` for EPUB 2 / `properties="cover-image"`
      on the manifest item for EPUB 3.
  - Cover detection precedence:
    1. EPUB 3: manifest item with `properties="cover-image"`.
    2. EPUB 2: `<meta name="cover" content="X"/>` → manifest item with `id="X"`.
    3. Fallback: manifest item with `id="cover-image"` or `id="cover"`.
  - Detect new cover format via `image::guess_format` on `new_cover_bytes`.
    Map to media-type (`image/jpeg`, `image/png`, `image/webp`).
  - When inserting a new entry, name is `images/cover-image.{ext}` (or
    `OEBPS/images/...` if existing manifest items are rooted under `OEBPS/`).
  - Use Stored compression for already-compressed formats (JPEG, WebP),
    Deflate for PNG (sometimes squeezes a little).
- **MIRROR**: Pure-function module pattern.
- **IMPORTS**: `image::guess_format`, `quick_xml`, the same OPF transform
  pipeline as Task 7 (consider sharing the OPF reader state machine).
- **GOTCHA**: When the existing cover entry's media-type matches the new
  cover's actual format, only the binary changes. When formats differ
  (e.g. old PNG, new JPEG), both the manifest item's `media-type` AND the
  entry's filename extension change — that requires a NEW entry name (rename
  is not a thing in ZIP) and dropping the old entry in `binary_replacements`
  via "include with `Vec::new()` to delete" — but our repack helper doesn't
  support deletion. Instead: write the new entry under a fresh name
  (e.g. `images/cover-image-{vid_short}.jpg`), update OPF to point at the new
  name, and let the old entry be silently shadowed (it's still in the ZIP,
  but no manifest item references it). Document the small bloat as an
  accepted MVP trade-off; Step 11 sweep can re-pack to drop orphans later.
- **VALIDATE**: Task 19.

### Task 9: Path rename (`writeback/path_rename.rs`)
- **STATUS**: Pending.
- **ACTION**: Render template + collision check + EXDEV-aware atomic move.
- **IMPLEMENT**:
  - `pub fn render(template: &str, work: &WorkSnapshot, manifestation: &ManifestationSnapshot) -> PathBuf` — reuse `services::ingestion::path_template::render`.
  - `pub fn commit(temp: NamedTempFile, dest: &Path) -> Result<(), WritebackError>`
    per ATOMIC_RENAME_WITH_EXDEV_FALLBACK pattern.
  - `pub fn check_collision(dest: &Path) -> Result<(), WritebackError>` —
    if `dest.exists()` AND it's not the same inode as the source, return
    `WritebackError::PathCollision(dest.to_path_buf())`.
- **MIRROR**: ATOMIC_RENAME (same-FS) + ATOMIC_RENAME_WITH_EXDEV_FALLBACK.
- **IMPORTS**: `tempfile::NamedTempFile`, `sha2::{Digest, Sha256}`, `std::fs`,
  `std::path::{Path, PathBuf}`.
- **GOTCHA 1**: `dest.exists()` returns true even when dest IS the source —
  use `std::fs::canonicalize` on both and compare. This catches the
  same-path-for-content-only-update case.
- **GOTCHA 2**: EXDEV detection across kernels. Linux 5.x returns
  `ErrorKind::CrossesDevices` (stable from Rust 1.85); older Rust returns
  `ErrorKind::Other` with `raw_os_error() == Some(18)`. Match both per the
  pattern snippet.
- **GOTCHA 3**: `path_template::render` may produce paths containing characters
  illegal on the underlying FS. Sanitisation is the path-template module's
  responsibility; writeback only normalises `..` and absolute components
  defensively before any file op.
- **VALIDATE**: Task 20.

### Task 10: Wire worker into `main.rs`
- **STATUS**: Pending.
- **ACTION**: Spawn `services::writeback::spawn_worker` alongside enrichment queue.
- **IMPLEMENT**:
  ```rust
  // After the enrichment::spawn_queue spawn at main.rs:107
  let writeback_token = cancel_token.clone();
  let writeback_pool = state.pool.clone();      // reverie_app pool
  let writeback_config = config.clone();
  tokio::spawn(async move {
      if let Err(e) = services::writeback::spawn_worker(writeback_pool, writeback_config, writeback_token).await {
          tracing::error!(error = %e, "writeback worker exited with error");
      }
  });
  ```
- **MIRROR**: `main.rs:107` (enrichment spawn).
- **GOTCHA**: Use `state.pool` (`reverie_app`), NOT `state.ingestion_pool`.
  `reverie_ingestion` has no grant on `writeback_jobs` (per task 2's grants
  block) — wrong pool surfaces as a permission error at runtime.
- **VALIDATE**: `cargo run`; observe `writeback queue started` log line
  alongside `enrichment queue started`.

### Task 11: Config + `.env.example`
- **STATUS**: Pending.
- **ACTION**: Add `WritebackConfig` substructure to `Config`.
- **IMPLEMENT**: Mirror `EnrichmentConfig` shape from `config.rs`:
  - `enabled: bool` (default `true`)
  - `concurrency: u32` (default `2`, range 1–10 — same range as enrichment)
  - `poll_idle_secs: u64` (default `5` — see Task 5 GOTCHA on cadence)
  - `max_attempts: u32` (default `10`)
  - Reject out-of-range values via `ConfigError::Invalid`.
- **MIRROR**: `backend/src/config.rs:60-110` (env var parsing + defaults).
- **VALIDATE**: `cargo test config::tests` — extend `from_env_with_defaults`
  to assert the four new vars.

### Task 12: Webhook events
- **STATUS**: Pending.
- **ACTION**: Add `writeback_complete` and `writeback_failed` to Step 12's
  event enum. Step 12 owns the dispatcher; this task only adds enum variants
  + emits from `writeback::orchestrator::run_once`.
- **IMPLEMENT**:
  - Step 12's enum lives at `src/services/webhooks/events.rs` (or wherever
    Step 12 places it). Add the two variants with payload struct
    `WritebackEventPayload { manifestation_id, reason, attempt_count, error: Option<String>, current_file_hash: Option<Vec<u8>> }`.
  - In `writeback::orchestrator::run_once`, on terminal transitions, emit via
    Step 12's dispatcher API. If Step 12 hasn't shipped yet at implementation
    time, gate the emit behind a feature flag `writeback_emit_events: bool`
    and surface a TODO commit linking to Step 12.
- **MIRROR**: Step 12's existing event-emission pattern.
- **GOTCHA**: Step 12 is in the BLUEPRINT but may not yet be implemented.
  If implementing Step 8 BEFORE Step 12 lands, this task ships a no-op
  `events::emit_writeback_*` stub that logs at info-level and is upgraded
  to real webhook delivery when Step 12 ships.
- **VALIDATE**: With Step 12 stubbed, `cargo test services::writeback::orchestrator`
  asserts `tracing` log emission for the two events.

### Task 13: `epub::repack::with_modifications` round-trip tests
- **STATUS**: Pending.
- **ACTION**: Unit tests in `src/services/epub/repack.rs` `#[cfg(test)]`.
- **IMPLEMENT**:
  - `repack_round_trip_preserves_mimetype_first_stored`: build a known EPUB,
    call `with_modifications` with empty replacements, assert output has
    `mimetype` as entry 0 with `CompressionMethod::Stored`.
  - `repack_round_trip_preserves_per_entry_compression`: input has one Stored
    entry + one Deflated entry; output preserves both.
  - `repack_replaces_opf_when_provided`: input OPF says "Old Title", `opf_replacement`
    bytes say "New Title"; output OPF entry contains "New Title".
  - `repack_replaces_arbitrary_binary_entry`: input has `images/cover.jpg` with
    bytes A; `binary_replacements` maps to bytes B; output contains B.
  - `repack_appends_new_entry_via_additions`: input has no `images/extra.png`;
    `additions` adds it; output contains it.
  - `existing_repair_tests_still_pass`: re-run `repair.rs::tests::*`.
- **MIRROR**: TEST_STRUCTURE (file-system integration).
- **VALIDATE**: `cargo test services::epub::repack services::epub::repair`
  passes without `--ignored`.

### Task 14: Job emission integration test
- **STATUS**: Pending.
- **ACTION**: `#[ignore]` tests covering tasks 3a + 3b.
- **IMPLEMENT**:
  - In `services/enrichment/orchestrator.rs::tests` (extend existing): after
    a successful Apply, query `SELECT count(*) FROM writeback_jobs WHERE manifestation_id=$1` returns 1.
  - In `routes/metadata.rs::tests` (extend existing): accept route inserts
    1 job; revert route inserts 1 job; reject route inserts 0; double-accept
    inserts 2 (worker dedups, not the emitter).
- **MIRROR**: TEST_STRUCTURE (DB integration).
- **VALIDATE**: `cargo test --ignored services::enrichment::orchestrator routes::metadata`.

### Task 15: OPF rewrite — EPUB 2 vs 3 matrix
- **STATUS**: Pending.
- **ACTION**: Tests in `services/writeback/opf_rewrite.rs` `#[cfg(test)]`.
- **IMPLEMENT**:
  - `transform_epub2_writes_calibre_series`: input is EPUB 2 OPF with no series;
    target.series=Some({name:"Mistborn", index:1}); output contains
    `<meta name="calibre:series" content="Mistborn"/>`.
  - `transform_epub3_writes_belongs_to_collection`: same input as EPUB 3;
    output contains `<meta property="belongs-to-collection">Mistborn</meta>` +
    refinements.
  - `transform_epub3_with_existing_calibre_series_updates_both`: input is
    EPUB 3 with both forms; target updates name; output has both forms updated.
  - `transform_preserves_epub_version`: `<package version="3.0">` stays `3.0`.
- **VALIDATE**: `cargo test services::writeback::opf_rewrite::transform_epub`.

### Task 16: OPF rewrite — non-default OPF path discovery
- **STATUS**: Pending.
- **ACTION**: Test in `services/writeback/orchestrator.rs` `#[cfg(test)]`.
- **IMPLEMENT**: Build a fixture EPUB whose `META-INF/container.xml` points
  at `OEBPS/package.opf` (not `content.opf`). Run writeback; assert the
  changed OPF lives at `OEBPS/package.opf` in the output.
- **MIRROR**: Step 5's container.xml parser.
- **GOTCHA**: Reuse Step 5's discovery — do not hard-code `content.opf`.
- **VALIDATE**: `cargo test services::writeback::orchestrator::transform_finds_non_default_opf`.

### Task 17: OPF rewrite — multiple `<dc:identifier>` elements
- **STATUS**: Pending.
- **ACTION**: Tests in `services/writeback/opf_rewrite.rs`.
- **IMPLEMENT**:
  - `transform_updates_only_isbn_identifier`: input has UUID + ISBN identifiers;
    target.isbn_13=Some("9781234567890"); output's UUID identifier is unchanged,
    ISBN identifier updated.
  - `transform_inserts_isbn_when_absent`: input has only UUID identifier;
    target.isbn_13=Some(...); output has UUID PLUS new
    `<dc:identifier opf:scheme="ISBN">9781...</dc:identifier>`. Package's
    `unique-identifier` attribute still points at the UUID.
  - `transform_preserves_unique_identifier_attribute`: assert across all paths.
- **VALIDATE**: `cargo test services::writeback::opf_rewrite::transform_identifier`.

### Task 18: OPF rewrite — custom meta preservation
- **STATUS**: Pending.
- **ACTION**: Tests in `services/writeback/opf_rewrite.rs`.
- **IMPLEMENT**: Input OPF has a custom `<meta name="kobo:something" content="X"/>`
  and a `<dc:coverage>2010</dc:coverage>` element neither of which Step 8
  manages. Target updates only `title`. Output preserves both unchanged
  (verified by string-search and by re-parsing + counting elements).
- **VALIDATE**: `cargo test services::writeback::opf_rewrite::transform_preserves_custom`.

### Task 19: Cover embed — replace + insert + sidecar move
- **STATUS**: Pending.
- **ACTION**: Tests in `services/writeback/cover_embed.rs` + integration test
  in orchestrator.
- **IMPLEMENT**:
  - `plan_embed_replaces_existing_epub3`: fixture has `properties="cover-image"`
    on a manifest item; plan returns binary_replacement keyed by that entry's name.
  - `plan_embed_replaces_existing_epub2`: fixture has `<meta name="cover" content="X">`
    + manifest item `id="X"`; plan returns binary_replacement keyed by that item's href.
  - `plan_embed_inserts_when_absent_epub3`: fixture has no cover; plan
    returns an `additions` entry + an OPF replacement adding the manifest item
    with `properties="cover-image"`.
  - `plan_embed_inserts_when_absent_epub2`: same but with `<meta name="cover">`.
  - Integration: full writeback with reason=cover; sidecar at `_covers/pending/`
    moves to `_covers/accepted/` after success; on rollback, sidecar stays in pending.
- **MIRROR**: TEST_STRUCTURE (file-system integration).
- **VALIDATE**: `cargo test services::writeback::cover_embed`.

### Task 20: Path rename matrix
- **STATUS**: Pending.
- **ACTION**: Tests in `services/writeback/path_rename.rs`.
- **IMPLEMENT**:
  - `rename_same_directory_no_op`: `dest == source`; commit succeeds, file content unchanged.
  - `rename_cross_directory_same_fs`: dest in sibling dir on same FS; rename succeeds atomically.
  - `rename_cross_directory_cross_fs`: simulate EXDEV by mocking `temp.persist`
    to return `PersistError::CrossesDevices`; fallback path executes; final
    file matches expected sha256.
  - `rename_collision_aborts`: dest pre-exists with different content;
    `commit` returns `WritebackError::PathCollision`; source untouched.
- **MIRROR**: ATOMIC_RENAME_WITH_EXDEV_FALLBACK.
- **GOTCHA**: Real cross-FS testing on CI requires either Docker volumes on
  different mount points OR `mock`-style injection. Use a thin wrapper over
  `temp.persist` that the test can override.
- **VALIDATE**: `cargo test services::writeback::path_rename`.

### Task 21: Post-validation rollback
- **STATUS**: Pending.
- **ACTION**: Test in `services/writeback/orchestrator.rs` `#[cfg(test)]`.
- **IMPLEMENT**: Build a fixture where a deliberately-malformed OPF replacement
  causes `validate_and_repair` to surface a new issue (e.g. write garbage into
  the OPF entry). Run `run_once`; assert:
  - File at original path matches `original_bytes` byte-for-byte after rollback.
  - Job row has `status='failed'`, `error` starts with `post_writeback_validation_regressed:`.
  - Canonical pointers on the manifestation are UNCHANGED from before the run
    (the user's accept survives — only file mirroring failed).
- **MIRROR**: TEST_STRUCTURE (DB + file-system integration).
- **VALIDATE**: `cargo test --ignored services::writeback::orchestrator::rollback`.

### Task 22: Queue tests — concurrency, retry, shutdown
- **STATUS**: Pending.
- **ACTION**: `#[ignore]` tests in `services/writeback/queue.rs`.
- **IMPLEMENT**: Mirror Step 7 task 35:
  - `two_workers_for_same_manifestation_serialise` per TWO_WORKER_RACE_HARNESS.
  - `two_workers_for_distinct_manifestations_parallelise`: insert pending jobs
    for two different manifestations; both can be claimed `in_progress`
    simultaneously.
  - `retry_backoff_honoured`: failed job at `attempt_count=2`, `last_attempted_at = now() - 25min`;
    `claim_next` returns None (5min < 30min wait). Set `last_attempted_at = now() - 35min`;
    returns the row.
  - `shutdown_reverts_in_progress`: claim a row, fire cancel token, wait for
    worker exit, assert row status back to `pending`.
  - `max_attempts_transitions_to_skipped`: repeatedly claim + fail; at
    `attempt_count == max_attempts`, transition to `skipped`.
- **VALIDATE**: `cargo test --ignored services::writeback::queue`.

### Task 23: Crash-recovery reconciler
- **STATUS**: Pending.
- **ACTION**: `#[ignore]` test in `services/writeback/queue.rs`.
- **IMPLEMENT**: Insert a job row in `status='in_progress'` (simulating a
  crash during processing). Call `spawn_worker` in a tokio task; observe via
  polling that the row transitions back to `pending` within one tick; cancel
  the worker before it actually processes.
- **MIRROR**: REVERT_IN_PROGRESS_ON_SHUTDOWN — same statement, different trigger.
- **VALIDATE**: `cargo test --ignored services::writeback::queue::crash_recovery`.

### Task 24: Hash columns — current vs ingestion
- **STATUS**: Pending.
- **ACTION**: `#[ignore]` test in `services/writeback/orchestrator.rs`.
- **IMPLEMENT**:
  - `current_file_hash_updates_after_writeback`: insert a manifestation
    with `ingestion_file_hash = X`; ingest a real EPUB; run writeback;
    assert `current_file_hash != X`, `current_file_hash == sha256(file_after)`,
    AND `ingestion_file_hash == X` still.
  - `ingestion_file_hash_immutable_across_writeback_chain`: run writeback twice
    (two accepts); `ingestion_file_hash` constant, `current_file_hash` updates
    twice.
- **VALIDATE**: `cargo test --ignored services::writeback::orchestrator::hash`.

---

## Testing Strategy

### Unit Tests (no DB)

| Test | Input | Expected | Edge Case? |
|---|---|---|---|
| `repack::with_modifications` | empty replacements | byte-identical re-pack | happy |
| `repack::with_modifications` | preserves Stored vs Deflated | per-entry compression preserved | yes |
| `opf_rewrite::transform` | EPUB 2 + series | `<meta name="calibre:series">` | happy |
| `opf_rewrite::transform` | EPUB 3 + series | `<meta property="belongs-to-collection">` | happy |
| `opf_rewrite::transform` | EPUB 3 with calibre form pre-existing | both forms updated | yes |
| `opf_rewrite::transform` | multi-identifier UUID + ISBN | only ISBN updated | yes |
| `opf_rewrite::transform` | no ISBN, target sets one | new ISBN inserted, unique-identifier preserved | yes |
| `opf_rewrite::transform` | custom `<meta>` + `<dc:coverage>` | preserved | yes |
| `cover_embed::plan_embed` | EPUB 3 with cover-image | binary_replacement only | happy |
| `cover_embed::plan_embed` | EPUB 2 with `<meta name="cover">` | binary_replacement only | happy |
| `cover_embed::plan_embed` | no cover entry | additions + OPF mods | yes |
| `path_rename::commit` | same-FS persist | atomic rename succeeds | happy |
| `path_rename::commit` | EXDEV simulated | copy + verify + unlink fallback | yes |
| `path_rename::check_collision` | dest exists, different inode | `PathCollision` error | yes |

### Integration Tests (DB required; `#[ignore]`)

Tasks 14, 16, 19 (integration), 21, 22, 23, 24.

### Edge Cases Checklist
- [ ] Empty `writeback_jobs` queue → idle worker no-ops on every tick.
- [ ] Manifestation with `format != 'epub'` → job claimed, marked `'skipped'` immediately, no file I/O.
- [ ] Manifestation with NULL `file_path` or missing file → same.
- [ ] Two accepts on different fields of same manifestation in <1s → two jobs, second is sub-100ms no-op.
- [ ] Accept while worker is mid-job on the same manifestation → second job stays `pending` (NOT EXISTS clause), worker picks it up after current job completes.
- [ ] EPUB OPF located at `OEBPS/package.opf` → discovered via container.xml.
- [ ] Multi-identifier OPF (UUID + ISBN) → only ISBN updated, unique-identifier preserved.
- [ ] EPUB 3 with `calibre:series` and `belongs-to-collection` both present → both updated.
- [ ] Cover replacement with same media-type → binary swap.
- [ ] Cover insertion when EPUB has no cover → manifest item added, OPF reflects.
- [ ] Path-template rename across FS boundaries → EXDEV fallback invoked.
- [ ] Path collision (target file exists) → job `failed`, source untouched.
- [ ] Post-writeback validation regression → file rolled back, DB pointer preserved.
- [ ] Crash with `in_progress` jobs → worker startup reverts to `pending`.
- [ ] Sidecar cover at `_covers/pending/` → moves to `_covers/accepted/` only on success; stays in pending on rollback.
- [ ] `current_file_hash` updates exactly once per successful writeback.
- [ ] `ingestion_file_hash` never changes after migration.
- [ ] Disk full during writeback → temp file unlinked on Drop, source untouched, job marked `failed`.
- [ ] Permission denied on rename → same.

---

## Validation Commands

### Static Analysis
```bash
cd backend
cargo fmt --check
```

```bash
cd backend
cargo clippy --all-targets -- -D warnings
```
EXPECT: zero warnings, zero diffs.

### Unit Tests (fast, no DB)
```bash
cd backend
cargo test --lib
```
EXPECT: all pass.

### Integration Tests (DB required)
```bash
docker compose up -d
```

```bash
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev sqlx migrate run --source backend/migrations
```

```bash
cd backend
DATABASE_URL=postgres://reverie_app:reverie_app@localhost:5433/reverie_dev DATABASE_URL_INGESTION=postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev cargo test --lib -- --ignored
```
EXPECT: all pass.

### Migration Round-Trip
```bash
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev sqlx migrate run --source backend/migrations
```

```bash
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev sqlx migrate revert --source backend/migrations
```

```bash
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev sqlx migrate run --source backend/migrations
```
EXPECT: all three succeed; manual `\d manifestations` in psql confirms `ingestion_file_hash` and `current_file_hash` are present after up, just `file_hash` after down.

### Supply-chain
```bash
cd backend
cargo audit
```
EXPECT: no new advisories from `quick-xml`.

### Manual Smoke (from BLUEPRINT Verification section, lines 1223–1248)
- [ ] Ingest a book; accept a metadata change via API; observe `writeback_jobs` row created in same transaction as pointer move.
- [ ] Worker claims the job within `poll_idle_secs`; row transitions `pending → in_progress → complete`.
- [ ] Unzip the managed EPUB at the OPF path resolved from `META-INF/container.xml`; verify updated fields are present and well-formed XML.
- [ ] `manifestations.current_file_hash` updated; `ingestion_file_hash` unchanged.
- [ ] Cover smoke: accept a new cover; verify embedded cover binary in the EPUB is replaced; sidecar moves from `_covers/pending/` to `_covers/accepted/`.
- [ ] Path-rename smoke: change author such that path template re-renders; verify file moves and `manifestations.file_path` updates.
- [ ] Concurrent smoke: rapid-fire accept N fields on one manifestation; verify all changes land in the file (no race-overwrite); verify only one worker writes at a time per manifestation.
- [ ] Crash smoke: kill worker mid-job; restart; verify `in_progress` row transitions to `pending` and the job re-runs cleanly.
- [ ] Validation rollback smoke: feed writeback a fixture that would regress validation; verify file is restored, job marked failed, DB pointer move preserved.

---

## Acceptance Criteria

_Copied from BLUEPRINT Step 8 Exit Criteria (lines 1250–1268). Do not reinterpret._

- [ ] Canonical pointer moves enqueue `writeback_jobs` rows in the same transaction.
- [ ] Worker processes jobs outside any user-facing transaction; no row locks held across filesystem I/O.
- [ ] Writeback updates OPF text fields, embedded cover, and on-disk path; EPUB remains valid (passes `validate_and_repair`).
- [ ] Concurrent jobs for the same manifestation never produce overlapping file writes; jobs for distinct manifestations run in parallel.
- [ ] `EXDEV` cross-filesystem renames fall back to copy+verify+unlink.
- [ ] Path collisions abort with a `failed` status; source file untouched.
- [ ] Post-writeback validation regression rolls back the file but preserves the DB pointer move.
- [ ] Crash recovery: `in_progress` jobs at startup transition to `pending` and re-claim cleanly.
- [ ] `ingestion_file_hash` immutable; `current_file_hash` reflects on-disk state after every successful writeback.
- [ ] `writeback_complete` and `writeback_failed` events emitted on terminal transitions.

## Completion Checklist

Phase progress on `feat/metadata-writeback` (proposed):

- **Phase A** — refactor + migration (Tasks 1–2): no behaviour change.
- **Phase B** — pure modules (Tasks 4, 7, 8, 9): unit-testable, no DB.
- **Phase C** — orchestrator + queue + emission hooks + main wiring + config (Tasks 3a, 3b, 5, 6, 10, 11): integration glue.
- **Phase D** — webhook events + tests (Tasks 12–24): coverage.

- [ ] All tasks 1–24 complete and checked in (per-task `STATUS` lines).
- [ ] Code follows patterns in **Patterns to Mirror**.
- [ ] Error handling uses `WritebackError` at module boundaries and `AppError::Internal(anyhow::Error)` at the route boundary; no ad-hoc `StatusCode` at handlers.
- [ ] Logging uses `tracing::{info,warn,error}!` with structured fields; no `println!`.
- [ ] Tests follow TEST_STRUCTURE; DB tests `#[ignore]`-gated.
- [ ] No hardcoded paths, timeouts, or limits outside `Config`.
- [ ] Migration round-trip succeeds; `sqlx migrate revert` leaves DB in pre-migration state.
- [ ] PR description includes "Depends on Step 7" and lists env vars added.
- [ ] `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo test --ignored` all pass.
- [ ] `cargo audit` has no new advisories.
- [ ] Documentation (`backend/CLAUDE.md`) requires no updates — conventions already documented.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `quick-xml` event-stream loses an unknown construct on round-trip | Medium | High (silently corrupts an OPF the next reader chokes on) | Task 18 explicitly tests preservation of custom `<meta>` + `<dc:coverage>`; gold-master tests against fixture OPFs from real publishers |
| Compression-method preservation drift in `repack::with_modifications` | Low | Medium (mimetype must stay Stored — others usually fine but Adobe DRM EPUBs detect tampering by compression changes) | Task 13 explicitly tests per-entry compression preservation |
| EXDEV detection misses on some kernels (e.g. WSL2) | Low | High (file-system corruption if persist returns success but file is at wrong path) | Match BOTH `ErrorKind::CrossesDevices` AND `raw_os_error() == Some(18)`; CI runs on Linux + macOS |
| `pg_advisory_xact_lock` was considered for per-manifestation serialisation; we chose claim-CTE instead | Low | Low (slightly less defence-in-depth) | Documented as alternative; if production traffic shows races, advisory lock can be added in `orchestrator::run_once` as a single-line addition |
| `current_file_hash` drift between rename and DB update on crash | Medium | Low (Step 11 health surfaces it) | Documented as expected; Step 11 reconciliation runs on schedule |
| Cover insertion creates orphaned ZIP entries when format changes (PNG → JPEG) | Medium | Low (small file bloat) | Documented in task 8 GOTCHA; Step 11 sweep can repack to drop orphans |
| `writeback_jobs` queue grows unbounded under enrichment storm | Low | Medium (DB bloat) | Worker drains continuously; jobs are sub-second on cache-warm runs; Step 11 health surfaces queue depth as a metric |
| Adobe Digital Editions / Apple Books reject re-packaged EPUBs due to subtle ZIP-spec deviation | Medium | High (users can't sideload to those readers) | Task 13's automated structural check matches Step 5's; manual reader spot-check is a supplemental gate per BLUEPRINT line 324 |
| Step 12 webhook subsystem not yet implemented when Step 8 lands | High | Low (events are new but not load-bearing) | Task 12 GOTCHA: ship a logging stub if Step 12 is later; upgrade when Step 12 lands |

## Notes

- **Branch**: `feat/metadata-writeback` (per BLUEPRINT line 931).
- **Depends on**: Step 7 merged. Step 7's `metadata_versions` journal,
  per-field `*_version_id` pointers, `enrichment::orchestrator::apply_field`,
  and `routes/metadata::apply_version` are pre-conditions.
- **Blocks**: Step 11 (Library Health surfaces `writeback_jobs` failed-state
  + `current_file_hash != on_disk_hash` divergence); Step 12 (webhooks consume
  `writeback_complete` + `writeback_failed`).
- **Tooling & environment**: dev postgres on port 5433 (see `backend/CLAUDE.md`);
  `reverie` role runs migrations, `reverie_app` is the worker's runtime role,
  `reverie_ingestion` has NO grant on `writeback_jobs` (invariant 8 — ingestion
  never modifies managed files). Run `docker compose up -d` to start the stack.
- **Memory-instinct callouts** (from `~/.claude/projects/-home-coder-reverie/memory/`):
  - `feedback_postgres_enum_rebuild` — DROP DEFAULT / SET DEFAULT order
  - `project_schema_evolution` — pre-release schema is freely mutable
  - `project_time_not_chrono` — use `time` crate, not `chrono`
  - `global 90% atomic claim` — `FOR UPDATE SKIP LOCKED` in one CTE, never two statements
  - `feedback_secret_handling` — no grep / cat on decrypted SOPS during ops
- **Adversarial-review traceability**: All 13 findings (D1–D3, S1–S7, C1–C3)
  from the 2026-04-18 review are addressed by the BLUEPRINT spec edits and
  the tasks in this plan. See BLUEPRINT lines 929–1281 for the consolidated
  Step 8 spec; see this plan's task list for the implementation surface.
- **Open Questions**:
  - Step 12's webhook event API shape is not yet pinned. Task 12 ships a
    logging stub if Step 12 hasn't landed; upgrade is a one-line change.
  - Cover format conversions (PNG → WebP for size) are NOT in scope. We
    embed exactly what's at `_covers/pending/` regardless of size.
