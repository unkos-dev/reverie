# Feature: Migration Rollup

## Summary

Consolidate 27 incremental migration pairs (54 files, ~1187 lines of up-SQL) into a single initial migration that creates the current schema from scratch. Generated mechanically via `pg_dump` against a database with all existing migrations applied, not hand-written. Verified by schema-diff, sqlx cache round-trip, and full test suite.

## User Story

As a contributor
I want a single base migration representing the current schema
So that the migration history reflects intentional schema design rather than 6 weeks of incremental evolution

## Problem Statement

27 migrations accumulated during steps 1–11 of the MVP build. Intermediate states (enum rebuilds, column renames, constraint additions) add cognitive load and are no longer meaningful. Pre-v1.0, no deployed databases exist that need incremental upgrade paths.

## Solution Statement

Replace all migration files with one timestamped `initial_schema` migration pair. Use `pg_dump` to mechanically extract the final DDL + seed data, avoiding transcription errors across 1187 lines of SQL. Validate by comparing `pg_dump --schema-only` output from both approaches.

## Metadata

| Field            | Value                                 |
| ---------------- | ------------------------------------- |
| Type             | REFACTOR                              |
| Complexity       | MEDIUM                                |
| Systems Affected | backend/migrations, backend/.sqlx, CI |
| Dependencies     | sqlx-cli 0.8.6, pg_dump (postgres 18) |
| Estimated Tasks  | 9                                     |
| Prerequisite     | 11f PR merged to main                 |

---

## UX Design

### Before State

```text
backend/migrations/
├── 20260412150001_extensions_enums_and_roles.{up,down}.sql
├── 20260412150002_core_tables.{up,down}.sql
├── ... (25 more pairs, 54 files total)
├── 20260526015539_settings.{up,down}.sql
└── _sqlx_migrations table: 27 rows tracking incremental history
```

### After State

```text
backend/migrations/
├── 20260526000000_initial_schema.up.sql    ← pg_dump-derived, complete schema
├── 20260526000000_initial_schema.down.sql  ← hand-written tear-down
└── _sqlx_migrations table: 1 row
```

### Interaction Changes

| Location              | Before                                   | After                                  | User Impact                             |
| --------------------- | ---------------------------------------- | -------------------------------------- | --------------------------------------- |
| `backend/migrations/` | 54 files, 27 pairs                       | 2 files, 1 pair                        | Easier to understand schema at a glance |
| Dev DB setup          | `sqlx migrate run` applies 27 migrations | `sqlx migrate run` applies 1 migration | Faster fresh setup                      |
| `_sqlx_migrations`    | 27 rows                                  | 1 row                                  | Clean history                           |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File                                                                     | Lines   | Why Read This                                                |
| -------- | ------------------------------------------------------------------------ | ------- | ------------------------------------------------------------ |
| P0       | `backend/migrations/20260412150001_extensions_enums_and_roles.up.sql`    | all     | Extension + enum creation order                              |
| P0       | `backend/migrations/20260417000001_add_enrichment_pipeline.up.sql`       | all     | Largest migration: enum rebuilds, table rewrites, seed data  |
| P0       | `backend/migrations/20260419000001_add_writeback_pipeline.up.sql`        | all     | RLS system policies, column renames                          |
| P0       | `backend/migrations/20260421000002_writeback_system_context_guc.up.sql`  | all     | Final RLS policy replacements                                |
| P0       | `backend/migrations/20260428000001_activate_reading_state.up.sql`        | all     | Table replacement (reading_positions → reading_state)        |
| P0       | `backend/migrations/20260507000001_tower_sessions_postgres_store.up.sql` | all     | Separate schema (tower_sessions)                             |
| P0       | `backend/migrations/20260526015539_settings.up.sql`                      | all     | Singleton table + pg_notify trigger + seed row               |
| P1       | `docker/init-roles.sql`                                                  | all     | Role creation — NOT part of migrations, must remain separate |
| P1       | `.github/workflows/ci.yml`                                               | 86-226  | CI migration + sqlx prepare steps                            |
| P2       | `backend/src/test_support.rs`                                            | 259-334 | Test pool helpers — depend on role grants in migrations      |

---

## Architecture Decision: `./migrations` Path Retained

**APPROACH_CHOSEN**: Single consolidated migration in `backend/migrations/`

**RATIONALE**: 247 test functions reference `#[sqlx::test(migrations = "./migrations")]`. Changing to a `schema.sql` or alternative path would require touching 247 test attribute annotations across the entire backend for zero functional benefit.

**ALTERNATIVES_REJECTED:**

