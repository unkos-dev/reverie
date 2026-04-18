# Plan: Metadata Enrichment Pipeline (BLUEPRINT Step 7)

## Summary

Build Tome's three-layer metadata enrichment subsystem: an append-only **journal**
(`metadata_versions` keyed on value hash), per-field **policy** engine (auto-fill /
propose / lock, hard-coded in Rust), and **canonical** pointer columns
(`works.*_version_id`, `manifestations.*_version_id`) that make every displayed
value traceable to its source row. Includes a background enrichment queue with
atomic claim, a registry-driven adapter model (Open Library, Google Books,
Hardcover), an SSRF-safe cover downloader, accept/reject/revert/lock/dry-run API
endpoints, and a work-rematch helper that auto-merges stub works on ISBN correction.

## User Story

As a Tome librarian, I want external metadata providers to enrich my books
automatically — filling empty fields when one source speaks, staging competing
values for my review when sources disagree, and remembering every value ever seen
so I can revert cleanly — so that my library is rich and accurate without losing
edits I made by hand.

## Problem → Solution

**Current state:** `metadata_versions` exists but is a flat per-field draft table
(`UNIQUE (manifestation_id, source, field_name)`). OPF drafts overwrite each other
on re-extraction. No adapters, no queue, no canonical pointers, no audit trail
beyond "last draft wins," no cover fetcher, no review UI endpoints.

**Desired state:** Three-layer model (journal + policy + canonical) with dedup on
`value_hash`, an extensible source registry, atomic queue claim, dry-run preview,
SSRF-safe cover download, field locks, and work-rematch on ISBN correction.

## Metadata

- **Complexity**: XL (30+ files, migrations, new subsystem, cross-cutting refactor
  of ingestion orchestrator and work matcher)
- **Source PRD**: `plans/BLUEPRINT.md` — "Step 7: Metadata Enrichment Pipeline"
  (lines 380–928)
- **PRD Phase**: Step 7 of 12; depends on Step 6 complete, blocks Step 8
- **Estimated Files**: ~35 created, ~5 modified, 1 migration pair

---

## UX Design

### Before
Backend-only; ingestion writes drafts with "last-write-wins." Users have no
endpoints to review, accept, or reject metadata suggestions.

### After

```text
┌────────────────────────────────────────────────────────────┐
│ Ingestion → journal (opf) → policy → canonical pointer set │
│       ↓                                                    │
│ Enrichment queue polls pending manifestations              │
│       ↓                                                    │
│ Parallel fan-out: openlibrary + googlebooks + hardcover    │
│       ↓                                                    │
│ Journal upsert (dedup on value_hash, bumps observation)    │
│       ↓                                                    │
│ Policy decides per-field:                                  │
│   empty canonical + auto-fill → Apply (pointer update)     │
│   occupied or disagree → Stage (pending row)               │
│   locked → Reject                                          │
│       ↓                                                    │
│ REST endpoints: GET metadata, POST accept/reject/revert    │
│                 /lock /dry-run /trigger                    │
└────────────────────────────────────────────────────────────┘
```

### Interaction Changes

| Touchpoint | Before | After | Notes |
|---|---|---|---|
| `GET /api/manifestations/:id/metadata` | N/A | canonical + pending + locks + provenance | New route family |
| `POST /api/manifestations/:id/enrichment/trigger` | N/A | re-queues manifestation | Admin or adult; child 403 |
| `POST /api/manifestations/:id/metadata/:field/{accept,reject,revert,lock}` | N/A | policy-engine writes | Admin or adult; child 403 |
| `POST /api/manifestations/:id/enrichment/dry-run` | N/A | preview diff; no journal writes | Admin or adult; child 403 |
| `GET  /api/enrichment/status` | N/A | queue counters | Admin or adult |
| Ingestion | writes draft rows, no canonical pointers | writes journal rows AND sets canonical pointers atomically | Existing behaviour preserved as invariant |

---

## Mandatory Reading

| Priority | File | Lines | Why |
|---|---|---|---|
| P0 | `plans/BLUEPRINT.md` | 380–928 | Complete Step 7 spec (schema, adapters, policy, tasks, exit criteria) |
| P0 | `.claude/projects/-home-coder-Tome/memory/project_enrichment_architecture.md` | all | Frozen design axes (layer boundaries, confidence formula, status simplification) |
| P0 | `backend/src/error.rs` | 1–40 | `AppError` variants + `IntoResponse` mapping |
| P0 | `backend/src/services/metadata/draft.rs` | 1–140 | Current draft writer — task 2 rewrites this |
| P0 | `backend/src/services/ingestion/orchestrator.rs` | 470–570 | Current ingestion DB insert shape — task 3 refactors this into a transaction |
| P0 | `backend/src/models/work.rs` | 1–150 | Current `find_or_create` — tasks 4 and 6 extend it |
| P0 | `backend/migrations/20260412150003_series_and_metadata.up.sql` | all | Current `metadata_versions` shape |
| P0 | `backend/migrations/20260415000003_unique_hash_and_drafts.up.sql` | all | Unique constraint to drop + stale comment to tombstone |
| P0 | `backend/migrations/20260412150005_system_tables.up.sql` | 48–62 | Per-role grants pattern — mirror for every new table |
| P0 | `backend/migrations/20260416000001_remove_invalid_validation_status.up.sql` | all | Enum-rebuild pattern (DROP DEFAULT → rename → create → cast → SET DEFAULT → DROP old) |
| P0 | `backend/src/auth/middleware.rs` | 12–22 | `CurrentUser { role, is_child }` — task 7 adds helpers consuming these |
| P0 | `backend/src/routes/ingestion.rs` | 15–30 | Current `role != "admin"` check — task 7 migrates to helper |
| P1 | `backend/src/routes/tokens.rs` | all | Route-module template (router fn, DTO structs, `CurrentUser` extractor, `State<AppState>`) |
| P1 | `backend/src/services/metadata/isbn.rs` | all | `parse_isbn` / `normalise` — used by lookup_key canonicalisation |
| P1 | `backend/src/services/ingestion/orchestrator.rs` | 82–110 | `CancellationToken` + advisory lock shutdown pattern — mirror for queue |
| P1 | `backend/src/main.rs` | 80–96 | Where to spawn `services::enrichment::spawn_queue` |
| P1 | `backend/src/config.rs` | 1–30, 60–110 | `Config` struct layout; how env vars are parsed & validated |
| P1 | `backend/src/models/ingestion_job.rs` | all | Status-column transition pattern (`mark_running/complete/failed/skipped`) |
| P1 | `backend/src/test_support.rs` | all | `test_server()` + `test_config()` for route tests |
| P2 | `backend/src/services/metadata/extractor.rs` | all | `ExtractedMetadata` shape consumed by draft-writer rewrite |
| P2 | `backend/src/services/ingestion/orchestrator.rs` | 600–820 | Full `process_file` for task 3 refactor surface area |
| P2 | `backend/src/services/metadata/sanitiser.rs` | all | Existing normalisation patterns — reuse in `value_hash.rs` |
| P2 | `backend/src/db.rs` | all | `init_pool` + `acquire_with_rls` — if queue needs RLS context |
| P2 | `backend/src/auth/backend.rs` | all | `AuthBackend` used by session login (referenced by queue config) |
| P2 | `backend/CLAUDE.md` | all | Backend-specific error/testing/logging conventions |
| P2 | `CLAUDE.md` | all | Conventional Commits + TDD mandate |

## External Documentation

| Topic | Source | Key Takeaway |
|---|---|---|
| Open Library API | `https://openlibrary.org/dev/docs/api/books` | `/isbn/{isbn}.json` returns single book; `/search.json?title=&author=` returns list; no auth; 100 req/min |
| Google Books API | `https://developers.google.com/books/docs/v1/using` | `/volumes?q=isbn:9780306406157`, `/volumes?q=intitle:X+inauthor:Y`; optional `key=` param; 1000 req/day unauthenticated |
| Hardcover GraphQL | `https://docs.hardcover.app/api/` | POST GraphQL to `api.hardcover.app/v1/graphql`; `Authorization: Bearer <token>` required; auto-disable adapter if token missing |
| `governor` crate | `https://docs.rs/governor/latest/governor/` | `RateLimiter::direct(Quota::per_minute(nonzero!(60u32)))`; async wait via `until_ready()`; one limiter per source |
| `hickory-resolver` | `https://docs.rs/hickory-resolver/` | System config resolver; use to pre-resolve hostnames in cover redirect policy so we can IP-class-check before connect |
| `reqwest` redirect policy | `https://docs.rs/reqwest/latest/reqwest/redirect/` | `ClientBuilder::redirect(Policy::custom(|a| ...))`; attempt exposes previous URLs and new URL; validate IP on each hop |
| `sqlx` FOR UPDATE SKIP LOCKED | `https://www.postgresql.org/docs/current/sql-select.html` | One-statement CTE: `WITH claimed AS (SELECT id FROM t WHERE status='pending' FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE t SET status='in_progress' FROM claimed WHERE t.id=claimed.id RETURNING t.id` |

> Fetch current docs via the `documentation-lookup` skill (Context7 MCP) at
> implementation time for `governor`, `hickory-resolver`, and any API schema
> changes. The Blueprint's endpoint URLs and auth schemes are authoritative; HTTP
> shapes can drift, so fetch once per adapter before writing it.

---

## Patterns to Mirror

> Every snippet below is copied from the live codebase. File:line references are
> anchors — do not invent alternatives.

### NAMING_CONVENTION
```rust
// SOURCE: backend/src/services/ingestion/orchestrator.rs:1-14
// - Modules: snake_case files, `mod` rooted in parent's mod.rs
// - Public fns: snake_case; structs/enums: PascalCase
// - Error types: thiserror at library boundaries, anyhow at app boundaries
use std::path::{Path, PathBuf};
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config::{CleanupMode, Config, SUPPORTED_FORMATS};
use crate::models::{ingestion_job, work};
use crate::services::epub::{self, ValidationOutcome};
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

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".to_owned()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_owned()),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden".to_owned()),
            Self::Validation(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg),
            Self::Internal(err) => {
                tracing::error!(error = %err, "internal server error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error".to_owned())
            }
        };
        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}
```

### LOGGING_PATTERN
```rust
// SOURCE: backend/src/services/ingestion/orchestrator.rs:53-60
tracing::info!(
    processed = r.processed,
    failed = r.failed,
    skipped = r.skipped,
    "batch complete"
);
// never println! / eprintln! — use structured `field = value` pairs
```

### REPOSITORY_PATTERN
```rust
// SOURCE: backend/src/models/work.rs:20-39
pub async fn find_or_create(
    pool: &PgPool,
    metadata: &ExtractedMetadata,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Step 1: ISBN match
    if let Some(ref isbn) = metadata.isbn
        && let Some(ref isbn_13) = isbn.isbn_13
    {
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT w.id FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.isbn_13 = $1 \
             LIMIT 1",
        )
        .bind(isbn_13)
        .fetch_optional(&mut *tx)
        .await?;
        // ...
    }
    tx.commit().await?;
    Ok(work_id)
}
```