- `backend/src/db/schema.sql` standalone file: rejected because sqlx expects migrations in the `migrations/` directory. Would require custom migration runner or abandoning `sqlx::test` macro.
- Multiple logical groupings (e.g., `001_schema.up.sql`, `002_grants.up.sql`): rejected because the goal is consolidation, not re-organization. One file, one transaction.

---

## NOT Building (Scope Limits)

- **No changes to `docker/init-roles.sql`** — role creation is not a migration concern
- **No changes to `.github/workflows/ci.yml`** — CI steps work unchanged (same directory, same commands)
- **No changes to test code** — `#[sqlx::test(migrations = "./migrations")]` path unchanged
- **No schema changes** — consolidated migration must produce byte-identical schema to running all 27
- **No down-migration testing infrastructure** — existing pattern ships both directions but tests only exercise up

---

## Step-by-Step Tasks

### Task 1: Capture reference schema dump

**ACTION**: Dump the current schema from a fresh database with all 27 migrations applied. This becomes the ground truth.

**IMPLEMENT**:

```bash
# From repo root. Assumes dev postgres running on 5433.
# Drop and recreate to ensure clean state
docker exec reverie-postgres psql -U reverie -d postgres -c "DROP DATABASE IF EXISTS reverie_rollup_ref"
docker exec reverie-postgres psql -U reverie -d postgres -c "CREATE DATABASE reverie_rollup_ref"

# Seed roles (needed for GRANTs in migrations)
docker exec reverie-postgres psql -U reverie -d reverie_rollup_ref -c "
  CREATE ROLE reverie_app WITH LOGIN PASSWORD 'reverie_app';
  CREATE ROLE reverie_ingestion WITH LOGIN PASSWORD 'reverie_ingestion';
  CREATE ROLE reverie_readonly WITH LOGIN PASSWORD 'reverie_readonly';
" 2>/dev/null || true
# Roles may already exist from dev DB — ignore "already exists" errors

# Apply all 27 migrations
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_rollup_ref sqlx migrate run --source backend/migrations

# Capture reference DDL (schema-only, no owner annotations)
docker exec reverie-postgres pg_dump -U reverie --schema-only --no-owner --no-privileges reverie_rollup_ref > /tmp/ref_schema.sql

# Capture reference with privileges (for grant verification)
docker exec reverie-postgres pg_dump -U reverie --schema-only --no-owner reverie_rollup_ref > /tmp/ref_schema_with_grants.sql

# Capture seed data (metadata_sources + settings)
docker exec reverie-postgres pg_dump -U reverie --data-only --table=metadata_sources --table=settings --inserts reverie_rollup_ref > /tmp/ref_seed_data.sql
```

**VALIDATE**: Files exist and are non-empty: `wc -l /tmp/ref_schema.sql /tmp/ref_schema_with_grants.sql /tmp/ref_seed_data.sql`

**GOTCHA**: `CREATE ROLE` may fail if roles already exist from dev DB. Use `2>/dev/null || true`. Alternatively, connect to a pristine postgres instance.

---

### Task 2: Generate consolidated up migration

**ACTION**: Create `backend/migrations/20260526000000_initial_schema.up.sql` from the `pg_dump` output.

**IMPLEMENT**:

1. Start from `pg_dump --schema-only --no-owner` output of the reference DB
2. Clean up the dump:
   - Remove `pg_dump` header/footer comments (version, timestamp)
   - Remove `SET` statements that pg_dump prepends (`SET statement_timeout`, `SET lock_timeout`, etc.)
   - Remove `SELECT pg_catalog.set_config('search_path', ...)` lines
   - Keep `CREATE EXTENSION`, `CREATE TYPE`, `CREATE TABLE`, `CREATE INDEX`, `CREATE FUNCTION`, `CREATE TRIGGER`, `ALTER TABLE` (constraints, RLS), `CREATE POLICY`, `CREATE SCHEMA` statements
3. Add grant statements — Task 1's first dump uses `--no-privileges` which strips grants. Extract from `ref_schema_with_grants.sql` (Task 1's second dump, which omits `--no-privileges`). Cross-reference against the grant matrix (Tables + Grant Matrix section) for completeness:
   - `GRANT USAGE ON SCHEMA public TO reverie_app, reverie_ingestion, reverie_readonly`
   - Per-table GRANTs (per grant matrix below)
   - `tower_sessions` schema grants
4. Add seed data at the end:
   - `metadata_sources` INSERT rows (6 rows: opf, manual, openlibrary, googlebooks, hardcover, ai)
   - `settings` INSERT DEFAULT VALUES