### UPSERT_WITH_RETURNING
```rust
// SOURCE: backend/src/models/work.rs:120-130
// DO UPDATE SET name = EXCLUDED.name is a no-op trick to make RETURNING work
// on the conflict path (DO NOTHING doesn't return the existing row).
sqlx::query_scalar(
    "INSERT INTO authors (name, sort_name) VALUES ($1, $2) \
     ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name \
     RETURNING id",
)
.bind(name)
.bind(sort_name)
.fetch_one(&mut *conn)
.await
```

### SERVICE_PATTERN (module root + public API)
```rust
// SOURCE: backend/src/services/ingestion/mod.rs
pub mod cleanup;
pub mod copier;
pub mod format_filter;
pub mod orchestrator;
pub mod path_template;
pub mod quarantine;
pub mod watcher;

pub use orchestrator::{run_watcher, scan_once};
```

### ROUTE_PATTERN
```rust
// SOURCE: backend/src/routes/tokens.rs:15-50
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tokens", post(create_token))
        .route("/api/tokens", get(list_tokens))
        .route("/api/tokens/{id}", delete(revoke_token))
}

#[derive(serde::Deserialize)]
struct CreateTokenRequest { name: String }

async fn create_token(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.name.trim().is_empty() || body.name.len() > 255 {
        return Err(AppError::Validation("name must be 1-255 characters".into()));
    }
    // ...
}
```

### ROLE_CHECK (current — tasks 7 migrates this)
```rust
// SOURCE: backend/src/routes/ingestion.rs:19-21
if current_user.role != "admin" {
    return Err(AppError::Forbidden);
}
// task 7 replaces with: current_user.require_admin()?;
```

### CANCELLATION_TOKEN_SHUTDOWN
```rust
// SOURCE: backend/src/services/ingestion/orchestrator.rs:39-72
loop {
    tokio::select! {
        _ = cancel.cancelled() => {
            tracing::info!("orchestrator shutting down");
            break;
        }
        batch = rx.recv() => { /* process */ }
    }
}
```

### ADVISORY_LOCK (for queue-wide serialisation if needed)
```rust
// SOURCE: backend/src/services/ingestion/orchestrator.rs:82-106
const SCAN_ADVISORY_LOCK_ID: i64 = 0x546F6D65_00000004; // "Tome" + step 4
let mut lock_conn = pool.acquire().await?;
sqlx::query("SELECT pg_advisory_lock($1)")
    .bind(SCAN_ADVISORY_LOCK_ID)
    .execute(&mut *lock_conn)
    .await?;
// ...
let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
    .bind(SCAN_ADVISORY_LOCK_ID)
    .execute(&mut *lock_conn)
    .await;
```

### ENUM_REBUILD (Postgres — required for status simplification)
```sql
-- SOURCE: backend/migrations/20260416000001_remove_invalid_validation_status.up.sql
-- PostgreSQL cannot DROP an enum value directly; rebuild the type.
ALTER TYPE metadata_review_status RENAME TO metadata_review_status_old;
CREATE TYPE metadata_review_status AS ENUM ('pending', 'rejected');
-- Drop the default BEFORE altering the column type.
ALTER TABLE metadata_versions ALTER COLUMN status DROP DEFAULT;
-- Rows currently 'draft' → 'pending'; 'accepted' rows become pointer-referenced.
ALTER TABLE metadata_versions
    ALTER COLUMN status TYPE metadata_review_status
    USING CASE status::text
        WHEN 'draft' THEN 'pending'::metadata_review_status
        WHEN 'accepted' THEN 'pending'::metadata_review_status
        WHEN 'rejected' THEN 'rejected'::metadata_review_status
    END;
ALTER TABLE metadata_versions ALTER COLUMN status SET DEFAULT 'pending';
DROP TYPE metadata_review_status_old;
```

### PER_ROLE_GRANTS (every new table must emit these)
```sql
-- SOURCE: backend/migrations/20260412150005_system_tables.up.sql:48-62
GRANT SELECT, INSERT, UPDATE, DELETE ON <new_table> TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON <new_table> TO tome_ingestion;
-- (or SELECT-only if ingestion only needs FK resolution — see metadata_sources)
GRANT SELECT ON <new_table> TO tome_readonly;
```

### MIGRATION_NAMING
```text
backend/migrations/20260417NNNNNN_add_enrichment_pipeline.up.sql
backend/migrations/20260417NNNNNN_add_enrichment_pipeline.down.sql
# Use today's UTC date; allocate a six-digit serial higher than the last migration.
```

### TEST_STRUCTURE (DB integration)
```rust
// SOURCE: backend/src/services/metadata/draft.rs:140-200
fn db_url() -> String {
    std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
        "postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev".into()
    })
}

#[tokio::test]
#[ignore] // requires PostgreSQL with migrations applied
async fn write_drafts_creates_metadata_version_rows() {
    let pool = PgPool::connect(&db_url()).await.unwrap();
    let (work_id, manifestation_id) = setup_manifestation(&pool).await;
    // ...
    cleanup(&pool, work_id, manifestation_id).await;
}
```

### TEST_STRUCTURE (route integration)
```rust
// SOURCE: backend/src/routes/tokens.rs:143-195
#[tokio::test]
async fn create_token_returns_401_without_auth() {
    let server = test_support::test_server();
    let response = server
        .post("/api/tokens")
        .json(&serde_json::json!({"name": "My Kindle"}))
        .await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

// Basic-auth route test — Basic creds, NOT Bearer:
use base64ct::Encoding;
let basic = base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());
server.post("/api/...")
    .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
    .await;
```

---

## Files to Change

| File | Action | Justification |
|---|---|---|
| `backend/Cargo.toml` | UPDATE | Add `governor`, `hickory-resolver`; add `webp` feature to `image` |
| `backend/migrations/20260417NNNNNN_add_enrichment_pipeline.up.sql` | CREATE | Full schema (registry, journal rewrite, pointers, field_locks, queue columns, cache kind) |
| `backend/migrations/20260417NNNNNN_add_enrichment_pipeline.down.sql` | CREATE | Clean reversal |
| `backend/src/services/metadata/draft.rs` | UPDATE | Rewrite to new journal shape; compute `value_hash`; return created version IDs |
| `backend/src/services/ingestion/orchestrator.rs` | UPDATE | `process_file`: single-tx insert → draft → update pointers (task 3); heuristic-fallback journal row (task 5) |
| `backend/src/models/work.rs` | UPDATE | `find_or_create`: wire pointers (task 4); add `rematch_on_isbn_change` (task 6) |
| `backend/src/auth/middleware.rs` | UPDATE | Add `require_admin()`, `require_not_child()` to `CurrentUser` (task 7) |
| `backend/src/routes/ingestion.rs` | UPDATE | Migrate to `require_admin()` (task 7) |
| `backend/src/routes/tokens.rs` | UPDATE | Migrate 3 sites to `require_admin()` (task 7) |
| `backend/src/routes/auth.rs` | UPDATE | Migrate 1 site to `require_admin()` (task 7) |
| `backend/src/config.rs` | UPDATE | Add enrichment + cover env vars |
| `backend/src/main.rs` | UPDATE | Spawn `enrichment::spawn_queue` alongside watcher; use `tome_app` pool |
| `backend/src/services/mod.rs` | UPDATE | `pub mod enrichment;` |
| `backend/src/routes/mod.rs` | UPDATE | `pub mod enrichment; pub mod metadata;` |
| `backend/src/services/enrichment/mod.rs` | CREATE | Module root |
| `backend/src/services/enrichment/lookup_key.rs` | CREATE | ISBN / title+author key canonicalisation (task 10) |
| `backend/src/services/enrichment/value_hash.rs` | CREATE | Canonical-JSON + SHA-256 with per-field normalisation hooks (task 11) |
| `backend/src/services/enrichment/cache.rs` | CREATE | `api_cache` reader/writer with per-kind TTL (task 12) |
| `backend/src/services/enrichment/policy.rs` | CREATE | `FieldPolicy` enum + defaults + `decide()` (task 13) |
| `backend/src/services/enrichment/confidence.rs` | CREATE | `score(source, match_type, quorum)` (task 14) |
| `backend/src/services/enrichment/http.rs` | CREATE | `api_client()` + SSRF-guarded `cover_client()` (task 15) |
| `backend/src/services/enrichment/cover_download.rs` | CREATE | Byte-capped streaming download + magic-byte + dimension gate (task 16) |
| `backend/src/services/enrichment/sources/mod.rs` | CREATE | `MetadataSource` trait (task 17) |
| `backend/src/services/enrichment/sources/open_library.rs` | CREATE | Adapter (task 18) |
| `backend/src/services/enrichment/sources/google_books.rs` | CREATE | Adapter (task 19) |
| `backend/src/services/enrichment/sources/hardcover.rs` | CREATE | Adapter (task 20) |
| `backend/src/services/enrichment/orchestrator.rs` | CREATE | Per-manifestation flow + rematch hook (task 21) |
| `backend/src/services/enrichment/queue.rs` | CREATE | Background loop, atomic claim CTE, retry backoff (task 22) |
| `backend/src/services/enrichment/dry_run.rs` | CREATE | Preview diff; no journal writes (task 23) |
| `backend/src/services/enrichment/field_lock.rs` | CREATE | `is_locked`/`lock`/`unlock` helpers (task 24) |
| `backend/src/routes/enrichment.rs` | CREATE | trigger / dry-run / status endpoints (task 25) |
| `backend/src/routes/metadata.rs` | CREATE | GET + accept/reject/revert/lock endpoints (task 26) |
| `.env.example` | UPDATE | All new TOME_ENRICHMENT_* + TOME_COVER_* vars |

## NOT Building

- **Goodreads adapter** — no public API; deferred until isolated scraper subsystem
  is specced.
- **Frontend metadata review UI** — belongs to Step 10 (React frontend). This step
  ships only the REST endpoints that feed it.
- **Cover cleanup sweep** — deferred to Step 11 (Library Health dashboard); this
  step leaves rejected/superseded covers in `_covers/pending/`.
- **User-configurable policy** — hard-coded per-field in Rust for MVP. Settings
  UI deferred.
- **UI exposure of `TOME_ENRICHMENT_CONCURRENCY` / `TOME_COVER_MIN_LONG_EDGE_PX`** —
  Step 11 surfaces these; MVP uses env vars only.
- **Writeback of accepted metadata into the EPUB file** — Step 8.
- **Webhook delivery** — Step 12; this step emits `work.duplicate_suspected` +
  enrichment events to `webhook_deliveries` via the existing event plumbing only.
- **Provider-specific advanced features** (subject/BISAC mappings, reviews,
  ratings, linked works). MVP is scalar/simple-list fields only.
- **Pre-existing `metadata_source` enum migration outside the `metadata_versions`
  table** — `manifestations.cover_source` is the only other column of that type
  and is added fresh in this migration as `TEXT REFERENCES metadata_sources(id)`.

---

## Step-by-Step Tasks

> Ordering = the canonical dependency order from BLUEPRINT. Schema before code;
> invariant-preserving refactors (tasks 3–5) before new enrichment code; helpers
> (6–8) before orchestration. Do not reorder.

### Task 1: Migration — `add_enrichment_pipeline.up.sql` / `.down.sql`
- **STATUS**: Complete — Phase A (`4e61154`).
- **ACTION**: Create paired migration at `backend/migrations/20260417NNNNNN_add_enrichment_pipeline.up.sql` and `.down.sql`. Serial number = largest existing + 1.
- **IMPLEMENT**:
  1. Create `metadata_sources` table (id TEXT PK, display_name, kind, enabled, base_priority, config JSONB, added_at). Seed six rows: `opf/OPF Metadata/file/100`, `manual/Manual Override/user/10`, `openlibrary/Open Library/api/100`, `googlebooks/Google Books/api/100`, `hardcover/Hardcover/api/90`, `ai/AI-assisted/ai/500`.
  2. **Rebuild `metadata_review_status` enum** to `('pending', 'rejected')` via the ENUM_REBUILD pattern above, mapping `'draft'→'pending'`, `'accepted'→'pending'` (canonical pointer is the source of truth for accepted), `'rejected'→'rejected'`.
  3. **Drop** `metadata_versions_manifestation_source_field_unique` constraint (added by migration 20260415000003). Leave a SQL comment tombstoning the "one draft per source per field" line.
  4. **Alter `metadata_versions`**: add `value_hash BYTEA NOT NULL DEFAULT '\x00'` (drop default after backfill), `match_type TEXT NOT NULL DEFAULT 'title'` (drop default after), `first_seen_at TIMESTAMPTZ DEFAULT now() NOT NULL`, `last_seen_at TIMESTAMPTZ DEFAULT now() NOT NULL`, `observation_count INTEGER DEFAULT 1 NOT NULL`. Change `source` column type from `metadata_source` ENUM → `TEXT REFERENCES metadata_sources(id)` (must first drop enum column and re-add as TEXT, or do `ALTER COLUMN source TYPE TEXT USING source::text` then add FK).
  5. **Backfill**: `UPDATE metadata_versions SET value_hash = sha256((new_value::text)::bytea)` for existing rows (this is a coarse hash; acceptable since existing rows are only OPF drafts). Drop the default.
  6. **Drop** `metadata_source` ENUM TYPE (ENUM_REBUILD pattern — rename → drop).
  7. **Add unique constraint** `UNIQUE (manifestation_id, source, field_name, value_hash)`.
  8. Indexes: `idx_mv_manifestation_field`, `idx_mv_last_seen`.
  9. **Canonical pointer columns**: `ALTER TABLE works ADD COLUMN title_version_id UUID REFERENCES metadata_versions(id) ON DELETE SET NULL, ADD COLUMN description_version_id UUID ..., ADD COLUMN language_version_id UUID ...` Same for `manifestations`: `publisher_version_id`, `pub_date_version_id`, `isbn_10_version_id`, `isbn_13_version_id`, `cover_path TEXT`, `cover_sha256 BYTEA`, `cover_size_bytes BIGINT`, `cover_source TEXT REFERENCES metadata_sources(id)`, `cover_version_id UUID REFERENCES metadata_versions(id) ON DELETE SET NULL`. Same for `work_authors.source_version_id`, `manifestation_tags.source_version_id`.
  10. **Work rematch column**: `ALTER TABLE manifestations ADD COLUMN suspected_duplicate_work_id UUID REFERENCES works(id) ON DELETE SET NULL`.
  11. **Field locks**: `CREATE TABLE field_locks (manifestation_id UUID REFERENCES manifestations(id) ON DELETE CASCADE, entity_type TEXT NOT NULL, field_name TEXT NOT NULL, locked_at TIMESTAMPTZ DEFAULT now() NOT NULL, locked_by UUID REFERENCES users(id) ON DELETE SET NULL, PRIMARY KEY (manifestation_id, entity_type, field_name))`.
  12. **Enrichment queue**: `CREATE TYPE enrichment_status AS ENUM ('pending', 'in_progress', 'complete', 'failed', 'skipped')`; `ALTER TABLE manifestations ADD COLUMN enrichment_status enrichment_status NOT NULL DEFAULT 'pending', ADD COLUMN enrichment_attempted_at TIMESTAMPTZ, ADD COLUMN enrichment_attempt_count INTEGER NOT NULL DEFAULT 0, ADD COLUMN enrichment_error TEXT`. Partial index: `CREATE INDEX idx_manifestations_enrichment_queue ON manifestations (enrichment_status, enrichment_attempted_at NULLS FIRST) WHERE enrichment_status IN ('pending', 'failed')`.
  13. **Cache kind**: `CREATE TYPE api_cache_kind AS ENUM ('hit', 'miss', 'error')`; `ALTER TABLE api_cache ADD COLUMN response_kind api_cache_kind NOT NULL DEFAULT 'hit', ADD COLUMN http_status INT`.
  14. **Grants per table** (mirror PER_ROLE_GRANTS pattern):
     - `metadata_sources`: `SELECT` → `tome_ingestion`, full DML → `tome_app`, `SELECT` → `tome_readonly`.
     - `field_locks`: full DML → `tome_app` only; `SELECT` → `tome_readonly`.
     - No grant change on rewritten `metadata_versions` (preserves existing).
- **MIRROR**: ENUM_REBUILD pattern; PER_ROLE_GRANTS pattern; MIGRATION_NAMING.
- **GOTCHA**: Enum rebuilds with dependent columns require `DROP DEFAULT` before `ALTER COLUMN TYPE` (memory: `feedback_postgres_enum_rebuild`). Backfilling `value_hash` with sha256 of `new_value::text` gives a coarse but stable hash that keeps existing OPF rows queryable. The down migration must recreate `metadata_source` and `metadata_review_status` enums and reverse-map via `SELECT INTO` so OPF drafts survive.
- **VALIDATE**: `DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev sqlx migrate run` succeeds; `sqlx migrate revert` succeeds; re-apply succeeds; existing OPF rows still queryable after backfill.

### Task 2: Rewrite `draft.rs` to new journal shape
- **STATUS**: Complete — Phase A (`4e61154`).
- **ACTION**: Replace `backend/src/services/metadata/draft.rs::insert_draft` SQL + change `write_drafts` return type.
- **IMPLEMENT**:
  - Change `write_drafts` signature to `pub async fn write_drafts(conn: &mut PgConnection, manifestation_id: Uuid, metadata: &ExtractedMetadata) -> Result<HashMap<String, Uuid>, sqlx::Error>` — accept a transaction connection (so caller controls commit) and return `{field_name → metadata_versions.id}`.
  - For each field: compute `value_hash` via the task 11 `value_hash` helper; set `match_type = 'isbn'` if the OPF data contains a validated ISBN and the field is `isbn_10`/`isbn_13`, else `'title'`.
  - New SQL: `INSERT INTO metadata_versions (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) VALUES ($1, 'opf', $2, $3, $4, $5, $6) ON CONFLICT (manifestation_id, source, field_name, value_hash) DO UPDATE SET last_seen_at = now(), observation_count = metadata_versions.observation_count + 1 RETURNING id`.
  - Delete `insert_draft`; inline the upsert; return the `RETURNING id` values.
  - Drop `opf::metadata_source` cast since `source` is now TEXT.
- **MIRROR**: UPSERT_WITH_RETURNING; REPOSITORY_PATTERN (transaction-bound).
- **IMPORTS**: `std::collections::HashMap`, `sqlx::PgConnection`, `crate::services::enrichment::value_hash`.
- **GOTCHA**: Task 11 (`value_hash`) ships in the same PR and is a dependency — write task 11 first, but it compiles fine without task 2 so order as listed. Removing `::metadata_source` cast is the one-line fix for the `ValueError` the DB will throw if you leave it. `inversion_detected` is no longer a real field — drop that branch (it was a meta-signal, not a metadata value; relocate to `ingestion_jobs.error_message` if needed).
- **VALIDATE**: `cargo test -p tome-api services::metadata::draft --features --ignored`; verify a second call with same values bumps `observation_count` rather than failing.

### Task 3: Refactor `orchestrator::process_file` for ingest invariant
- **STATUS**: Complete — Phase A (`4e61154`).
- **ACTION**: Sequence manifestation insert → draft write → pointer update inside one transaction.
- **IMPLEMENT**:
  - Open a transaction at the start of the DB section of `process_file`.
  - Step A: `INSERT INTO manifestations (work_id, format, file_path, file_hash, file_size_bytes, ingestion_status, validation_status, accessibility_metadata) VALUES (...) RETURNING id` — leave `isbn_10`, `isbn_13`, `publisher`, `pub_date` NULL; all `*_version_id` pointers NULL.
  - Step B: call `draft::write_drafts(&mut *tx, manifestation_id, meta)` — returns `HashMap<field_name, version_id>`.
  - Step C: `UPDATE manifestations SET isbn_10=$1, isbn_13=$2, publisher=$3, pub_date=$4, isbn_10_version_id=$5, isbn_13_version_id=$6, publisher_version_id=$7, pub_date_version_id=$8 WHERE id=$9` — pulling canonical values from `extracted` and version IDs from the hashmap.
  - Commit tx.
  - All three steps and the work insertion (task 4) share the same tx.
- **MIRROR**: REPOSITORY_PATTERN (tx.begin / tx.commit).
- **IMPORTS**: none new.
- **GOTCHA**: `extracted` may be `None` (non-EPUB path) — in that case step C writes no values but task 5's heuristic-fallback still writes a title journal row. When `extracted` is Some but contains None fields, step C still sets the pointers that do exist (whichever fields the hashmap contains).
- **VALIDATE**: new ingest invariant test (task 30): after `scan_once`, for every non-NULL canonical field on the manifestation, there exists a `metadata_versions` row with matching id at the `*_version_id` pointer.

### Task 4: Refactor `work::find_or_create` to wire work-level pointers
- **STATUS**: Complete — Phase A (`4e61154`).
- **ACTION**: Extend `find_or_create` to accept the HashMap from task 2 and set `works.{title,description,language}_version_id` + `work_authors.source_version_id`.
- **IMPLEMENT**:
  - New signature: `find_or_create(tx: &mut Transaction<'_, Postgres>, metadata: &ExtractedMetadata, draft_ids: &HashMap<String, Uuid>) -> Result<Uuid, sqlx::Error>`. Use an already-open transaction (no `pool.begin()` inside).
  - On "create new work" path: INSERT `works` including `title_version_id = draft_ids.get("title").copied()`, etc.
  - On `work_authors` INSERT: set `source_version_id = draft_ids.get("creators").copied()` (list-field pointer on the join row).
  - On ISBN match / title-author fuzzy match paths (existing work found): do NOT touch pointers. Existing work's values aren't being changed.