5. Add header comment explaining the rollup

**STRUCTURE of consolidated file** (dependency order):

```sql
-- Consolidated initial schema for Reverie.
-- Generated by pg_dump from the result of running migrations
-- 20260412150001 through 20260526015539, then cleaned and
-- augmented with grants and seed data.
--
-- Rollup PR: chore/migration-rollup

-- 1. Extensions
CREATE EXTENSION IF NOT EXISTS "pg_trgm";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- 2. Enum types (final values only — no rebuilds needed)
CREATE TYPE user_role AS ENUM (...);
CREATE TYPE author_role AS ENUM (...);
-- ... all 12 active enum types with final values

-- 3. Schemas
CREATE SCHEMA IF NOT EXISTS tower_sessions;

-- 4. Tables (FK-dependency order)
CREATE TABLE users (...);
CREATE TABLE works (...);
-- ... all tables in correct order

-- 5. Indexes (non-PK, non-unique-constraint)
CREATE INDEX idx_works_search_vector ...;
-- ... all indexes

-- 6. Functions
CREATE OR REPLACE FUNCTION set_updated_at() ...;
CREATE OR REPLACE FUNCTION works_search_vector_update() ...;
CREATE OR REPLACE FUNCTION set_shelves_updated_at() ...;
CREATE OR REPLACE FUNCTION notify_settings_changed() ...;

-- 7. Triggers
CREATE TRIGGER trg_users_updated_at ...;
-- ... all 7 triggers

-- 8. RLS
ALTER TABLE manifestations ENABLE ROW LEVEL SECURITY;
ALTER TABLE reading_state ENABLE ROW LEVEL SECURITY;
-- ... all 9 policies

-- 9. Grants
GRANT USAGE ON SCHEMA public TO ...;
-- ... per-table grants

-- 10. Seed data
INSERT INTO metadata_sources (...) VALUES ...;
INSERT INTO settings DEFAULT VALUES;
```

**MIRROR**: pg_dump output provides the canonical DDL; grants extracted from `ref_schema_with_grants.sql` (Task 1 second dump), cross-referenced against grant matrix.

**VALIDATE**: File is valid SQL: `docker exec reverie-postgres psql -U reverie -d postgres -c "SELECT 1"` (connectivity check)

**GOTCHA**: `pg_dump` output may reorder columns alphabetically in some versions — verify column order matches CREATE TABLE statements in original migrations (positional column order matters for `query_as!` structs if any use positional binding). In practice, pg_dump preserves creation order.

---

### Task 3: Write consolidated down migration

**ACTION**: Create `backend/migrations/20260526000000_initial_schema.down.sql` — hand-written reverse of entire schema.

**IMPLEMENT**: Drop in reverse dependency order:

```sql
-- Reverse of 20260526000000_initial_schema.up.sql
-- Drops everything created by the consolidated schema.

-- 1. Triggers (before dropping functions they reference)
DROP TRIGGER IF EXISTS settings_changed_trigger ON settings;
DROP TRIGGER IF EXISTS shelves_set_updated_at ON shelves;
DROP TRIGGER IF EXISTS trg_reading_state_updated_at ON reading_state;
DROP TRIGGER IF EXISTS trg_works_search_vector ON works;
DROP TRIGGER IF EXISTS trg_manifestations_updated_at ON manifestations;
DROP TRIGGER IF EXISTS trg_works_updated_at ON works;
DROP TRIGGER IF EXISTS trg_users_updated_at ON users;

-- 2. Functions
DROP FUNCTION IF EXISTS notify_settings_changed();
DROP FUNCTION IF EXISTS set_shelves_updated_at();
DROP FUNCTION IF EXISTS works_search_vector_update();
DROP FUNCTION IF EXISTS set_updated_at();

-- 3. Tables (FK-reverse order, CASCADE not needed if order correct)
DROP TABLE IF EXISTS settings;
DROP TABLE IF EXISTS tower_sessions.session;
DROP TABLE IF EXISTS writeback_jobs;
DROP TABLE IF EXISTS reading_state;
DROP TABLE IF EXISTS reading_sessions;
DROP TABLE IF EXISTS webhook_deliveries;
DROP TABLE IF EXISTS webhooks;
DROP TABLE IF EXISTS ingestion_jobs;
DROP TABLE IF EXISTS api_cache;
DROP TABLE IF EXISTS device_tokens;
DROP TABLE IF EXISTS shelf_items;
DROP TABLE IF EXISTS shelves;
DROP TABLE IF EXISTS field_locks;
DROP TABLE IF EXISTS manifestation_tags;
DROP TABLE IF EXISTS tags;
DROP TABLE IF EXISTS metadata_versions;
DROP TABLE IF EXISTS metadata_sources;
DROP TABLE IF EXISTS omnibus_contents;
DROP TABLE IF EXISTS series_works;
DROP TABLE IF EXISTS series;
DROP TABLE IF EXISTS manifestations;
DROP TABLE IF EXISTS work_authors;
DROP TABLE IF EXISTS authors;
DROP TABLE IF EXISTS works;
DROP TABLE IF EXISTS users;

-- 4. Schemas
DROP SCHEMA IF EXISTS tower_sessions;

-- 5. Enum types
DROP TYPE IF EXISTS theme_preference;
DROP TYPE IF EXISTS writeback_status;
DROP TYPE IF EXISTS api_cache_kind;
DROP TYPE IF EXISTS enrichment_status;
DROP TYPE IF EXISTS job_status;
DROP TYPE IF EXISTS tag_type;
DROP TYPE IF EXISTS metadata_review_status;
DROP TYPE IF EXISTS ingestion_status;
DROP TYPE IF EXISTS validation_status;
DROP TYPE IF EXISTS manifestation_format;
DROP TYPE IF EXISTS author_role;
DROP TYPE IF EXISTS user_role;

-- 6. Extensions
DROP EXTENSION IF EXISTS "pgcrypto";
DROP EXTENSION IF EXISTS "pg_trgm";

-- 7. Revoke schema-level grants
REVOKE USAGE ON SCHEMA public FROM reverie_app, reverie_ingestion, reverie_readonly;
```