- **MIRROR**: REPOSITORY_PATTERN; UPSERT_WITH_RETURNING.
- **IMPORTS**: `std::collections::HashMap`, `sqlx::{Postgres, Transaction}`.
- **GOTCHA**: Caller (orchestrator) must open the tx before calling this. Update orchestrator's call site. Tests in `work.rs` must be updated to open a transaction first.
- **VALIDATE**: `cargo test --ignored -p tome-api models::work::tests::find_or_create_new_work` — extend to assert `works.title_version_id IS NOT NULL` and references a real `metadata_versions.id`.

### Task 5: Heuristic-fallback journal row
- **STATUS**: Complete — Phase A (`4e61154`).
- **ACTION**: In `orchestrator.rs` when `extracted.is_none()` (no OPF), write a low-confidence OPF journal row for the filename-inferred title so the canonical invariant holds.
- **IMPLEMENT**:
  - Before the fallback `INSERT INTO works (title, sort_title)` branch, fabricate a synthetic `ExtractedMetadata` with only `title: Some(filename_title)` and `confidence: 0.2`.
  - Call `draft::write_drafts(&mut *tx, manifestation_id, &synthetic)` — returns `{title: version_id}`.
  - Pass that hashmap to `find_or_create` (task 4 signature).
  - Ensure `match_type` for the row is `'title'` and `confidence_score = 0.2`.
- **MIRROR**: as task 3.
- **GOTCHA**: Don't conflate `ExtractedMetadata::confidence` (input source confidence) with journal-row `confidence_score` (output stored value). Task 2 currently writes `metadata.confidence` as-is — that's fine; here we pass `0.2`.
- **VALIDATE**: Test (task 30): ingest a file with NO OPF; assert a `metadata_versions` row exists with `source='opf', field_name='title', confidence_score=0.2` AND `works.title_version_id` points at it.

### Task 6: `work::rematch_on_isbn_change`
- **STATUS**: Complete — Phase A (`4e61154`).
- **ACTION**: Create `pub async fn rematch_on_isbn_change(tx: &mut PgConnection, manifestation_id: Uuid) -> Result<RematchOutcome, sqlx::Error>` in `backend/src/models/work.rs`.
- **IMPLEMENT**:
  - Fetch current manifestation: `work_id`, `isbn_13`, `isbn_10`.
  - Query other works holding the same ISBN: `SELECT DISTINCT m.work_id FROM manifestations m WHERE (m.isbn_13 = $1 AND $1 IS NOT NULL) OR (m.isbn_10 = $2 AND $2 IS NOT NULL) AND m.work_id != $3`.
  - If **exactly one** match AND current work has zero other manifestations (`SELECT COUNT(*) FROM manifestations WHERE work_id = $3 AND id != $4` = 0) AND zero manual drafts on current work (`SELECT COUNT(*) FROM metadata_versions mv JOIN manifestations m ON m.id = mv.manifestation_id WHERE m.work_id = $3 AND mv.source = 'manual'` = 0):
    - `UPDATE manifestations SET work_id = $matched WHERE id = $4`.
    - `DELETE FROM works WHERE id = $3` (CASCADE cleans `work_authors`, `series_works`).
    - Return `RematchOutcome::AutoMerged { from: old_work_id, to: matched }`.
  - Else if ≥1 matched work:
    - `UPDATE manifestations SET suspected_duplicate_work_id = $matched WHERE id = $4` (pick first match).
    - Return `RematchOutcome::Suspected { matched_work: matched }`.
  - Else: `Ok(RematchOutcome::NoOp)`.
  - Define: `pub enum RematchOutcome { NoOp, AutoMerged { from: Uuid, to: Uuid }, Suspected { matched_work: Uuid } }`.
- **MIRROR**: REPOSITORY_PATTERN (tx-scoped); ordering of SELECTs before UPDATE.
- **IMPORTS**: none beyond existing `sqlx`, `uuid`.
- **GOTCHA**: **All queries must run inside the caller's tx** — orchestrator calls this within the policy-apply transaction. Never call with `&PgPool` directly or you break atomicity. Use `FOR UPDATE` on the candidate work rows to avoid concurrent rematches racing on the same stub.
- **VALIDATE**: task 34 unit tests (auto-merge, suspicion, no-op paths).

### Task 7: Auth helpers `require_admin` and `require_not_child`
- **STATUS**: Complete — Phase A (`4e61154`).
- **ACTION**: Add two methods to `CurrentUser` in `backend/src/auth/middleware.rs` and migrate 5 existing call sites.
- **IMPLEMENT**:
  ```rust
  impl CurrentUser {
      pub fn require_admin(&self) -> Result<(), AppError> {
          if self.role == "admin" { Ok(()) } else { Err(AppError::Forbidden) }
      }
      pub fn require_not_child(&self) -> Result<(), AppError> {
          if self.is_child { Err(AppError::Forbidden) } else { Ok(()) }
      }
  }
  ```
  Remove the `#[allow(dead_code)]` from `role` and `is_child`.
  - Migrate: `routes/ingestion.rs:19-21` → `current_user.require_admin()?;`
  - Migrate: `routes/tokens.rs` — the three sites that implicitly assume any authenticated user can manage their own tokens (no role check currently; **do not add** `require_admin` there — audit and confirm). If audit shows they need `require_not_child`, wire that.
  - Migrate: `routes/auth.rs:163` — audit. The `/auth/me` endpoint is likely any authenticated user; don't force an admin check where one isn't required.
  - **Step 7's own routes** (tasks 25, 26) use `require_not_child()` — accept adult+admin, reject child.
- **MIRROR**: ERROR_HANDLING (AppError::Forbidden).
- **IMPORTS**: no new.
- **GOTCHA**: Memory instinct says a prior session found `role` marked `#[allow(dead_code)]` — do not resurrect it elsewhere; these two methods are the ONLY new consumers. The Blueprint's Pass A Finding F3 is specifically that role-check scattering is a smell; this step consolidates it.
- **VALIDATE**: `cargo build` — dead-code lint clears once the allow attributes are removed. task 31 tests verify child 403 / adult 200 / admin 200 on all new routes.

### Task 8: Cargo deps
- **STATUS**: Complete — Phase A/B (`a935f3e` async-trait, `cb0fd35` rate-limit + HTTP deps).
- **ACTION**: `backend/Cargo.toml` — add `governor = "0.8"`, `hickory-resolver = "0.26"` (verify current majors via Context7). Update `image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }` — add `"webp"`.
- **MIRROR**: existing Cargo.toml ordering (alphabetical within deps/dev-deps).
- **VALIDATE**: `cargo build` succeeds; run `cargo audit` — no new advisories.

### Task 9: `services/enrichment/mod.rs`
- **STATUS**: Complete — Phase B (`cb0fd35`).
- **ACTION**: Module root with public surface.
- **IMPLEMENT**:
  ```rust
  pub mod cache;
  pub mod confidence;
  pub mod cover_download;
  pub mod dry_run;
  pub mod field_lock;
  pub mod http;
  pub mod lookup_key;
  pub mod orchestrator;
  pub mod policy;
  pub mod queue;
  pub mod sources;
  pub mod value_hash;

  pub use orchestrator::run_once;
  pub use queue::spawn_queue;
  ```
- **MIRROR**: SERVICE_PATTERN (module root pattern from `services/ingestion/mod.rs`).
- **VALIDATE**: `cargo check` passes once all submodules exist.

### Task 10: `lookup_key.rs`
- **STATUS**: Complete — Phase B (`cb0fd35`).
- **ACTION**: Canonicalise raw titles/authors/ISBNs into stable cache keys.
- **IMPLEMENT**:
  ```rust
  pub fn isbn_key(raw: &str) -> Option<String> {
      // Reuse crate::services::metadata::isbn::parse_isbn. If valid, always
      // return the ISBN-13 form prefixed "isbn:". This dedupes ISBN-10/13 inputs.
      let r = crate::services::metadata::isbn::parse_isbn(raw);
      r.isbn_13.map(|s| format!("isbn:{s}"))
  }
  pub fn title_author_key(title: &str, author: &str) -> String {
      // NFKC → lowercase → whitespace-collapse → punctuation-strip
      let t = canonicalise(title);
      let a = canonicalise(author);
      format!("ta:{t}|{a}")
  }
  fn canonicalise(s: &str) -> String { /* unicode_normalization NFKC → ... */ }
  ```
- **MIRROR**: `backend/src/services/metadata/isbn.rs::normalise` for the approach.
- **IMPORTS**: `unicode-normalization` crate (add to Cargo.toml if not present; fallback: implement with `char::to_lowercase` + whitespace/punct filter).
- **GOTCHA**: Canonical keys for `"Dune"/"Frank Herbert"` and `" dune  "/"herbert, frank"` must converge — test both directions.
- **VALIDATE**: unit tests (task 29) prove ISBN-10 and ISBN-13 of the same book produce the same key; title/author whitespace and case variants produce the same key.

### Task 11: `value_hash.rs`
- **STATUS**: Complete — Phase B (`cb0fd35`).
- **ACTION**: Canonical-JSON + SHA-256 with per-field normalisation hooks.
- **IMPLEMENT**:
  ```rust
  use serde_json::Value;
  use sha2::{Digest, Sha256};
  pub fn value_hash(field_name: &str, value: &Value) -> Vec<u8> {
      let normalised = normalise(field_name, value);
      let canonical = canonical_json(&normalised);
      Sha256::digest(canonical.as_bytes()).to_vec()
  }
  fn normalise(field: &str, v: &Value) -> Value {
      match field {
          "pub_date" => /* coerce any ISO date to YYYY-MM-DD */ ...,
          "publisher" => /* trim */ ...,
          "creators" | "subjects" | "genres" | "tags" => /* sort array items */ ...,
          _ => v.clone(),
      }
  }
  fn canonical_json(v: &Value) -> String {
      // Sort object keys recursively; use serde_json::to_string with a key-sorting
      // wrapper. Avoid relying on Map insertion order.
  }
  ```
- **MIRROR**: `backend/src/services/metadata/sanitiser.rs` for field-specific normalisation approach.
- **IMPORTS**: `sha2::{Digest, Sha256}`, `serde_json::Value`.
- **GOTCHA**: `serde_json::Map` uses insertion-order by default in the `preserve_order` build; canonical JSON must sort keys explicitly. Use a recursive BTreeMap conversion.
- **VALIDATE**: unit tests (task 29): same logical value → same hash regardless of key order or whitespace; `["a","b"]` and `["b","a"]` for list fields produce same hash; different values produce different hashes.