**VALIDATE**: SQL syntax valid. Idempotent (`IF EXISTS` everywhere).

**GOTCHA**: RLS policies are dropped implicitly when their table is dropped. No need for explicit `DROP POLICY` statements if tables are dropped. However, if you want the down migration to be re-runnable without dropping tables, explicit policy drops would be needed — but the full down migration drops everything, so implicit is fine.

---

### Task 4: Delete all existing migration files

**ACTION**: Remove all 54 files from `backend/migrations/`.

**IMPLEMENT**:

```bash
# Remove all existing migrations
rm backend/migrations/20260412150001_extensions_enums_and_roles.{up,down}.sql
rm backend/migrations/20260412150002_core_tables.{up,down}.sql
rm backend/migrations/20260412150003_series_and_metadata.{up,down}.sql
rm backend/migrations/20260412150004_user_features.{up,down}.sql
rm backend/migrations/20260412150005_system_tables.{up,down}.sql
rm backend/migrations/20260412150006_triggers_and_functions.{up,down}.sql
rm backend/migrations/20260412150007_search_rls_and_reserved.{up,down}.sql
rm backend/migrations/20260414000001_add_session_version.{up,down}.sql
rm backend/migrations/20260414100001_add_skipped_job_status.{up,down}.sql
rm backend/migrations/20260415000001_epub_validation.{up,down}.sql
rm backend/migrations/20260415000002_unique_authors_series.{up,down}.sql
rm backend/migrations/20260415000003_unique_hash_and_drafts.{up,down}.sql
rm backend/migrations/20260416000001_remove_invalid_validation_status.{up,down}.sql
rm backend/migrations/20260417000001_add_enrichment_pipeline.{up,down}.sql
rm backend/migrations/20260417000002_grant_field_locks_select_ingestion.{up,down}.sql
rm backend/migrations/20260419000001_add_writeback_pipeline.{up,down}.sql
rm backend/migrations/20260421000001_serialise_writeback_per_manifestation.{up,down}.sql
rm backend/migrations/20260421000002_writeback_system_context_guc.{up,down}.sql
rm backend/migrations/20260427000001_add_theme_preference.{up,down}.sql
rm backend/migrations/20260428000001_activate_reading_state.{up,down}.sql
rm backend/migrations/20260506000001_metadata_versions_new_value_not_null.{up,down}.sql
rm backend/migrations/20260506000002_metadata_versions_confidence_score_not_null.{up,down}.sql
rm backend/migrations/20260507000001_tower_sessions_postgres_store.{up,down}.sql
rm backend/migrations/20260523015643_idx_works_sort_title.{up,down}.sql
rm backend/migrations/20260524044439_shelves_updated_at.{up,down}.sql
rm backend/migrations/20260525000001_users_email_unique_index.{up,down}.sql
rm backend/migrations/20260526015539_settings.{up,down}.sql
```