### Task 12: `cache.rs`
- **STATUS**: Complete — Phase B (`cb0fd35`).
- **ACTION**: `api_cache` read/write with per-kind TTL and eviction on stale read.
- **IMPLEMENT**:
  - `read(pool, source, lookup_key) -> Option<CachedResponse>` — `SELECT ... WHERE (source, lookup_key) = ($1, $2) AND expires_at > now()`. On miss (expired OR absent) and pool-permission allows, `DELETE WHERE expires_at <= now() AND (source, lookup_key) = ($1, $2)` opportunistically.
  - `write(pool, source, lookup_key, response, kind, http_status, config)` — compute `expires_at = now() + (kind == hit ? ttl_hit : kind == miss ? ttl_miss : ttl_error)`. Upsert on `(source, lookup_key)`.
  - `CachedResponse { response: Value, kind: ApiCacheKind, http_status: Option<i32>, fetched_at: OffsetDateTime }`.
- **MIRROR**: UPSERT_WITH_RETURNING; time handling via `time` crate (memory: `project_time_not_chrono`).
- **GOTCHA**: TTLs come from `Config` — plumb them through. A `miss` should still be cached to prevent thrashing on dead ISBNs.
- **VALIDATE**: task 36 tests — distinct TTLs honoured; stale read does not return data.

### Task 13: `policy.rs`
- **STATUS**: Complete — Phase B (`cb0fd35`).
- **ACTION**: `FieldPolicy` enum + defaults + `decide` function.
- **IMPLEMENT**:
  ```rust
  #[derive(Copy, Clone, Debug)]
  pub enum FieldPolicy { AutoFill, Propose, Lock }
  pub fn default_policy(field: &str) -> FieldPolicy {
      match field {
          "title" | "sort_title" | "language" | "isbn_10" | "isbn_13"
          | "publisher" | "pub_date" | "cover" => FieldPolicy::AutoFill,
          "description" | "series" | "series_position"
          | "creators" | "subjects" | "genres" | "tags" => FieldPolicy::Propose,
          _ => FieldPolicy::Propose,  // unknown = conservative
      }
  }
  pub enum Decision { Apply(Uuid), Stage, NoOp }
  pub fn decide(
      field: &str,
      canonical_is_empty: bool,
      incoming_version: &MetadataVersion,
      quorum_count: u32,
      field_locked: bool,
      existing_pending: &[MetadataVersion],
  ) -> Decision {
      if field_locked { return Decision::NoOp; }
      let base = default_policy(field);
      // Downgrade AutoFill → Propose if any *pending* row on this field has a
      // different value_hash from incoming (disagreement).
      let effective = if matches!(base, FieldPolicy::AutoFill)
          && existing_pending.iter().any(|v| v.value_hash != incoming_version.value_hash) {
          FieldPolicy::Propose
      } else { base };
      match effective {
          FieldPolicy::AutoFill if canonical_is_empty => Decision::Apply(incoming_version.id),
          FieldPolicy::AutoFill => Decision::Stage,
          FieldPolicy::Propose => Decision::Stage,
          FieldPolicy::Lock => Decision::NoOp,
      }
  }
  ```
- **MIRROR**: enum + dispatcher pattern (thin, pure function; no DB).
- **GOTCHA**: Policy is pure logic — no DB access here. Disagreement detection is "among pending rows for the same field," not "among all-time rows." The Blueprint's "agreement boost at decision time" lives in `confidence.rs` (task 14), NOT here — keep separation clean.
- **VALIDATE**: task 29 tests cover each branch of `decide`.

### Task 14: `confidence.rs`
- **STATUS**: Complete — Phase B (`cb0fd35`).
- **ACTION**: `score(source, match_type, quorum_count) -> f32`.
- **IMPLEMENT**:
  ```rust
  pub fn base_source(source: &str) -> f32 {
      match source {
          "manual" => 1.00, "hardcover" => 0.85, "openlibrary" => 0.80,
          "googlebooks" => 0.75, "opf" => 0.50, "ai" => 0.30, _ => 0.30,
      }
  }
  pub fn match_modifier(match_type: &str) -> f32 {
      match match_type {
          "isbn" => 1.00, "title_author_exact" => 0.90,
          "title_author_fuzzy" => 0.75, "title" => 0.50, _ => 0.50,
      }
  }
  pub fn agreement_boost(quorum: u32) -> f32 {
      match quorum { 0 | 1 => 1.00, 2 => 1.10, _ => 1.20 }
  }
  pub fn score(source: &str, match_type: &str, quorum: u32) -> f32 {
      let raw = base_source(source) * match_modifier(match_type) * agreement_boost(quorum);
      let clamped = raw.clamp(0.0, 0.99);
      if source == "manual" { raw.min(1.00) } else { clamped }
  }
  ```
- **GOTCHA**: Only `manual` reaches 1.00; all other sources clamp at 0.99 even with agreement.
- **VALIDATE**: task 29 tests cover the clamping edge (manual alone = 1.00; openlibrary+quorum_3 stays ≤ 0.99).

### Task 15: `http.rs` — two clients
- **STATUS**: Complete — Phase B (`cb0fd35`).
- **ACTION**: `api_client()` for metadata APIs (no SSRF guard); `cover_client()` for cover downloads (SSRF-guarded redirect policy).
- **IMPLEMENT**:
  ```rust
  pub fn api_client() -> reqwest::Client {
      reqwest::ClientBuilder::new()
          .timeout(Duration::from_secs(10))
          .redirect(reqwest::redirect::Policy::limited(5))
          .build().expect("api_client")
  }
  pub fn cover_client(redirect_limit: usize, timeout_secs: u64) -> reqwest::Client {
      let policy = reqwest::redirect::Policy::custom(move |attempt| {
          if attempt.previous().len() >= redirect_limit {
              return attempt.stop();
          }
          match validate_hop(attempt.url()) {
              Ok(_) => attempt.follow(),
              Err(_) => attempt.stop(),
          }
      });
      reqwest::ClientBuilder::new()
          .timeout(Duration::from_secs(timeout_secs))
          .redirect(policy)
          .build().expect("cover_client")
  }
  fn validate_hop(url: &url::Url) -> Result<(), ()> {
      // Resolve host → IPs via hickory-resolver. For each IP, reject:
      //   IPv4: loopback (127/8), private (10/8, 172.16/12, 192.168/16),
      //         link-local (169.254/16), CGNAT (100.64/10), multicast (224/4),
      //         169.254.169.254 (EC2 metadata).
      //   IPv6: loopback (::1), link-local (fe80::/10), ULA (fc00::/7), multicast.
      // Reject if ANY resolved IP is in these classes.
  }
  ```
- **IMPORTS**: `reqwest`, `url`, `hickory_resolver`, `std::net::{IpAddr, Ipv4Addr, Ipv6Addr}`.
- **GOTCHA**: Reqwest's redirect callback is synchronous; DNS resolution must be cached or pre-resolved. Use `hickory_resolver` via a shared async resolver initialised at client-build time; store it in a `OnceCell`. The callback uses `try_lookup_host(url.host())` against the resolver. **Never trust a single IP** — multi-homed hosts can return public + private; reject if ANY returned IP is in a denied range.
- **VALIDATE**: task 37 — mock DNS returns 127.0.0.1 for a public hostname → redirect blocked. Legit public IP → passes.

### Task 16: `cover_download.rs`
- **STATUS**: Complete — Phase B (`cb0fd35`).
- **ACTION**: Streaming download with byte cap, content-type allowlist, magic-byte sniff, dimension gate, stage path.
- **IMPLEMENT**:
  - `pub async fn download(url: &str, client: &reqwest::Client, config: &Config, manifestation_id: Uuid, version_id: Uuid) -> Result<CoverArtifact, CoverError>`:
    1. GET `url`; verify `Content-Type ∈ {image/jpeg, image/png, image/webp}`.
    2. Stream body with a `max_bytes` counter; abort on overrun → `CoverError::TooLarge`.
    3. After full read: `image::guess_format(&bytes)` — verify matches content-type (reject on mismatch).
    4. Load dimensions via `image::load_from_memory_with_format` → reject if `max(width, height) < config.cover_min_long_edge_px`.
    5. Compute SHA-256 of bytes.
    6. Write to `{library}/_covers/pending/{manifestation_id}-{version_id_short}.{ext}` (mkdir -p parent). `version_id_short` = first 8 hex chars of `version_id`.
    7. Return `CoverArtifact { path, sha256, size_bytes, width, height, format }`.
  - `CoverError` variants: `TooLarge`, `WrongContentType`, `MagicByteMismatch`, `DimensionsTooSmall`, `Io(io::Error)`, `Network(reqwest::Error)`.
- **GOTCHA**: Byte cap must trigger **mid-stream**, not after download. Use `bytes_stream().try_fold(Vec::new(), |mut acc, chunk| { if acc.len() + chunk.len() > max { Err(TooLarge) } else { acc.extend(chunk); Ok(acc) } })`. Magic-byte sniff catches a malicious server sending `Content-Type: image/jpeg` but serving EXE bytes.
- **VALIDATE**: task 37 — oversized body aborts mid-stream; wrong content-type rejected; sub-threshold rejected; valid 1200×1800 JPEG accepted and staged.

### Task 17: `sources/mod.rs` — trait
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: `MetadataSource` trait + common types.
- **IMPLEMENT**:
  ```rust
  #[async_trait::async_trait]
  pub trait MetadataSource: Send + Sync {
      fn id(&self) -> &'static str;   // "openlibrary" | "googlebooks" | "hardcover"
      fn enabled(&self) -> bool;
      async fn lookup(&self, ctx: &LookupCtx, key: &LookupKey) -> Result<Vec<SourceResult>, SourceError>;
  }
  pub struct SourceResult {
      pub field_name: String,
      pub raw_value: serde_json::Value,
      pub match_type: String,
  }
  pub enum SourceError { NotFound, RateLimited { retry_after: Option<Duration> }, Http(StatusCode), Timeout, Other(anyhow::Error) }
  pub enum LookupKey { Isbn(String), TitleAuthor { title: String, author: String } }
  pub struct LookupCtx<'a> { pub http: &'a reqwest::Client, pub cache: &'a CacheHandle, /* rate limiter ref */ }
  ```
- **IMPORTS**: `async-trait` (add to Cargo.toml — commonly needed for trait + async; check existing deps first).
- **VALIDATE**: Compiles standalone.

### Task 18: `sources/open_library.rs`
- **STATUS**: Complete — Phase C (`c7d87be`); migrated to `/api/books` in `1bdc3b9`.
- **ACTION**: Adapter.
- **IMPLEMENT**:
  - ISBN path: `GET {base_url}/isbn/{isbn}.json`. 404 → `Ok(vec![])`. 200 → map to `SourceResult`s (title → `dc:title`-ish, description, publishers[0], publish_date, authors, subjects[0..n]).
  - Title+author path: `GET {base_url}/search.json?title=...&author=...&limit=5`. Take first doc. Tag with `match_type = 'title_author_fuzzy'`.
  - Rate-limited via a module-level `governor::RateLimiter` (5/min conservative).
- **GOTCHA**: Open Library returns nested structures; flatten carefully. `publish_date` is freeform text, not ISO — pass through and let `value_hash.rs` normalise.
- **VALIDATE**: task 32 `wiremock` integration tests (happy 200, 404, 429, 500, timeout).