**VALIDATE**: `ls backend/migrations/` shows only `20260526000000_initial_schema.{up,down}.sql`

---

### Task 5: Schema-diff verification (the acid test)

**ACTION**: Apply consolidated migration to a fresh database, then compare `pg_dump --schema-only` output against the reference from Task 1.

**IMPLEMENT**:

```bash
# Create fresh test database
docker exec reverie-postgres psql -U reverie -d postgres -c "DROP DATABASE IF EXISTS reverie_rollup_test"
docker exec reverie-postgres psql -U reverie -d postgres -c "CREATE DATABASE reverie_rollup_test"

# Seed roles (idempotent — may already exist)
docker exec reverie-postgres psql -U reverie -d reverie_rollup_test -c "
  CREATE ROLE reverie_app WITH LOGIN PASSWORD 'reverie_app';
  CREATE ROLE reverie_ingestion WITH LOGIN PASSWORD 'reverie_ingestion';
  CREATE ROLE reverie_readonly WITH LOGIN PASSWORD 'reverie_readonly';
" 2>/dev/null || true

# Apply consolidated migration
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_rollup_test sqlx migrate run --source backend/migrations

# Dump new schema
docker exec reverie-postgres pg_dump -U reverie --schema-only --no-owner --no-privileges reverie_rollup_test > /tmp/new_schema.sql

# Compare (strip pg_dump timestamps/versions from both)
grep -v '^--' /tmp/ref_schema.sql | grep -v '^$' | sort > /tmp/ref_normalized.sql
grep -v '^--' /tmp/new_schema.sql | grep -v '^$' | sort > /tmp/new_normalized.sql
diff /tmp/ref_normalized.sql /tmp/new_normalized.sql
```

**VALIDATE**: `diff` output is empty (schemas identical). If not empty, fix the consolidated migration and re-run.

**GOTCHA**: `_sqlx_migrations` table will differ (27 rows vs 1 row) — exclude from comparison. pg_dump's `--schema-only` does not include `_sqlx_migrations` data, but it may include the table definition. Filter it: `grep -v '_sqlx_migrations'` from both dumps if needed.

Also verify seed data:

```bash
docker exec reverie-postgres psql -U reverie -d reverie_rollup_test -c "SELECT id, display_name, kind FROM metadata_sources ORDER BY id"
docker exec reverie-postgres psql -U reverie -d reverie_rollup_ref -c "SELECT id, display_name, kind FROM metadata_sources ORDER BY id"
# Must match exactly

docker exec reverie-postgres psql -U reverie -d reverie_rollup_test -c "SELECT count(*) FROM settings"
# Must be 1
```

---

### Task 6: Schema review via database-reviewer agent

**ACTION**: Run `database-reviewer` agent against the consolidated migration for expert schema review before committing to dev database.

**IMPLEMENT**: Invoke `database-reviewer` agent on `backend/migrations/20260526000000_initial_schema.up.sql` with context: "Review this consolidated initial schema migration for: dropped constraints, index gaps, enum ordering, RLS policy correctness, grant completeness, and FK dependency ordering."

**VALIDATE**: All reviewer findings addressed or explicitly accepted.

**GOTCHA**: Run AFTER schema diff passes (Task 5) — reviewing a broken migration wastes a pass.

---

### Task 7: Refresh dev database and regenerate sqlx cache

**ACTION**: Drop/recreate dev database and regenerate `backend/.sqlx/` cache.

**IMPLEMENT**:

```bash
# Drop and recreate dev database
docker exec reverie-postgres psql -U reverie -d postgres -c "DROP DATABASE IF EXISTS reverie_dev"
docker exec reverie-postgres psql -U reverie -d postgres -c "CREATE DATABASE reverie_dev"

# Seed roles (idempotent)
docker cp docker/init-roles.sql reverie-postgres:/tmp/init-roles.sql
docker exec reverie-postgres psql -U reverie -d reverie_dev -f /tmp/init-roles.sql 2>/dev/null || true

# Apply consolidated migration
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev sqlx migrate run --source backend/migrations

# Regenerate sqlx offline cache
cd backend && DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo sqlx prepare -- --tests

# Verify cache is fresh
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo sqlx prepare --check -- --tests
```

**VALIDATE**: `cargo sqlx prepare --check -- --tests` exits 0.

**GOTCHA**: Must run from `backend/` directory. The `-- --tests` suffix is critical — without it, test-only query macros won't be validated. Cache file count may change slightly if pg_dump column ordering differs from original — but the content should be functionally identical.

---

### Task 8: Run full test suite

**ACTION**: Execute complete test suite to verify no regressions.

**IMPLEMENT**:

```bash
cd backend

# Format check
cargo fmt --check

# Lint
cargo clippy -- -D warnings

# Full test suite (247 DB-backed tests + unit tests)
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo nextest run --workspace
```

**VALIDATE**: All three commands exit 0. Zero test failures.

**GOTCHA**: Each `#[sqlx::test(migrations = "./migrations")]` test creates its own database and runs the consolidated migration. If any test fails, it means the consolidated schema doesn't match what the test expects — likely a missing column, wrong default, or incorrect constraint. Fix the consolidated migration, not the test.

---

### Task 9: Clean up verification databases

**ACTION**: Remove temporary databases created during verification.

**IMPLEMENT**:

```bash
docker exec reverie-postgres psql -U reverie -d postgres -c "DROP DATABASE IF EXISTS reverie_rollup_ref"
docker exec reverie-postgres psql -U reverie -d postgres -c "DROP DATABASE IF EXISTS reverie_rollup_test"
rm -f /tmp/ref_schema.sql /tmp/ref_schema_with_grants.sql /tmp/ref_seed_data.sql /tmp/new_schema.sql /tmp/ref_normalized.sql /tmp/new_normalized.sql
```

**VALIDATE**: Databases gone, temp files cleaned.

---

## Patterns to Mirror

**MIGRATION_FILENAME:**

```text
// SOURCE: backend/migrations/ (existing convention)
// PATTERN: YYYYMMDDHHMMSS_slug.{up,down}.sql
// USE: 20260526000000_initial_schema.{up,down}.sql
```

**GRANT_PATTERN:**

```sql
-- SOURCE: backend/migrations/20260412150002_core_tables.up.sql:55-68
-- PATTERN: Per-table, three-role structure
GRANT SELECT, INSERT, UPDATE, DELETE ON users TO reverie_app;
GRANT SELECT ON users TO reverie_readonly;
-- (no ingestion access for user tables)
```

**RLS_POLICY_PATTERN:**

```sql
-- SOURCE: backend/migrations/20260412150007_search_rls_and_reserved.up.sql:45-100
-- PATTERN: ALTER TABLE ... ENABLE ROW LEVEL SECURITY; then CREATE POLICY per operation
ALTER TABLE manifestations ENABLE ROW LEVEL SECURITY;
CREATE POLICY manifestations_select_adult ON manifestations FOR SELECT TO reverie_app, reverie_readonly USING (...);
```

**TRIGGER_PATTERN:**

```sql
-- SOURCE: backend/migrations/20260412150006_triggers_and_functions.up.sql:1-30
-- PATTERN: CREATE OR REPLACE FUNCTION ... CREATE TRIGGER
CREATE OR REPLACE FUNCTION set_updated_at() RETURNS TRIGGER AS $$ ... $$;
CREATE TRIGGER trg_<table>_updated_at BEFORE UPDATE ON <table> FOR EACH ROW EXECUTE FUNCTION set_updated_at();
```

---

## Files to Change