### Task 19: `sources/google_books.rs`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: Adapter.
- **IMPLEMENT**:
  - ISBN: `GET {base_url}/volumes?q=isbn:{isbn}&maxResults=1`. If API key configured, append `&key={key}`.
  - Title+author: `GET {base_url}/volumes?q=intitle:{t}+inauthor:{a}&maxResults=5`.
  - Map `items[0].volumeInfo` → results (title, subtitle, authors, publisher, publishedDate, description, categories, industryIdentifiers → isbn_10/13).
  - Rate-limited.
- **GOTCHA**: Without API key, hard 1000 req/day across all of Tome users — keep the rate limiter on the conservative side.
- **VALIDATE**: task 32.

### Task 20: `sources/hardcover.rs`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: Adapter. GraphQL.
- **IMPLEMENT**:
  - If `config.hardcover_api_token.is_empty()` → adapter reports `enabled() == false`. Log at startup: `hardcover disabled: token not configured`.
  - POST `{base_url}` with `Authorization: Bearer {token}` and JSON body `{ "query": "query ($isbn: String!) { books_by_isbn(isbn: $isbn) { ... } }", "variables": { "isbn": "..." } }`.
  - Title/author: same GraphQL endpoint, different query.
  - Map response → `SourceResult`s.
  - Rate-limited (strictest of the three — 1 req/sec default).
- **GOTCHA**: Hardcover schema evolves; fetch current GraphQL schema via Context7 or `hardcover.app/docs` at implementation time. `books_by_isbn` may be renamed.
- **VALIDATE**: task 32.

### Task 21: `enrichment/orchestrator.rs`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: Per-manifestation flow.
- **IMPLEMENT**:
  - `run_once(pool: &PgPool, config: &Config, manifestation_id: Uuid) -> Result<RunOutcome, anyhow::Error>`:
    1. Load manifestation → canonical current values + enabled sources list.
    2. Build `LookupKey` (ISBN if present, else title+author).
    3. Parallel fan-out: `tokio::time::timeout(config.fetch_budget, futures::future::join_all(sources.iter().map(|s| s.lookup(...))))`. Per-source error isolated — a 429 from one source does not abort others.
    4. For each source result: cache-write the raw response; compute `value_hash`; upsert journal row (tx-bound).
    5. After all sources complete: for each field, compute quorum (count of journal rows with the same `value_hash` across all sources seen this run), call `confidence::score`, query existing pending rows, call `policy::decide`.
    6. For `Decision::Apply(version_id)`: in the same tx, UPDATE canonical field + `*_version_id` pointer. **If the applied field is `isbn_10` or `isbn_13`, call `work::rematch_on_isbn_change(tx, manifestation_id)` immediately**.
    7. Emit events: `metadata.applied`, `metadata.staged`, `work.duplicate_suspected` (insert into `webhook_deliveries` with `delivered_at = NULL` — Step 12 picks them up).
    8. Commit tx.
  - `spawn_queue` and `run_once` share this logic; queue calls `run_once` per claimed manifestation.
- **MIRROR**: REPOSITORY_PATTERN; CANCELLATION_TOKEN_SHUTDOWN (orchestrator receives a token from `spawn_queue`).
- **GOTCHA**: One transaction per manifestation processing run — all journal writes + all canonical updates + rematch happen atomically. Provider failures return `Err` from their `lookup`; the journal still gets partial results from succeeding providers. Dry-run (task 23) reuses steps 1–5 but stops before step 6's tx commit.
- **VALIDATE**: task 33 — multi-source agreement, disagreement-staging, empty-canonical auto-fill, locked-field rejection.

### Task 22: `queue.rs` — background worker
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: Background task with atomic claim + retry backoff + graceful shutdown.
- **IMPLEMENT**:
  ```rust
  pub async fn spawn_queue(pool: PgPool, config: Config, cancel: CancellationToken) -> anyhow::Result<()> {
      let concurrency = config.enrichment_concurrency;
      let semaphore = Arc::new(Semaphore::new(concurrency));
      loop {
          tokio::select! {
              _ = cancel.cancelled() => {
                  revert_in_progress(&pool).await?;
                  return Ok(());
              }
              _ = interval.tick() => {
                  if let Some(id) = claim_next(&pool).await? {
                      let permit = semaphore.clone().acquire_owned().await?;
                      tokio::spawn(async move {
                          let _p = permit;
                          let result = enrichment::run_once(&pool, &config, id).await;
                          finish(&pool, id, result).await;
                      });
                  }
              }
          }
      }
  }
  async fn claim_next(pool: &PgPool) -> sqlx::Result<Option<Uuid>> {
      sqlx::query_scalar(
          "WITH claimed AS (
               SELECT id FROM manifestations
               WHERE enrichment_status IN ('pending', 'failed')
                 AND (enrichment_attempted_at IS NULL OR enrichment_attempted_at < now() - retry_interval(enrichment_attempt_count))
               ORDER BY enrichment_attempted_at NULLS FIRST
               LIMIT 1
               FOR UPDATE SKIP LOCKED
           )
           UPDATE manifestations m
           SET enrichment_status = 'in_progress',
               enrichment_attempted_at = now(),
               enrichment_attempt_count = m.enrichment_attempt_count + 1
           FROM claimed WHERE m.id = claimed.id
           RETURNING m.id"
      ).fetch_optional(pool).await
  }
  ```
  - Retry backoff schedule: `[5m, 30m, 2h, 8h, 24h, 24h, 24h, 24h, 24h]` — attempts 1..10.
  - Finishing:
    - `Ok(_)` → `enrichment_status = 'complete'`, clear error.
    - `Err(RateLimited)` → `enrichment_status = 'failed'`, record `Retry-After` offset (bump `enrichment_attempted_at` forward so backoff honours it).
    - `Err(Http4xxNot429)` → `enrichment_status = 'complete'` (no retry; no 404 can fix itself).
    - Other `Err` → `enrichment_status = 'failed'`, `enrichment_error = msg`. When `enrichment_attempt_count >= config.max_attempts` → `enrichment_status = 'skipped'`.
  - `revert_in_progress`: `UPDATE manifestations SET enrichment_status = 'pending' WHERE enrichment_status = 'in_progress'`.
- **MIRROR**: CANCELLATION_TOKEN_SHUTDOWN; ADVISORY_LOCK (optional per-worker isolation); CTE atomic claim from the Blueprint.
- **GOTCHA**: Memory instinct (`global 90%`): atomic `FOR UPDATE SKIP LOCKED` in a one-statement CTE — **never** two-statement select-then-update. Must always revert `in_progress` on shutdown so a crashed worker doesn't leave rows stuck.
- **VALIDATE**: task 35 — two-worker race test; retry backoff; max-attempts → skipped; shutdown reverts in-progress.

### Task 23: `dry_run.rs`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: Preview diff without journal writes.
- **IMPLEMENT**:
  - `pub async fn preview(pool: &PgPool, config: &Config, manifestation_id: Uuid) -> Result<DryRunDiff, anyhow::Error>`: reuse `orchestrator` steps 1–5 but compute the diff in memory and return `DryRunDiff { would_apply: Vec<FieldChange>, would_stage: Vec<FieldChange>, locked: Vec<String> }`.
  - **Do** write to `api_cache` (we burned the request; caching it saves future calls).
  - **Do not** write to `metadata_versions` or canonical columns.
  - Do not call `rematch_on_isbn_change`.
- **GOTCHA**: Share the source-lookup + quorum-compute path with orchestrator via helper fns; don't duplicate.
- **VALIDATE**: task 33 dry-run case — asserts journal row count unchanged but `api_cache` count increased.

### Task 24: `field_lock.rs`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: CRUD helpers.
- **IMPLEMENT**:
  - `is_locked(pool, manifestation_id, entity_type, field) -> Result<bool, sqlx::Error>`.
  - `lock(pool, manifestation_id, entity_type, field, user_id) -> Result<(), sqlx::Error>` — upsert with `ON CONFLICT DO NOTHING`.
  - `unlock(pool, manifestation_id, entity_type, field) -> Result<bool, sqlx::Error>` — DELETE RETURNING for 404 detection.
- **GOTCHA**: Policy engine (task 13) consumes `is_locked` — but policy is pure, so caller must pre-resolve the lock state and pass it in. Keep the boundary clean.
- **VALIDATE**: inline `#[tokio::test] #[ignore]` tests.

### Task 25: `routes/enrichment.rs`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: Three routes (trigger, dry-run, status).
- **IMPLEMENT**:
  ```rust
  pub fn router() -> Router<AppState> {
      Router::new()
          .route("/api/manifestations/{id}/enrichment/trigger", post(trigger))
          .route("/api/manifestations/{id}/enrichment/dry-run", post(dry_run))
          .route("/api/enrichment/status", get(status))
  }
  async fn trigger(
      current_user: CurrentUser,
      State(state): State<AppState>,
      Path(id): Path<Uuid>,
  ) -> Result<impl IntoResponse, AppError> {
      current_user.require_not_child()?;
      sqlx::query("UPDATE manifestations SET enrichment_status = 'pending', enrichment_attempt_count = 0, enrichment_error = NULL WHERE id = $1")
          .bind(id).execute(&state.pool).await
          .map_err(|e| AppError::Internal(e.into()))?;
      Ok(StatusCode::ACCEPTED)
  }
  // dry_run: Json(enrichment::dry_run::preview(...).await?)
  // status: aggregate counts grouped by enrichment_status
  ```
- **MIRROR**: ROUTE_PATTERN.
- **VALIDATE**: task 31 authz tests; task 33 dry-run smoke.