| File                                                        | Action | Justification                                                                             |
| ----------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------- |
| `backend/migrations/20260526000000_initial_schema.up.sql`   | CREATE | Consolidated schema — all DDL, grants, seed data                                          |
| `backend/migrations/20260526000000_initial_schema.down.sql` | CREATE | Full tear-down in reverse dependency order                                                |
| `backend/migrations/20260412*.sql` (14 files)               | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260414*.sql` (4 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260415*.sql` (6 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260416*.sql` (2 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260417*.sql` (4 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260419*.sql` (2 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260421*.sql` (4 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260427*.sql` (2 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260428*.sql` (2 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260506*.sql` (4 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/20260507*.sql` (2 files)                | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/202605230*.sql` (2 files)               | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/202605240*.sql` (2 files)               | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/202605250*.sql` (2 files)               | DELETE | Replaced by consolidated migration                                                        |
| `backend/migrations/202605260*.sql` (2 files)               | DELETE | Replaced by consolidated migration                                                        |
| `backend/.sqlx/*.json` (~286 files)                         | UPDATE | Regenerated by `cargo sqlx prepare` — content may differ due to migration checksum change |

---

## Testing Strategy

### Verification Layers

| Layer       | Command                                    | What It Catches                               |
| ----------- | ------------------------------------------ | --------------------------------------------- |
| Schema diff | `diff ref_schema.sql new_schema.sql`       | Missing DDL, wrong types, dropped constraints |
| Seed data   | psql queries on metadata_sources, settings | Missing or wrong seed rows                    |
| sqlx cache  | `cargo sqlx prepare --check -- --tests`    | Query/type mismatches vs live schema          |
| Format      | `cargo fmt --check`                        | N/A (no Rust changes, but run anyway)         |
| Lint        | `cargo clippy -- -D warnings`              | N/A (no Rust changes, but run anyway)         |
| Test suite  | `cargo nextest run --workspace`            | 247 DB-backed tests exercise full schema      |

### Edge Cases Checklist

- [ ] Enum values match final state (no leftover `invalid`, `draft`, `accepted`, `metadata_source` enum)
- [ ] Column defaults preserved (especially `DEFAULT 'pending'`, `DEFAULT 0`, `DEFAULT now()`, `DEFAULT true`)
- [ ] NOT NULL constraints preserved (especially `metadata_versions.new_value`, `metadata_versions.confidence_score`)
- [ ] CHECK constraints preserved (`reading_state_progress_pct_range`, `reading_state_progress_paired_with_timestamp`, `chk_child_role_sync`, `writeback_jobs.reason IN ('metadata','cover')`, `settings.singleton`)
- [ ] Partial indexes preserved (`idx_writeback_jobs_in_progress_unique`, `idx_writeback_jobs_queue`, `idx_manifestations_enrichment_queue`)
- [ ] Column renames applied (`file_hash` → `ingestion_file_hash` + new `current_file_hash`)
- [ ] `tower_sessions` in separate schema, not `public`
- [ ] `tower_sessions.session` grants are column-scoped for `reverie_readonly` (`SELECT (id, expiry_date)` only, not full `SELECT`)
- [ ] GIST indexes for trigram exist (`idx_works_title_trgm`, `idx_authors_name_trgm`, `idx_series_name_trgm`)
- [ ] GIN index for full-text search (`idx_works_search_vector`)
- [ ] Composite sort index (`idx_works_sort_title_id` on `(sort_title, id)`)
- [ ] Unique indexes: `idx_users_email_lower` (partial, WHERE email IS NOT NULL, on LOWER(email))
- [ ] `settings` table seed row present (`INSERT INTO settings DEFAULT VALUES`)
- [ ] `metadata_sources` seed rows present (6 rows: opf, manual, openlibrary, googlebooks, hardcover, ai)
- [ ] `pg_notify` function and trigger on `settings` table

---

## Validation Commands

### Level 1: SCHEMA_DIFF

```bash
diff /tmp/ref_normalized.sql /tmp/new_normalized.sql
```

**EXPECT**: Empty output (schemas identical)

### Level 2: SEED_DATA

```bash
docker exec reverie-postgres psql -U reverie -d reverie_rollup_test -c "SELECT id FROM metadata_sources ORDER BY id"
docker exec reverie-postgres psql -U reverie -d reverie_rollup_test -c "SELECT count(*) FROM settings"
```

**EXPECT**: 6 metadata_sources rows, 1 settings row

### Level 3: SQLX_CACHE

```bash
cd backend && DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo sqlx prepare --check -- --tests
```

**EXPECT**: Exit 0

### Level 4: STATIC_ANALYSIS

```bash
cd backend && cargo fmt --check && cargo clippy -- -D warnings
```

**EXPECT**: Exit 0

### Level 5: FULL_SUITE

```bash
cd backend && DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo nextest run --workspace
```

**EXPECT**: All 247+ tests pass

---

## Acceptance Criteria

- [ ] `backend/migrations/` contains exactly 2 files: `20260526000000_initial_schema.{up,down}.sql`
- [ ] Schema diff between reference (27 migrations) and consolidated (1 migration) is empty
- [ ] `cargo sqlx prepare --check -- --tests` passes
- [ ] Full test suite passes with zero failures
- [ ] No Rust source files changed (this is a migrations-only chore)
- [ ] Seed data (metadata_sources, settings) present and correct
- [ ] All grants preserved — verified by comparing `pg_dump` with privileges

---

## Completion Checklist

- [ ] Task 1: Reference schema captured
- [ ] Task 2: Consolidated up migration generated and cleaned
- [ ] Task 3: Down migration written
- [ ] Task 4: Old migration files deleted
- [ ] Task 5: Schema diff passes (acid test)
- [ ] Task 6: database-reviewer agent schema review passed
- [ ] Task 7: Dev database refreshed, sqlx cache regenerated
- [ ] Task 8: Full test suite passes
- [ ] Task 9: Temporary databases cleaned up

---

## Risks and Mitigations

| Risk                                                      | Likelihood | Impact | Mitigation                                                                                                 |
| --------------------------------------------------------- | ---------- | ------ | ---------------------------------------------------------------------------------------------------------- |
| `pg_dump` column order differs from original CREATE TABLE | LOW        | MED    | Compare column order explicitly; `pg_dump` preserves creation order in practice                            |
| Grant statements missing from consolidated migration      | MED        | HIGH   | Extract from `ref_schema_with_grants.sql` (Task 1 second dump); cross-reference against grant matrix below |
| Seed data incomplete                                      | LOW        | HIGH   | Explicit psql queries to verify row counts and content                                                     |
| `.sqlx/` cache hash changes break CI                      | LOW        | LOW    | Cache is regenerated in Task 7; CI checks freshness, not content                                           |
| Dev contributors pulling PR have stale local DB           | MED        | LOW    | PR description includes dev database reset instructions                                                    |

---

## Notes

### Dev Database Reset Instructions (for PR description)

After pulling this branch, existing dev databases must be recreated:

```bash
docker exec reverie-postgres psql -U reverie -d postgres -c "DROP DATABASE IF EXISTS reverie_dev"
docker exec reverie-postgres psql -U reverie -d postgres -c "CREATE DATABASE reverie_dev"
docker cp docker/init-roles.sql reverie-postgres:/tmp/init-roles.sql
docker exec reverie-postgres psql -U reverie -d reverie_dev -f /tmp/init-roles.sql
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev sqlx migrate run --source backend/migrations
```

### Enum Final Values (quick reference)

| Enum                     | Values                                          |
| ------------------------ | ----------------------------------------------- |
| `user_role`              | admin, adult, child                             |
| `author_role`            | author, editor, translator, narrator            |
| `manifestation_format`   | epub, pdf, mobi, azw3, cbz, cbr                 |
| `validation_status`      | pending, valid, repaired, degraded              |
| `ingestion_status`       | pending, processing, complete, failed, skipped  |
| `metadata_review_status` | pending, rejected                               |
| `tag_type`               | genre, sub_genre, trope, theme                  |
| `job_status`             | queued, running, complete, failed, skipped      |
| `enrichment_status`      | pending, in_progress, complete, failed, skipped |
| `api_cache_kind`         | hit, miss, error                                |
| `writeback_status`       | pending, in_progress, complete, failed, skipped |
| `theme_preference`       | system, light, dark                             |

### Tables + Grant Matrix (quick reference)

| Table                  | reverie_app    | reverie_ingestion | reverie_readonly        |
| ---------------------- | -------------- | ----------------- | ----------------------- |
| users                  | CRUD           | —                 | SELECT                  |
| works                  | CRUD           | CRUD              | SELECT                  |
| authors                | CRUD           | CRUD              | SELECT                  |
| work_authors           | CRUD           | CRUD              | SELECT                  |
| manifestations         | CRUD           | CRUD (own policy) | SELECT                  |
| series                 | CRUD           | CRUD              | SELECT                  |
| series_works           | CRUD           | CRUD              | SELECT                  |
| omnibus_contents       | CRUD           | CRUD              | SELECT                  |
| metadata_versions      | CRUD           | CRUD              | SELECT                  |
| metadata_sources       | CRUD           | SELECT            | SELECT                  |
| tags                   | CRUD           | CRUD              | SELECT                  |
| manifestation_tags     | CRUD           | CRUD              | SELECT                  |
| shelves                | CRUD           | —                 | SELECT                  |
| shelf_items            | CRUD           | —                 | SELECT                  |
| device_tokens          | CRUD           | —                 | —                       |
| api_cache              | CRUD           | CRUD              | SELECT                  |
| ingestion_jobs         | CRUD           | CRUD              | SELECT                  |
| webhooks               | CRUD           | —                 | SELECT                  |
| webhook_deliveries     | CRUD           | —                 | SELECT                  |
| field_locks            | CRUD           | SELECT            | SELECT                  |
| writeback_jobs         | CRUD           | SELECT, INSERT    | SELECT                  |
| reading_sessions       | CRUD           | —                 | SELECT                  |
| reading_state          | CRUD           | —                 | SELECT                  |
| settings               | SELECT, UPDATE | —                 | SELECT                  |
| tower_sessions.session | CRUD           | —                 | SELECT(id, expiry_date) |

### Review Strategy

Per memory `[[migration-rollup]]`: run `database-reviewer` agent as explicit schema review pass alongside standard review. Migration consolidation is its sweet spot — dropped constraints, index gaps, enum ordering, RLS policy correctness.