### Task 26: `routes/metadata.rs`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: GET + accept/reject/revert/lock on both `/manifestations/{id}/metadata` and `/works/{id}/metadata`.
- **IMPLEMENT**:
  - All write routes: open tx; `SELECT ... FOR UPDATE` on owning row (manifestation or work); apply change; commit.
  - Accept: `UPDATE {table} SET {field} = (SELECT new_value FROM metadata_versions WHERE id = $1)::type, {field}_version_id = $1 WHERE id = $2`. Handle list fields differently (merge via join table).
  - Reject: `UPDATE metadata_versions SET status = 'rejected', resolved_by = $1, resolved_at = now() WHERE id = $2`.
  - Revert: `UPDATE {table} SET {field}_version_id = $1, {field} = (SELECT new_value ...) WHERE id = $2`. Pass `version_id = null` to clear.
  - Lock/unlock: wrap task 24 helpers.
  - **If accepting `isbn_10` or `isbn_13`, call `work::rematch_on_isbn_change(tx, manifestation_id)` before commit** (matches orchestrator's behaviour).
  - All routes call `require_not_child()`.
- **MIRROR**: ROUTE_PATTERN; REPOSITORY_PATTERN (FOR UPDATE).
- **GOTCHA**: The field→canonical-column mapping is verbose but static; encode it as a match-on-field_name helper that also returns which entity type (work vs manifestation) the field lives on. Reject unknown field names with `AppError::Validation`.
- **VALIDATE**: task 31 authz; task 33 accept path writes canonical + pointer; revert path restores to a prior version; manual rematch on accepted ISBN change triggers auto-merge (task 34 path).

### Task 27: Config + `.env.example`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: Add 14 env vars to `Config` + `.env.example`.
- **IMPLEMENT**: Mirror `backend/src/config.rs:60-110` parsing + defaults:
  - `enrichment_enabled: bool` (default `true`)
  - `enrichment_concurrency: u32` (default `2`, range 1–10)
  - `enrichment_poll_idle_secs: u64` (30)
  - `enrichment_fetch_budget_secs: u64` (15)
  - `enrichment_http_timeout_secs: u64` (10)
  - `enrichment_max_attempts: u32` (10)
  - `cache_ttl_hit_days: u32` (30), `cache_ttl_miss_days: u32` (7), `cache_ttl_error_mins: u32` (15)
  - `cover_max_bytes: u64` (10_485_760)
  - `cover_download_timeout_secs: u64` (30)
  - `cover_min_long_edge_px: u32` (1000)
  - `cover_redirect_limit: usize` (3)
  - `openlibrary_base_url: String`, `googlebooks_base_url: String`, `googlebooks_api_key: Option<String>`, `hardcover_base_url: String`, `hardcover_api_token: Option<String>`.
- **MIRROR**: `from_env` parsing + `ConfigError::Invalid` rejection of out-of-range values (e.g. `concurrency > 10` → error).
- **VALIDATE**: `cargo test config::tests` — extend `from_env_with_defaults` to assert new vars.

### Task 28: Wire queue into `main.rs`
- **STATUS**: Complete — Phase C (`c7d87be`).
- **ACTION**: Spawn `enrichment::spawn_queue` alongside the ingestion watcher.
- **IMPLEMENT**:
  ```rust
  let enrich_token = cancel_token.clone();
  let enrich_pool = state.pool.clone(); // tome_app pool — has webhook grants
  let enrich_config = config.clone();
  tokio::spawn(async move {
      if let Err(e) = services::enrichment::spawn_queue(enrich_pool, enrich_config, enrich_token).await {
          tracing::error!(error = %e, "enrichment queue exited with error");
      }
  });
  ```
- **MIRROR**: `backend/src/main.rs:83-95` (watcher spawn pattern).
- **GOTCHA**: Use `state.pool` (tome_app), NOT `state.ingestion_pool` — the queue writes to `webhook_deliveries` which `tome_ingestion` has no grants on (see `backend/migrations/20260412150005_system_tables.up.sql:52`).
- **VALIDATE**: Run `cargo run`; observe startup logs include both `ingestion watcher started` and `enrichment queue started`.

### Task 29: Unit tests for pure modules
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: `#[cfg(test)]` blocks in `lookup_key.rs`, `value_hash.rs`, `confidence.rs`, `policy.rs`.
- **IMPLEMENT**:
  - `lookup_key`: ISBN-10 and ISBN-13 of same book → same key; whitespace/case variants of "Dune"/"Frank Herbert" converge.
  - `value_hash`: key-order-independent; list-sorting normalisation; different values → different hashes.
  - `confidence`: each source/match-type combination; clamping at 0.99; manual alone reaches 1.00.
  - `policy`: AutoFill empty canonical → Apply; AutoFill occupied → Stage; AutoFill with disagreement → downgrades to Stage; Propose always Stage; Lock → NoOp; field_locked → NoOp regardless.
- **MIRROR**: TEST_STRUCTURE.
- **VALIDATE**: `cargo test` passes without `--ignored`; no DB required.

### Task 30: Ingest invariant tests (DB integration)
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: New `#[ignore]` tests in `backend/src/services/ingestion/orchestrator.rs::tests`.
- **IMPLEMENT**:
  - `ingest_sets_version_pointers_for_all_canonical_fields`: use `make_metadata_epub` helper; assert every non-NULL canonical field on the manifestation + work has a matching `*_version_id` pointer referencing a real `metadata_versions` row with `source='opf'`.
  - `ingest_without_opf_writes_heuristic_title_journal`: use non-EPUB or OPF-less EPUB; assert `metadata_versions` row with `source='opf', field_name='title', confidence_score=0.2` exists and `works.title_version_id` points to it.
  - `ingest_sets_work_authors_source_version_id`: assert `work_authors.source_version_id` is set for the `creators` journal row.
- **MIRROR**: TEST_STRUCTURE (DB integration), existing `scan_once_*` tests at `orchestrator.rs:795+`.
- **VALIDATE**: `cargo test --ignored -p tome-api services::ingestion::orchestrator`.

### Task 31: Authz helper tests
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: `#[cfg(test)]` in `backend/src/auth/middleware.rs` + route-level tests.
- **IMPLEMENT**:
  - Unit: `require_admin` on `{admin: Ok, adult: Err(Forbidden), child: Err(Forbidden)}`; `require_not_child` on `{admin: Ok, adult: Ok, child: Err(Forbidden)}`.
  - Route-level (`#[ignore]`): extend `routes/enrichment.rs::tests` + `routes/metadata.rs::tests` to create admin/adult/child users + device tokens, verify 403/200 matrix per route.
- **MIRROR**: `backend/src/routes/tokens.rs::tests::create_token_validates_name` for user-creation pattern.
- **GOTCHA**: Memory instinct: Basic auth test uses `format!("Basic {credentials}")`, NOT `authorization_bearer()`.
- **VALIDATE**: `cargo test auth::middleware`; `cargo test --ignored -p tome-api routes::enrichment routes::metadata`.

### Task 32: Per-adapter wiremock integration tests
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: Tests in `sources/open_library.rs`, `google_books.rs`, `hardcover.rs`.
- **IMPLEMENT**: Use `wiremock::MockServer` (already in dev-deps). Cover: 200 happy, 404, 429 with `Retry-After` header parsed, 500, timeout (configure server latency > client timeout).
- **MIRROR**: No existing wiremock tests in the repo yet — use wiremock's docs pattern.
- **VALIDATE**: `cargo test services::enrichment::sources`.

### Task 33: Orchestrator integration tests
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: `#[ignore]` tests in `enrichment/orchestrator.rs::tests`.
- **IMPLEMENT**:
  - **Multi-source agreement**: stub three sources returning the same title → assert Apply + quorum=3 boost.
  - **Disagreement**: two sources agree, third disagrees → pending pending_pending rows + title canonical untouched (task-13 downgrade fires).
  - **Empty-canonical auto-fill**: one source returns description, canonical empty → Apply (default is Propose for description → actually stages; choose a field with auto-fill default like `publisher`).
  - **Locked field rejection**: lock `title`; source returns new title → row written to journal as `status='pending'` but canonical untouched.
  - **Dry-run**: `dry_run::preview` returns diff; `metadata_versions` count unchanged; `api_cache` count incremented.
- **MIRROR**: TEST_STRUCTURE (DB integration).
- **VALIDATE**: `cargo test --ignored services::enrichment::orchestrator`.

### Task 34: Rematch tests
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: `#[ignore]` tests in `models/work.rs::tests`.
- **IMPLEMENT**:
  - `rematch_auto_merge`: stub work with 1 manifestation, no manual drafts; real work exists with correct ISBN; call rematch; assert stub work deleted, manifestation moved, no suspected_duplicate_work_id.
  - `rematch_suspected_when_multiple_manifestations`: stub work has 2 manifestations; call rematch; assert suspected_duplicate_work_id set, no deletion.
  - `rematch_suspected_when_manual_draft_exists`: stub work has a `source='manual'` metadata_version; call rematch; assert suspected, no deletion.
  - `rematch_noop_when_isbn_unique`: no other work has the ISBN; assert NoOp.
- **VALIDATE**: `cargo test --ignored models::work::tests::rematch`.

### Task 35: Queue tests
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: `#[ignore]` in `queue.rs::tests`.
- **IMPLEMENT**:
  - **Two-worker race**: spawn two `claim_next` calls concurrently against a single pending row; assert exactly one returns `Some`, other returns `None`.
  - **Retry backoff**: insert a row with `enrichment_status='failed', enrichment_attempt_count=1, enrichment_attempted_at = now() - 1min`; claim returns `None` (backoff 5min not yet elapsed). Set `enrichment_attempted_at = now() - 6min`; claim returns the row.
  - **Max-attempts**: repeatedly claim + fail; assert transition to `skipped` at `max_attempts`.
  - **Shutdown reverts**: claim a row, cancel token, assert row is back to `pending`.
- **VALIDATE**: `cargo test --ignored services::enrichment::queue`.

### Task 36: Cache tests
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: `#[ignore]` in `cache.rs::tests`.
- **IMPLEMENT**: TTLs per kind; stale entry not returned; ISBN-10 and ISBN-13 inputs dedupe via `lookup_key`.
- **VALIDATE**: `cargo test --ignored services::enrichment::cache`.

### Task 37: Cover-download safety tests
- **STATUS**: Complete — Phase D (`bd28164`).
- **ACTION**: Tests in `cover_download.rs` + `http.rs`.
- **IMPLEMENT**:
  - SSRF: mock hickory-resolver to return 127.0.0.1 for a public hostname; redirect policy blocks; cover_download returns `CoverError::Network(...)`.
  - Byte cap: mock server serves a stream of 20MB with `TOME_COVER_MAX_BYTES=10MB`; assert `TooLarge` before full download.
  - Content-type mismatch: server declares `image/jpeg` but serves PNG bytes → `MagicByteMismatch`.
  - Sub-threshold: 500×500 PNG with `min_long_edge_px=1000` → `DimensionsTooSmall`.
  - **Initial-URL SSRF pre-check**: Phase B shipped `cover_download::download` with
    the initial-URL `validate_hop` call gated by `#[cfg(not(test))]` because every
    existing test uses a wiremock server on `127.0.0.1`, which would be rejected
    by the production SSRF guard. This weakens coverage on the pre-check path —
    the production guard is only exercised via the `cover_client()` redirect
    policy, not the first hop. Task 37 must restore coverage without the
    `#[cfg(not(test))]` escape hatch. Preferred fix: add an
    `allow_private_hosts: bool` field to `DownloadConfig` (defaults `false`; tests
    set it `true`) and make the pre-check consult the flag. Delete the
    `#[cfg(not(test))]` gating in `cover_download.rs` at the same time so the
    pre-check runs in all builds. Verify a unit test that constructs a
    `DownloadConfig` with `allow_private_hosts: false` and a 127.0.0.1 URL still
    returns `CoverError::SsrfBlocked`.
- **MIRROR**: wiremock + in-process DNS mock pattern (inject a custom resolver via `hickory-resolver` config).
- **VALIDATE**: `cargo test services::enrichment::cover_download services::enrichment::http`. Confirm the `#[cfg(not(test))]` marker is no longer present in `cover_download.rs` after this task lands.

---

## Testing Strategy

### Unit Tests (no DB)

| Test | Input | Expected | Edge Case? |
|---|---|---|---|
| lookup_key::isbn_key | ISBN-10 and ISBN-13 of same book | same key | yes |
| lookup_key::title_author_key | whitespace/case variants | same key | yes |
| value_hash | two objects differing only in key order | same hash | yes |
| value_hash | list fields with reordered items | same hash | yes |
| confidence::score | manual + ISBN + quorum_3 | 1.00 exactly | edge: cap |
| confidence::score | openlibrary + title + quorum_3 | < 0.99 | edge: clamp |
| policy::decide | AutoFill + empty canonical | Apply | happy |
| policy::decide | AutoFill + occupied canonical | Stage | happy |
| policy::decide | AutoFill + disagreement among pending | Stage (downgrade) | yes |
| policy::decide | field_locked=true | NoOp | yes |
| require_admin | role != 'admin' | Forbidden | happy |
| require_not_child | is_child=true | Forbidden | happy |

### Integration Tests (DB required; `#[ignore]`)

Tasks 30, 32, 33, 34, 35, 36, 37 (see above).

### Edge Cases Checklist
- [ ] Empty OPF metadata → heuristic fallback still writes journal row (task 30).
- [ ] Same book re-ingested → journal dedups on `value_hash`, `observation_count` increments (task 30).
- [ ] Provider 429 with `Retry-After: 60` → queue honours backoff (task 35).
- [ ] Two queue workers on the same row → exactly one claims it (task 35).
- [ ] Ingestion crashes mid-process → rolled back transaction leaves no orphan rows.
- [ ] Queue crashes with in_progress rows → shutdown reverts them to pending (task 35).
- [ ] Cover URL redirects to 127.0.0.1 → blocked (task 37).
- [ ] Cover ≥ 10MB → aborted mid-stream (task 37).
- [ ] All three providers down → items stay `failed`/`pending`, no crash.
- [ ] Accept ISBN change that maps to a safe stub → auto-merge; with manual drafts → suspected (task 34).
- [ ] Child account POSTs accept/reject/revert/lock/dry-run/trigger → 403 every time (task 31).

---

## Validation Commands

### Static Analysis
```bash
cd backend
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
EXPECT: no warnings, no format diffs.

### Unit Tests (fast, no DB)
```bash
cd backend
cargo test --lib
```
EXPECT: all pass.

### Integration Tests (DB required)
```bash
# from repo root, ensure dev postgres is up
docker compose up -d

# migrate as schema owner
DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev \
  sqlx migrate run --source backend/migrations

# run ignored tests
cd backend
DATABASE_URL=postgres://tome_app:tome_app@localhost:5433/tome_dev \
DATABASE_URL_INGESTION=postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev \
cargo test --lib -- --ignored
```
EXPECT: all pass.

### Migration Round-Trip
```bash
DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev \
  sqlx migrate run --source backend/migrations
DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev \
  sqlx migrate revert --source backend/migrations
DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev \
  sqlx migrate run --source backend/migrations
```
EXPECT: migrate / revert / re-apply all succeed.

### Supply-chain
```bash
cd backend
cargo audit
```
EXPECT: no new advisories from `governor`, `hickory-resolver`, updated `image`.

### Manual Smoke (from BLUEPRINT Verification section)
- [ ] Ingest a book with valid ISBN; watch `enrichment_status` transition `pending → in_progress → complete`.
- [ ] After ingest, every non-NULL canonical field has its `*_version_id` pointer set (ingest invariant).
- [ ] Ingest OPF-less file; heuristic-fallback journal row exists at `confidence=0.2`.
- [ ] Craft a provider-disagreement case for `pub_date`; verify competing pending rows + canonical stays NULL.
- [ ] `POST /accept` with a version_id → canonical + pointer update.
- [ ] `POST /revert` with an older version_id → pointer moves back.
- [ ] `POST /dry-run` → journal unchanged, `api_cache` populated.
- [ ] Rematch auto-merge smoke (two ingests of same book, wrong ISBN first).
- [ ] Rematch suspicion smoke (with manual-source draft on stub).
- [ ] Child account gets 403 on all new endpoints.
- [ ] Force `ENRICHMENT_CONCURRENCY=10` + mock 429; queue honours `Retry-After`.

---

## Acceptance Criteria

_Copied from BLUEPRINT Exit Criteria. Do not reinterpret._

- [ ] Journal: every distinct `(source, field, value)` stored once; dedup via `value_hash` confirmed under repeated ingestion.
- [ ] Sources: registry has six rows; Hardcover auto-disables without token; a seventh source = INSERT + adapter (no migration).
- [ ] Canonical pointers: every displayed value traceable via its `*_version_id`.
- [ ] Ingest invariant holds on every ingest path including OPF-less heuristic.
- [ ] Policy: auto-fill fills empty, propose stages, lock rejects all; list fields union-merge.
- [ ] Quorum: multi-source agreement boosts confidence; disagreement downgrades auto-fill → propose (no silent winner).
- [ ] Queue: `FOR UPDATE SKIP LOCKED` claim; two-worker race shows no double-processing; retry backoff → skipped; shutdown reverts in_progress.
- [ ] Cache: per-kind TTLs; canonicalised `lookup_key` dedupes ISBN-10/13.
- [ ] Cover safety: SSRF rejects 127.0.0.1; byte cap mid-stream; content-type mismatch; sub-threshold rejected.
- [ ] Work rematch auto-merges on safe stub; otherwise sets `suspected_duplicate_work_id`.
- [ ] Authz: `require_admin`/`require_not_child` are the only role-check pattern; existing sites migrated; child 403 on every enrichment+metadata endpoint.
- [ ] API: accept/reject/revert/lock/dry-run endpoints work; `SELECT ... FOR UPDATE` on every write path's owning row.
- [ ] Accepted covers at `${library}/_covers/{manifestation_id}.{ext}`; rejected stays in `_covers/pending/` (sweep is Step 11).
- [ ] Graceful degradation: all three providers down → no crash, no cache poisoning.

## Completion Checklist

Phase progress on `feat/metadata-enrichment-pipeline` (unmerged):

- Phase A — schema + ingest invariant (Tasks 1–7): landed in `4e61154`.
- Phase B — pure-logic modules + SSRF-safe cover pipeline (Tasks 8–16): landed in `a935f3e` + `cb0fd35`.
- Phase C — source adapters, orchestrator, queue, routes (Tasks 17–28): landed in `c7d87be`; OpenLibrary `/api/books` + identified User-Agent follow-up in `1bdc3b9`.
- Phase D — integration coverage (Tasks 29–37): landed in `bd28164`.

- [x] All tasks 1–37 complete and checked in (see per-task `STATUS` lines).
- [x] Code follows patterns in **Patterns to Mirror**.
- [x] Error handling uses `AppError` variants — no ad-hoc `StatusCode::...` at route handlers.
- [x] Logging uses `tracing::{info,warn,error}!` with structured fields; no `println!`.
- [x] Tests follow TEST_STRUCTURE; DB tests `#[ignore]`-gated.
- [x] No hardcoded URLs, timeouts, or limits outside `Config`.
- [ ] Migration round-trip succeeds; `sqlx migrate revert` leaves DB in pre-migration state modulo preserved journal rows.
- [ ] PR description includes "Depends on Step 6" and lists env vars added.
- [ ] `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo test --ignored` all pass. *(Phase D reported 1 pre-existing `lock_unlock_roundtrip` failure and 2 pre-existing clippy errors in `db.rs` + `cleanup.rs` — resolve before PR.)*
- [ ] `cargo audit` has no new advisories.
- [ ] Documentation (`backend/CLAUDE.md`) has no new entries required — conventions already documented.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `value_hash` collisions across differently-shaped inputs of the same field | Low | High (dedup fails, double-counting quorum) | Field-specific normalisation in `value_hash.rs`; tests cover key-order, whitespace, list-sort |
| Hardcover GraphQL schema drift | Medium | Medium | Adapter auto-disables on token absence; fetch live schema via Context7 at implementation; wiremock-gated test matrix |
| SSRF bypass via DNS rebinding | Low | High | `validate_hop` resolves hostname at each redirect; rejects any IP in denied class; all redirect policy executed in cover_client only |
| Enum rebuild loses data during migration | Low | High | Backfill `value_hash` with deterministic hash of existing `new_value`; mapping preserves rejected/legacy; `.down.sql` re-creates original enum via `SELECT INTO` |
| Queue starves on fetch-budget timeout | Medium | Low | Per-source timeout inside fetch budget means one slow provider still yields to others; timeout is a wall-clock budget, not a request budget |
| Provider rate-limit collisions across Tome instances (shared API quota) | Low | Medium | Conservative default quotas; env var tunable; document in `.env.example` |
| Work rematch deletes a stub with undetected user edits | Medium | High | Safe-stub check requires zero manifestations (besides current) AND zero manual drafts; otherwise suspected_duplicate_work_id set, user confirms |

## Notes

- **Branch**: `feat/metadata-enrichment` (per BLUEPRINT).
- **Depends on**: Step 6 merged. Confirm `draft.rs` (from Step 6) is in its current
  "one-draft-per-source-per-field" shape; Step 7 rewrites it.
- **Blocks**: Step 8 (writeback reads canonical pointers); Step 10 (frontend UI
  consumes `/api/manifestations/:id/metadata`); Step 11 (Library Health surfaces
  `suspected_duplicate_work_id` + cover sweep); Step 12 (webhooks consume
  `work.duplicate_suspected`).
- **Tooling & environment**: dev postgres on port 5433 (see `backend/CLAUDE.md`);
  `tome` role runs migrations, `tome_app` is the web-app runtime role,
  `tome_ingestion` is the background-pipeline role. Run `docker compose up -d` to
  start the stack.
- **Memory-instinct callouts** (from `~/.claude/projects/-home-coder-Tome/memory/`):
  - `project_enrichment_architecture.md` — layer boundaries and status simplification
  - `feedback_postgres_enum_rebuild` — DROP DEFAULT / SET DEFAULT order
  - `project_schema_evolution` — pre-release schema is freely mutable; add migrations now
  - `project_time_not_chrono` — use `time` crate, not `chrono`
  - `feedback_pgtrgm_test_titles` — distinct vocabulary in fuzzy-match tests
  - `global 90% atomic claim` — `FOR UPDATE SKIP LOCKED` in one CTE, never two statements
  - `global 85% timing-safe token compare` — iterate ALL candidates, store match, return after loop
- **Adversarial-review traceability**: Pass A Findings F1 (ingest invariant), F2
  (draft rewrite), F3 (authz helpers), F4 (work rematch) all addressed by
  tasks 3–7 inside this PR. Do not defer any.
- **Open Questions**: none at PRP generation; design axes are locked per memory
  `project_enrichment_architecture.md` (2026-04-17).
