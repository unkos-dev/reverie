# Plan: Database Schema and Migrations

## Summary

Create the full PostgreSQL schema for Tome using sqlx reversible migrations. This is the architectural foundation — a FRBR-inspired model with Works/Manifestations, relational series, metadata versioning, per-user shelving, Row Level Security, and full-text search. Implements a 4-role database architecture (`tome` owner, `tome_app` web service, `tome_ingestion` background pipeline, `tome_readonly` debugging) with per-operation RLS policies on manifestations. No Rust application code changes beyond adding sqlx to Cargo.toml; wiring the pool into Axum is Step 2.

## User Story

As a developer building Tome,
I want a complete, well-structured database schema with migrations,
so that all subsequent features (ingestion, search, OPDS, UI) have a stable data layer to build on.

## Problem -> Solution

No database layer exists -> Complete PostgreSQL schema with 20 tables, a 4-role access architecture, per-operation RLS policies, full-text search indexes, and reversible migrations managed by sqlx-cli.

## Metadata

- **Complexity**: Large
- **Source**: `plans/BLUEPRINT.md` Step 1 (lines 76-133)
- **Branch**: `feat/database-schema`
- **Estimated Files**: 19 (7 migration pairs = 14 files + Cargo.toml + .env.example + docker-compose.yml + docker/init-roles.sql + docs/schema.md)

---

## UX Design

N/A — internal infrastructure change. No user-facing UX transformation.

---

## Mandatory Reading

| Priority | File | Lines | Why |
|---|---|---|---|
| P0 | `backend/CLAUDE.md` | all | Conventions: sqlx, error handling, project structure |
| P0 | `backend/Cargo.toml` | all | Current deps — must add sqlx correctly |
| P0 | `plans/BLUEPRINT.md` | 76-133 | Step 1 spec with all table definitions |
| P1 | `plans/DESIGN_BRIEF.md` | all | FRBR model rationale, RLS design, series nesting |
| P1 | `.gitignore` | all | Confirms .env ignored, .env.example tracked |
| P2 | `.github/workflows/ci.yml` | all | CI pipeline — will need sqlx checks later |
| P2 | `Dockerfile` | all | Build context for future sqlx offline mode |

---

## Patterns to Mirror

### CARGO_TOML_STYLE
```toml
// SOURCE: backend/Cargo.toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```
Follow: version strings without `^`, features as inline arrays, alphabetical ordering.

### TRACING_PATTERN
```rust
// SOURCE: backend/src/main.rs:17
tracing::info!("listening on {}", listener.local_addr().unwrap());
```
Use `tracing` macros with structured fields. Never `println!`.

### TEST_STRUCTURE
```rust
// SOURCE: backend/src/main.rs:28-38
#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;

    #[tokio::test]
    async fn health_returns_ok() {
        let server = TestServer::new(app());
        let response = server.get("/health").await;
        response.assert_status_ok();
        response.assert_text("ok");
    }
}
```
Tests co-located in `#[cfg(test)]` modules. Integration tests use `axum-test`.

### NAMING_CONVENTION_SQL
Database identifiers use `snake_case` exclusively. No hyphens anywhere in the schema — enum values, column names, table names all use underscores. Example: `sub_genre` not `sub-genre`, `is_child` not `is-child`.

---

## Database Role Architecture

Four roles, covering all MVP and Phase 2+ features:

| Role | Purpose | Privileges | RLS |
|---|---|---|---|
| `tome` | Schema owner, runs migrations | DDL + full DML on all objects | Bypasses (owner) |
| `tome_app` | Web app, OPDS, webhooks | DML on all tables | Enforced — user-scoped visibility |
| `tome_ingestion` | Background pipeline (Steps 4-8) | DML on pipeline tables only | Own permissive policy on manifestations |
| `tome_readonly` | Debugging, reporting, future read replicas | SELECT on all tables | Enforced — same visibility as `tome_app` |

### `tome_ingestion` table access (scoped to pipeline concerns)

| Table | Access | Why |
|---|---|---|
| `works` | SELECT, INSERT, UPDATE, DELETE | Creates works from EPUB metadata |
| `authors`, `work_authors` | SELECT, INSERT, UPDATE, DELETE | Creates/links authors from OPF |
| `manifestations` | SELECT, INSERT, UPDATE, DELETE | Creates on ingest, updates validation/ingestion status |
| `series`, `series_works` | SELECT, INSERT, UPDATE, DELETE | Creates series from metadata |
| `metadata_versions` | SELECT, INSERT, UPDATE, DELETE | Writes draft metadata from each enrichment source |
| `tags`, `manifestation_tags` | SELECT, INSERT, UPDATE, DELETE | Writes genre/theme tags from metadata |
| `ingestion_jobs` | SELECT, INSERT, UPDATE, DELETE | Tracks job lifecycle |
| `api_cache` | SELECT, INSERT, UPDATE, DELETE | Caches Open Library/Google Books responses |
| `omnibus_contents` | SELECT, INSERT, UPDATE, DELETE | Maps omnibus editions |

### Tables excluded from `tome_ingestion`

| Table | Why |
|---|---|
| `users` | Pipeline doesn't manage users |
| `shelves`, `shelf_items` | User-facing feature only |
| `device_tokens` | Auth concern only |
| `webhooks`, `webhook_deliveries` | Event system, not ingestion |
| `reading_sessions`, `reading_positions` | Reserved, user-scoped |

### Phase 2+ role coverage

Every deferred feature maps to an existing role:

| Feature | Role | Reason |
|---|---|---|
| Reading sync adapters (Kobo, KOReader, Moon+) | `tome_app` | User-scoped device sync |
| Web reader, Companion reader app | `tome_app` | Authenticated user reads |
| OPDS 2.0 | `tome_app` | Same as OPDS 1.2 |
| LLM content analysis | `tome_ingestion` | Background job, same pattern as enrichment |
| Semantic search (pgvector) | `tome_ingestion` writes embeddings, `tome_app` queries | Split: pipeline writes, user reads |
| NER-based inversion detection | `tome_ingestion` | Background author analysis |
| Import adapters (Calibre, Grimmory) | `tome_ingestion` | Same shape as file ingestion |
| PKM integration | `tome_app` | User-initiated export |
| Knowledge graphs | `tome_ingestion` | Background graph building |

---

## Files to Change

| File | Action | Justification |
|---|---|---|
| `backend/Cargo.toml` | UPDATE | Add sqlx, serde, serde_json dependencies |
| `.env.example` | CREATE | DATABASE_URL templates for all roles |
| `docker-compose.yml` | CREATE | PostgreSQL service for local dev |
| `docker/init-roles.sql` | CREATE | Provisions all 4 database roles at container init |
| `backend/migrations/*_extensions_enums_and_roles.{up,down}.sql` | CREATE | Extensions, enum types, schema grants |
| `backend/migrations/*_core_tables.{up,down}.sql` | CREATE | users, works, authors, work_authors, manifestations + grants |
| `backend/migrations/*_series_and_metadata.{up,down}.sql` | CREATE | series, series_works, omnibus_contents, metadata_versions, tags, manifestation_tags + grants |
| `backend/migrations/*_user_features.{up,down}.sql` | CREATE | shelves, shelf_items, device_tokens + grants |
| `backend/migrations/*_system_tables.{up,down}.sql` | CREATE | api_cache, ingestion_jobs, webhooks, webhook_deliveries + grants |
| `backend/migrations/*_triggers_and_functions.{up,down}.sql` | CREATE | set_updated_at trigger, search_vector trigger |
| `backend/migrations/*_search_rls_and_reserved.{up,down}.sql` | CREATE | Indexes, per-operation RLS policies, reserved tables + grants |
| `docs/schema.md` | CREATE | Schema documentation (exit criterion) |

## NOT Building

- No Rust model structs or query macros (Step 2)
- No PgPool wiring into main.rs or Axum state (Step 2)
- No application error types (Step 2)
- No API endpoints or routes beyond existing health check
- No pgvector extension install or actual vector column (blueprint says "commented out, migration-ready" — add as SQL comment only)
- No seed data or fixtures

---

## Step-by-Step Tasks

### Task 1: Add sqlx dependencies to Cargo.toml

- **ACTION**: Update `backend/Cargo.toml` to add sqlx and supporting crates
- **IMPLEMENT**: Add to `[dependencies]`:
  ```toml
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "time", "json", "macros", "migrate"] }
  time = { version = "0.3", features = ["serde"] }
  uuid = { version = "1", features = ["v4", "serde"] }
  ```
- **MIRROR**: CARGO_TOML_STYLE — match existing formatting, alphabetical order
- **GOTCHA**: Must include `migrate` feature for `sqlx::migrate!()` macro. Must include `json` feature for JSONB columns. The `macros` feature enables `query!` and `query_as!`. Note: `serde`/`serde_json` aren't needed for migration-only work but are added now to avoid a second Cargo.toml edit in Step 2.
- **VALIDATE**: `cargo check` succeeds in backend/

### Task 2: Create .env.example

- **ACTION**: Create `.env.example` in repo root
- **IMPLEMENT**:
  ```
  # --- Database ---

  # Web application runtime (RLS enforced, user-scoped read filtering)
  DATABASE_URL=postgres://tome_app:tome_app@localhost:5432/tome_dev

  # Background ingestion pipeline (permissive RLS, scoped table access)
  # Used by the ingestion/enrichment workers (Steps 4-8).
  # DATABASE_URL_INGESTION=postgres://tome_ingestion:tome_ingestion@localhost:5432/tome_dev

  # Migrations — schema owner, bypasses RLS. DO NOT use for application runtime.
  # Run migrations explicitly: cd backend && DATABASE_URL=postgres://tome:tome@localhost:5432/tome_dev sqlx migrate run
  # DATABASE_URL_MIGRATIONS=postgres://tome:tome@localhost:5432/tome_dev
  ```
- **GOTCHA**: `.gitignore` already has `!.env.example` exception so this will be tracked. The primary `DATABASE_URL` is `tome_app` — this is what the Axum server reads. Migration and ingestion URLs are commented-out references. A developer who copies this to `.env` gets the safe default.
- **VALIDATE**: File exists and is not gitignored (`git check-ignore .env.example` returns nothing)

### Task 3: Create docker-compose.yml and role init script

- **ACTION**: Create `docker-compose.yml` in repo root and `docker/init-roles.sql`
- **IMPLEMENT** (`docker-compose.yml`):
  ```yaml
  services:
    db:
      image: postgres:17
      environment:
        POSTGRES_USER: tome
        POSTGRES_PASSWORD: tome
        POSTGRES_DB: tome_dev
      ports:
        - "5432:5432"
      volumes:
        - pgdata:/var/lib/postgresql/data
        - ./docker/init-roles.sql:/docker-entrypoint-initdb.d/01-init-roles.sql:ro
      healthcheck:
        test: ["CMD-SHELL", "pg_isready -U tome -d tome_dev"]
        interval: 5s
        timeout: 5s
        retries: 5

  volumes:
    pgdata:
  ```

  **IMPLEMENT** (`docker/init-roles.sql`):
  ```sql
  -- Database role provisioning for Tome.
  -- This script runs once when the PostgreSQL container is first created
  -- (empty pgdata volume). It creates the application roles that sqlx
  -- migrations will grant privileges to.
  --
  -- Role architecture:
  --   tome           — schema owner (created by POSTGRES_USER). Runs migrations.
  --                    Bypasses RLS. Never used by the application at runtime.
  --   tome_app       — web application service account. RLS enforced (user-scoped).
  --   tome_ingestion — background pipeline service account. Has own permissive
  --                    RLS policy on manifestations. Scoped to pipeline tables.
  --   tome_readonly  — debugging and reporting. SELECT only. RLS enforced.

  -- Web application service account
  CREATE ROLE tome_app WITH LOGIN PASSWORD 'tome_app';
  GRANT CONNECT ON DATABASE tome_dev TO tome_app;

  -- Background ingestion pipeline service account
  CREATE ROLE tome_ingestion WITH LOGIN PASSWORD 'tome_ingestion';
  GRANT CONNECT ON DATABASE tome_dev TO tome_ingestion;

  -- Read-only account for debugging and reporting
  CREATE ROLE tome_readonly WITH LOGIN PASSWORD 'tome_readonly';
  GRANT CONNECT ON DATABASE tome_dev TO tome_readonly;
  ```
- **GOTCHA**: `docker-compose.override.yml` is gitignored — users can override locally. The init script runs only on first container creation (empty `pgdata` volume). If the volume already exists, you must `docker compose down -v && docker compose up -d` to re-initialize. The `tome` owner role is created automatically by `POSTGRES_USER`.
- **VALIDATE**: `docker compose config` validates. After `docker compose up -d`, all four roles can connect:
  ```bash
  PGPASSWORD=tome psql -h localhost -U tome -d tome_dev -c "SELECT 1"
  PGPASSWORD=tome_app psql -h localhost -U tome_app -d tome_dev -c "SELECT 1"
  PGPASSWORD=tome_ingestion psql -h localhost -U tome_ingestion -d tome_dev -c "SELECT 1"
  PGPASSWORD=tome_readonly psql -h localhost -U tome_readonly -d tome_dev -c "SELECT 1"
  ```

### Task 4: Migration 1 — Extensions, Enums, and Schema Grants

- **ACTION**: Create first reversible migration with `sqlx migrate add -r extensions_enums_and_roles` from `backend/`
- **IMPLEMENT** (up.sql):
  ```sql
  -- Extensions
  CREATE EXTENSION IF NOT EXISTS "pg_trgm";

  -- Enum types
  CREATE TYPE user_role AS ENUM ('admin', 'adult', 'child');
  CREATE TYPE author_role AS ENUM ('author', 'editor', 'translator', 'narrator');
  CREATE TYPE manifestation_format AS ENUM ('epub', 'pdf', 'mobi', 'azw3', 'cbz', 'cbr');
  CREATE TYPE validation_status AS ENUM ('pending', 'valid', 'invalid', 'repaired');
  CREATE TYPE ingestion_status AS ENUM ('pending', 'processing', 'complete', 'failed', 'skipped');
  CREATE TYPE metadata_source AS ENUM ('opf', 'openlibrary', 'googlebooks', 'manual', 'ai');
  CREATE TYPE metadata_review_status AS ENUM ('draft', 'accepted', 'rejected');
  CREATE TYPE tag_type AS ENUM ('genre', 'sub_genre', 'trope', 'theme');

  -- Grant schema usage to all application roles.
  -- Table-level grants are issued in subsequent migrations after tables exist.
  GRANT USAGE ON SCHEMA public TO tome_app;
  GRANT USAGE ON SCHEMA public TO tome_ingestion;
  GRANT USAGE ON SCHEMA public TO tome_readonly;

  -- NOTE on enum design:
  -- manifestations.ingestion_status tracks the file's lifecycle within the ingestion
  -- pipeline (pending -> processing -> complete/failed/skipped). It lives on the
  -- manifestation row itself.
  --
  -- ingestion_jobs.status (using job_status, defined in the system_tables migration)
  -- tracks the batch job orchestration layer — a single job may process many files.
  -- These are intentionally separate concerns: a job can fail while individual files
  -- within it succeeded, and vice versa.
  ```
- **IMPLEMENT** (down.sql):
  ```sql
  REVOKE USAGE ON SCHEMA public FROM tome_readonly;
  REVOKE USAGE ON SCHEMA public FROM tome_ingestion;
  REVOKE USAGE ON SCHEMA public FROM tome_app;

  DROP TYPE IF EXISTS tag_type;
  DROP TYPE IF EXISTS metadata_review_status;
  DROP TYPE IF EXISTS metadata_source;
  DROP TYPE IF EXISTS ingestion_status;
  DROP TYPE IF EXISTS validation_status;
  DROP TYPE IF EXISTS manifestation_format;
  DROP TYPE IF EXISTS author_role;
  DROP TYPE IF EXISTS user_role;

  DROP EXTENSION IF EXISTS "pg_trgm";
  ```
- **GOTCHA**: `gen_random_uuid()` is built-in since PostgreSQL 13 — no `uuid-ossp` extension needed. The roles themselves are created by `docker/init-roles.sql` at container init time, not by migrations, because `CREATE ROLE` is a cluster-level operation and migrations run against a specific database. Default PostgreSQL grants give `PUBLIC` usage on types, which covers all roles. If the database is hardened with `REVOKE ALL FROM PUBLIC`, explicit type grants would be needed — document this assumption.
- **VALIDATE**: `sqlx migrate run` succeeds (run as `tome` owner), `\dT+` shows all enum types, `\dn+` shows all three app roles have USAGE on public schema

### Task 5: Migration 2 — Core Tables

- **ACTION**: Create `sqlx migrate add -r core_tables`
- **IMPLEMENT** (up.sql):
  ```sql
  -- Users
  CREATE TABLE users (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      oidc_subject TEXT UNIQUE NOT NULL,
      display_name TEXT NOT NULL,
      email TEXT,
      role user_role NOT NULL DEFAULT 'adult',
      is_child BOOLEAN NOT NULL DEFAULT FALSE,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      -- is_child is an admin-set flag (UI checkbox) that triggers child-related
      -- policies (RLS content filtering). It is intentionally separate from role:
      -- role controls permissions, is_child controls content visibility.
      -- They must stay in sync — enforced by this constraint.
      CONSTRAINT chk_child_role_sync CHECK (
          (is_child = TRUE AND role = 'child') OR (is_child = FALSE AND role != 'child')
      )
  );

  -- Works (abstract titles — FRBR model)
  CREATE TABLE works (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      title TEXT NOT NULL,
      sort_title TEXT NOT NULL,
      description TEXT,
      language TEXT,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      search_vector TSVECTOR
  );

  -- Authors
  CREATE TABLE authors (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      name TEXT NOT NULL,
      sort_name TEXT NOT NULL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );

  -- Work-Author join
  CREATE TABLE work_authors (
      work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
      author_id UUID NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
      role author_role NOT NULL DEFAULT 'author',
      position INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY (work_id, author_id, role)
  );

  -- Manifestations (concrete files — FRBR model)
  CREATE TABLE manifestations (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
      isbn_10 TEXT,
      isbn_13 TEXT,
      publisher TEXT,
      pub_date DATE,
      format manifestation_format NOT NULL,
      file_path TEXT NOT NULL UNIQUE,
      file_hash TEXT NOT NULL,
      file_size_bytes BIGINT NOT NULL,
      validation_status validation_status NOT NULL DEFAULT 'pending',
      ingestion_status ingestion_status NOT NULL DEFAULT 'pending',
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );

  -- Grants: tome_app (full DML, all core tables)
  GRANT SELECT, INSERT, UPDATE, DELETE ON users TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON works TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON authors TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON work_authors TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON manifestations TO tome_app;

  -- Grants: tome_ingestion (pipeline tables only — no users access)
  GRANT SELECT, INSERT, UPDATE, DELETE ON works TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON authors TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON work_authors TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON manifestations TO tome_ingestion;

  -- Grants: tome_readonly (SELECT on all)
  GRANT SELECT ON users TO tome_readonly;
  GRANT SELECT ON works TO tome_readonly;
  GRANT SELECT ON authors TO tome_readonly;
  GRANT SELECT ON work_authors TO tome_readonly;
  GRANT SELECT ON manifestations TO tome_readonly;
  ```
- **IMPLEMENT** (down.sql):
  ```sql
  REVOKE ALL ON manifestations FROM tome_readonly;
  REVOKE ALL ON work_authors FROM tome_readonly;
  REVOKE ALL ON authors FROM tome_readonly;
  REVOKE ALL ON works FROM tome_readonly;
  REVOKE ALL ON users FROM tome_readonly;

  REVOKE ALL ON manifestations FROM tome_ingestion;
  REVOKE ALL ON work_authors FROM tome_ingestion;
  REVOKE ALL ON authors FROM tome_ingestion;
  REVOKE ALL ON works FROM tome_ingestion;

  REVOKE ALL ON manifestations FROM tome_app;
  REVOKE ALL ON work_authors FROM tome_app;
  REVOKE ALL ON authors FROM tome_app;
  REVOKE ALL ON works FROM tome_app;
  REVOKE ALL ON users FROM tome_app;

  DROP TABLE IF EXISTS manifestations;
  DROP TABLE IF EXISTS work_authors;
  DROP TABLE IF EXISTS authors;
  DROP TABLE IF EXISTS works;
  DROP TABLE IF EXISTS users;
  ```
- **GOTCHA**: `sort_title` on works strips leading articles for display ordering. `file_path` UNIQUE prevents duplicate ingestion. `ON DELETE CASCADE` on work_id FKs means deleting a work removes its manifestations. The CHECK constraint on `is_child`/`role` enforces that the admin UI checkbox and the role enum stay in sync. `tome_ingestion` has no access to `users` — the pipeline doesn't manage user accounts.
- **VALIDATE**: `\dt` shows all 5 tables. `INSERT INTO users (oidc_subject, display_name, role, is_child) VALUES ('test', 'Test', 'adult', true)` fails the CHECK constraint. Connect as `tome_ingestion` and verify `SELECT * FROM users` is denied.

### Task 6: Migration 3 — Series and Metadata

- **ACTION**: Create `sqlx migrate add -r series_and_metadata`
- **IMPLEMENT** (up.sql):
  ```sql
  -- Series with self-referential nesting
  CREATE TABLE series (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      name TEXT NOT NULL,
      sort_name TEXT NOT NULL,
      parent_id UUID REFERENCES series(id) ON DELETE SET NULL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );

  -- Series-Works join
  CREATE TABLE series_works (
      series_id UUID NOT NULL REFERENCES series(id) ON DELETE CASCADE,
      work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
      position NUMERIC,
      is_omnibus BOOLEAN NOT NULL DEFAULT FALSE,
      note TEXT,
      PRIMARY KEY (series_id, work_id)
  );

  -- Omnibus contents mapping
  CREATE TABLE omnibus_contents (
      omnibus_manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
      contained_work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
      position INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY (omnibus_manifestation_id, contained_work_id)
  );

  -- Metadata versioning
  CREATE TABLE metadata_versions (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
      source metadata_source NOT NULL,
      field_name TEXT NOT NULL,
      old_value JSONB,
      new_value JSONB,
      status metadata_review_status NOT NULL DEFAULT 'draft',
      confidence_score REAL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      resolved_at TIMESTAMPTZ,
      resolved_by UUID REFERENCES users(id) ON DELETE SET NULL
  );

  -- Tags
  CREATE TABLE tags (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      name TEXT NOT NULL,
      tag_type tag_type NOT NULL,
      UNIQUE (name, tag_type)
  );

  -- Manifestation-Tags join
  CREATE TABLE manifestation_tags (
      manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
      tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
      PRIMARY KEY (manifestation_id, tag_id)
  );

  -- Grants: tome_app (full DML)
  GRANT SELECT, INSERT, UPDATE, DELETE ON series TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON series_works TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON omnibus_contents TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON metadata_versions TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON tags TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON manifestation_tags TO tome_app;

  -- Grants: tome_ingestion (pipeline tables)
  GRANT SELECT, INSERT, UPDATE, DELETE ON series TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON series_works TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON omnibus_contents TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON metadata_versions TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON tags TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON manifestation_tags TO tome_ingestion;

  -- Grants: tome_readonly (SELECT)
  GRANT SELECT ON series TO tome_readonly;
  GRANT SELECT ON series_works TO tome_readonly;
  GRANT SELECT ON omnibus_contents TO tome_readonly;
  GRANT SELECT ON metadata_versions TO tome_readonly;
  GRANT SELECT ON tags TO tome_readonly;
  GRANT SELECT ON manifestation_tags TO tome_readonly;
  ```
- **IMPLEMENT** (down.sql):
  ```sql
  REVOKE ALL ON manifestation_tags FROM tome_readonly;
  REVOKE ALL ON tags FROM tome_readonly;
  REVOKE ALL ON metadata_versions FROM tome_readonly;
  REVOKE ALL ON omnibus_contents FROM tome_readonly;
  REVOKE ALL ON series_works FROM tome_readonly;
  REVOKE ALL ON series FROM tome_readonly;

  REVOKE ALL ON manifestation_tags FROM tome_ingestion;
  REVOKE ALL ON tags FROM tome_ingestion;
  REVOKE ALL ON metadata_versions FROM tome_ingestion;
  REVOKE ALL ON omnibus_contents FROM tome_ingestion;
  REVOKE ALL ON series_works FROM tome_ingestion;
  REVOKE ALL ON series FROM tome_ingestion;

  REVOKE ALL ON manifestation_tags FROM tome_app;
  REVOKE ALL ON tags FROM tome_app;
  REVOKE ALL ON metadata_versions FROM tome_app;
  REVOKE ALL ON omnibus_contents FROM tome_app;
  REVOKE ALL ON series_works FROM tome_app;
  REVOKE ALL ON series FROM tome_app;

  DROP TABLE IF EXISTS manifestation_tags;
  DROP TABLE IF EXISTS tags;
  DROP TABLE IF EXISTS metadata_versions;
  DROP TABLE IF EXISTS omnibus_contents;
  DROP TABLE IF EXISTS series_works;
  DROP TABLE IF EXISTS series;
  ```
- **GOTCHA**: `position NUMERIC` in series_works allows fractional ordering (e.g., 1.5 for novellas). Self-referential `parent_id` uses `ON DELETE SET NULL` to orphan children rather than cascade-delete entire series trees.
- **VALIDATE**: `\dt` shows 6 new tables, self-referential FK on series visible via `\d series`

### Task 7: Migration 4 — User Features

- **ACTION**: Create `sqlx migrate add -r user_features`
- **IMPLEMENT** (up.sql):
  ```sql
  -- Shelves (per-user collections)
  CREATE TABLE shelves (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      name TEXT NOT NULL,
      is_system BOOLEAN NOT NULL DEFAULT FALSE,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );

  -- Shelf items
  CREATE TABLE shelf_items (
      shelf_id UUID NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
      manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
      added_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      position INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY (shelf_id, manifestation_id)
  );

  -- Device tokens (OPDS/reader auth)
  CREATE TABLE device_tokens (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      name TEXT NOT NULL,
      token_hash TEXT NOT NULL,
      last_used_at TIMESTAMPTZ,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      revoked_at TIMESTAMPTZ
  );

  -- Grants: tome_app only (user-facing tables — no ingestion access)
  GRANT SELECT, INSERT, UPDATE, DELETE ON shelves TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON shelf_items TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON device_tokens TO tome_app;

  -- Grants: tome_readonly (SELECT)
  GRANT SELECT ON shelves TO tome_readonly;
  GRANT SELECT ON shelf_items TO tome_readonly;
  GRANT SELECT ON device_tokens TO tome_readonly;
  ```
- **IMPLEMENT** (down.sql):
  ```sql
  REVOKE ALL ON device_tokens FROM tome_readonly;
  REVOKE ALL ON shelf_items FROM tome_readonly;
  REVOKE ALL ON shelves FROM tome_readonly;

  REVOKE ALL ON device_tokens FROM tome_app;
  REVOKE ALL ON shelf_items FROM tome_app;
  REVOKE ALL ON shelves FROM tome_app;

  DROP TABLE IF EXISTS device_tokens;
  DROP TABLE IF EXISTS shelf_items;
  DROP TABLE IF EXISTS shelves;
  ```
- **GOTCHA**: `is_system` shelves (Reading, Want to Read, Read) are created per-user by application logic, not by migration. `token_hash` stores argon2 hash — never store raw tokens. `tome_ingestion` has no access to these tables — they are user-facing only.
- **VALIDATE**: `\dt` shows 3 new tables. Connect as `tome_ingestion` and verify `SELECT * FROM shelves` is denied.

### Task 8: Migration 5 — System Tables

- **ACTION**: Create `sqlx migrate add -r system_tables`
- **IMPLEMENT** (up.sql):
  ```sql
  -- Job status enum (separate from ingestion_status — see note in migration 1).
  -- ingestion_status tracks per-file lifecycle on manifestations.
  -- job_status tracks batch orchestration on ingestion_jobs.
  -- A job can fail while individual files within it succeeded, and vice versa.
  CREATE TYPE job_status AS ENUM ('queued', 'running', 'complete', 'failed');

  -- API response cache
  CREATE TABLE api_cache (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      source TEXT NOT NULL,
      lookup_key TEXT NOT NULL,
      response JSONB NOT NULL,
      fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      expires_at TIMESTAMPTZ NOT NULL,
      UNIQUE (source, lookup_key)
  );

  -- Ingestion job tracking
  CREATE TABLE ingestion_jobs (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      batch_id UUID NOT NULL,
      source_path TEXT NOT NULL,
      status job_status NOT NULL DEFAULT 'queued',
      error_message TEXT,
      started_at TIMESTAMPTZ,
      completed_at TIMESTAMPTZ,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );

  -- Webhooks
  CREATE TABLE webhooks (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      url TEXT NOT NULL,
      events JSONB NOT NULL DEFAULT '[]',
      payload_template TEXT,
      enabled BOOLEAN NOT NULL DEFAULT TRUE,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );

  -- Webhook delivery log
  CREATE TABLE webhook_deliveries (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
      event_type TEXT NOT NULL,
      payload JSONB NOT NULL,
      response_status INTEGER,
      delivered_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );

  -- Grants: tome_app (full DML on all system tables)
  GRANT SELECT, INSERT, UPDATE, DELETE ON api_cache TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON ingestion_jobs TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON webhooks TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON webhook_deliveries TO tome_app;

  -- Grants: tome_ingestion (pipeline tables only — no webhook access)
  GRANT SELECT, INSERT, UPDATE, DELETE ON api_cache TO tome_ingestion;
  GRANT SELECT, INSERT, UPDATE, DELETE ON ingestion_jobs TO tome_ingestion;

  -- Grants: tome_readonly (SELECT on all)
  GRANT SELECT ON api_cache TO tome_readonly;
  GRANT SELECT ON ingestion_jobs TO tome_readonly;
  GRANT SELECT ON webhooks TO tome_readonly;
  GRANT SELECT ON webhook_deliveries TO tome_readonly;
  ```
- **IMPLEMENT** (down.sql):
  ```sql
  REVOKE ALL ON webhook_deliveries FROM tome_readonly;
  REVOKE ALL ON webhooks FROM tome_readonly;
  REVOKE ALL ON ingestion_jobs FROM tome_readonly;
  REVOKE ALL ON api_cache FROM tome_readonly;

  REVOKE ALL ON ingestion_jobs FROM tome_ingestion;
  REVOKE ALL ON api_cache FROM tome_ingestion;

  REVOKE ALL ON webhook_deliveries FROM tome_app;
  REVOKE ALL ON webhooks FROM tome_app;
  REVOKE ALL ON ingestion_jobs FROM tome_app;
  REVOKE ALL ON api_cache FROM tome_app;

  DROP TABLE IF EXISTS webhook_deliveries;
  DROP TABLE IF EXISTS webhooks;
  DROP TABLE IF EXISTS ingestion_jobs;
  DROP TABLE IF EXISTS api_cache;

  DROP TYPE IF EXISTS job_status;
  ```
- **GOTCHA**: `api_cache` UNIQUE on (source, lookup_key) enables upsert pattern. `events` is a JSONB array of event type strings. `job_status` enum is defined here (not in migration 1) because it's scoped to system tables. `tome_ingestion` gets access to `api_cache` and `ingestion_jobs` but NOT `webhooks`/`webhook_deliveries` — webhooks are an event system concern, not pipeline.
- **VALIDATE**: `\dt` shows 4 new tables, `\dT+ job_status` shows the enum. Connect as `tome_ingestion` and verify `SELECT * FROM webhooks` is denied.

### Task 9: Migration 6 — Triggers and Functions

- **ACTION**: Create `sqlx migrate add -r triggers_and_functions`
- **IMPLEMENT** (up.sql):
  ```sql
  -- Generic updated_at trigger function.
  -- Apply to any table with an updated_at column.
  CREATE OR REPLACE FUNCTION set_updated_at() RETURNS TRIGGER AS $$
  BEGIN
      NEW.updated_at := now();
      RETURN NEW;
  END;
  $$ LANGUAGE plpgsql;

  CREATE TRIGGER trg_users_updated_at
      BEFORE UPDATE ON users
      FOR EACH ROW
      EXECUTE FUNCTION set_updated_at();

  CREATE TRIGGER trg_works_updated_at
      BEFORE UPDATE ON works
      FOR EACH ROW
      EXECUTE FUNCTION set_updated_at();

  CREATE TRIGGER trg_manifestations_updated_at
      BEFORE UPDATE ON manifestations
      FOR EACH ROW
      EXECUTE FUNCTION set_updated_at();

  -- NOTE: reading_positions also has an updated_at column but is a reserved
  -- table with no logic yet. Add the trigger when the table gets active use
  -- (Phase 2: reading sync adapters).

  -- Auto-update search_vector on works
  CREATE OR REPLACE FUNCTION works_search_vector_update() RETURNS TRIGGER AS $$
  BEGIN
      NEW.search_vector := to_tsvector('english', COALESCE(NEW.title, '') || ' ' || COALESCE(NEW.description, ''));
      RETURN NEW;
  END;
  $$ LANGUAGE plpgsql;

  CREATE TRIGGER trg_works_search_vector
      BEFORE INSERT OR UPDATE OF title, description ON works
      FOR EACH ROW
      EXECUTE FUNCTION works_search_vector_update();
  ```
- **IMPLEMENT** (down.sql):
  ```sql
  DROP TRIGGER IF EXISTS trg_works_search_vector ON works;
  DROP FUNCTION IF EXISTS works_search_vector_update();

  DROP TRIGGER IF EXISTS trg_manifestations_updated_at ON manifestations;
  DROP TRIGGER IF EXISTS trg_works_updated_at ON works;
  DROP TRIGGER IF EXISTS trg_users_updated_at ON users;
  DROP FUNCTION IF EXISTS set_updated_at();
  ```
- **GOTCHA**: `set_updated_at()` is generic and reusable — apply it to any future table with `updated_at`. The search_vector trigger fires on INSERT (to populate on creation) and UPDATE OF title, description (to re-index on change). The `updated_at` triggers fire only on UPDATE (the DEFAULT handles INSERT). `reading_positions.updated_at` is deliberately left without a trigger — the table is reserved with no active logic.
- **VALIDATE**: Insert a work, then update its title — verify both `search_vector` and `updated_at` change. Update a user — verify `updated_at` changes.

### Task 10: Migration 7 — Search Indexes, RLS, and Reserved Tables

- **ACTION**: Create `sqlx migrate add -r search_rls_and_reserved`
- **IMPLEMENT** (up.sql):
  ```sql
  -- Full-text search: GIN index on works.search_vector
  CREATE INDEX idx_works_search_vector ON works USING GIN (search_vector);

  -- Trigram indexes for fuzzy matching
  CREATE INDEX idx_works_title_trgm ON works USING GIST (title gist_trgm_ops);
  CREATE INDEX idx_authors_name_trgm ON authors USING GIST (name gist_trgm_ops);
  CREATE INDEX idx_series_name_trgm ON series USING GIST (name gist_trgm_ops);

  -- Additional useful indexes
  CREATE INDEX idx_manifestations_work_id ON manifestations (work_id);
  CREATE INDEX idx_manifestations_isbn_13 ON manifestations (isbn_13) WHERE isbn_13 IS NOT NULL;
  CREATE INDEX idx_metadata_versions_manifestation_id ON metadata_versions (manifestation_id);
  CREATE INDEX idx_shelf_items_manifestation_id ON shelf_items (manifestation_id);
  CREATE INDEX idx_ingestion_jobs_batch_id ON ingestion_jobs (batch_id);
  CREATE INDEX idx_ingestion_jobs_status ON ingestion_jobs (status);
  CREATE INDEX idx_api_cache_expires_at ON api_cache (expires_at);

  -- FK indexes for user-scoped lookups
  CREATE INDEX idx_shelves_user_id ON shelves (user_id);
  CREATE INDEX idx_device_tokens_user_id ON device_tokens (user_id);
  CREATE INDEX idx_webhooks_user_id ON webhooks (user_id);
  CREATE INDEX idx_webhook_deliveries_webhook_id ON webhook_deliveries (webhook_id);

  -- FK indexes for relational lookups
  CREATE INDEX idx_series_parent_id ON series (parent_id) WHERE parent_id IS NOT NULL;
  CREATE INDEX idx_work_authors_author_id ON work_authors (author_id);

  ---------------------------------------------------------------------------
  -- Row Level Security on manifestations
  ---------------------------------------------------------------------------
  -- RLS contract:
  --   tome       (owner)     — bypasses RLS. Used for migrations and admin only.
  --   tome_app   (web app)   — enforced. Must SET LOCAL app.current_user_id in a
  --                            transaction. SET LOCAL is transaction-scoped and
  --                            auto-resets on commit/rollback — safe with pools.
  --                            Use: SELECT set_config('app.current_user_id', $1::text, true)
  --   tome_ingestion (pipeline) — has own permissive policy. No session var needed.
  --   tome_readonly            — enforced, same visibility rules as tome_app.
  ---------------------------------------------------------------------------

  ALTER TABLE manifestations ENABLE ROW LEVEL SECURITY;

  COMMENT ON TABLE manifestations IS
      'RLS enabled. tome_app and tome_readonly must SET LOCAL app.current_user_id in a transaction. tome_ingestion has unconditional access. tome (owner) bypasses RLS.';

  -- SELECT: adults/admins see all, children see shelf-assigned only
  CREATE POLICY manifestations_select_adult ON manifestations
      FOR SELECT
      TO tome_app, tome_readonly
      USING (
          EXISTS (
              SELECT 1 FROM users
              WHERE id = current_setting('app.current_user_id', true)::uuid
              AND role IN ('admin', 'adult')
          )
      );

  CREATE POLICY manifestations_select_child ON manifestations
      FOR SELECT
      TO tome_app, tome_readonly
      USING (
          EXISTS (
              SELECT 1 FROM users
              WHERE id = current_setting('app.current_user_id', true)::uuid
              AND role = 'child'
          )
          AND EXISTS (
              SELECT 1 FROM shelf_items si
              JOIN shelves s ON s.id = si.shelf_id
              WHERE si.manifestation_id = manifestations.id
              AND s.user_id = current_setting('app.current_user_id', true)::uuid
          )
      );

  -- INSERT: app-level authorization controls who can trigger creation.
  -- The ingestion pipeline creates manifestations without user context.
  -- RLS only validates that the row is well-formed (always true here).
  CREATE POLICY manifestations_insert ON manifestations
      FOR INSERT
      TO tome_app
      WITH CHECK (true);

  -- UPDATE: can only update rows you can see (USING = SELECT visibility).
  -- The post-update row is unrestricted (WITH CHECK true) — app logic validates.
  CREATE POLICY manifestations_update_adult ON manifestations
      FOR UPDATE
      TO tome_app
      USING (
          EXISTS (
              SELECT 1 FROM users
              WHERE id = current_setting('app.current_user_id', true)::uuid
              AND role IN ('admin', 'adult')
          )
      )
      WITH CHECK (true);

  CREATE POLICY manifestations_update_child ON manifestations
      FOR UPDATE
      TO tome_app
      USING (
          EXISTS (
              SELECT 1 FROM users
              WHERE id = current_setting('app.current_user_id', true)::uuid
              AND role = 'child'
          )
          AND EXISTS (
              SELECT 1 FROM shelf_items si
              JOIN shelves s ON s.id = si.shelf_id
              WHERE si.manifestation_id = manifestations.id
              AND s.user_id = current_setting('app.current_user_id', true)::uuid
          )
      )
      WITH CHECK (true);

  -- DELETE: can only delete rows you can see. Same USING as SELECT.
  CREATE POLICY manifestations_delete_adult ON manifestations
      FOR DELETE
      TO tome_app
      USING (
          EXISTS (
              SELECT 1 FROM users
              WHERE id = current_setting('app.current_user_id', true)::uuid
              AND role IN ('admin', 'adult')
          )
      );

  CREATE POLICY manifestations_delete_child ON manifestations
      FOR DELETE
      TO tome_app
      USING (
          EXISTS (
              SELECT 1 FROM users
              WHERE id = current_setting('app.current_user_id', true)::uuid
              AND role = 'child'
          )
          AND EXISTS (
              SELECT 1 FROM shelf_items si
              JOIN shelves s ON s.id = si.shelf_id
              WHERE si.manifestation_id = manifestations.id
              AND s.user_id = current_setting('app.current_user_id', true)::uuid
          )
      );

  -- Ingestion pipeline: unconditional access to all operations.
  -- The pipeline operates without user context — it must see and modify all rows.
  CREATE POLICY manifestations_ingestion_full_access ON manifestations
      FOR ALL
      TO tome_ingestion
      USING (true)
      WITH CHECK (true);

  ---------------------------------------------------------------------------
  -- Reserved tables for future features (empty structure, no logic)
  ---------------------------------------------------------------------------

  CREATE TABLE reading_sessions (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
      started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      ended_at TIMESTAMPTZ,
      duration_seconds INTEGER
  );

  CREATE TABLE reading_positions (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
      position_cfi TEXT,
      percentage REAL,
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      -- NOTE: no updated_at trigger yet. Add when this table gets active use
      -- (Phase 2: reading sync adapters). See migration 6 for the reusable
      -- set_updated_at() function.
      UNIQUE (user_id, manifestation_id)
  );

  -- Grants: tome_app only (reserved tables are user-scoped)
  GRANT SELECT, INSERT, UPDATE, DELETE ON reading_sessions TO tome_app;
  GRANT SELECT, INSERT, UPDATE, DELETE ON reading_positions TO tome_app;

  -- Grants: tome_readonly
  GRANT SELECT ON reading_sessions TO tome_readonly;
  GRANT SELECT ON reading_positions TO tome_readonly;

  -- pgvector reserved for future semantic search (Phase 2):
  -- When ready, run in a new migration:
  --   CREATE EXTENSION IF NOT EXISTS vector;
  --   ALTER TABLE works ADD COLUMN embedding vector(1536);
  --   CREATE INDEX idx_works_embedding ON works USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);
  --   GRANT SELECT ON works TO tome_readonly;  -- already granted, but embedding queries need it
  --   -- tome_ingestion writes embeddings; tome_app queries them via SELECT (already granted).
  ```
- **IMPLEMENT** (down.sql):
  ```sql
  REVOKE ALL ON reading_positions FROM tome_readonly;
  REVOKE ALL ON reading_sessions FROM tome_readonly;
  REVOKE ALL ON reading_positions FROM tome_app;
  REVOKE ALL ON reading_sessions FROM tome_app;

  DROP TABLE IF EXISTS reading_positions;
  DROP TABLE IF EXISTS reading_sessions;

  DROP POLICY IF EXISTS manifestations_ingestion_full_access ON manifestations;
  DROP POLICY IF EXISTS manifestations_delete_child ON manifestations;
  DROP POLICY IF EXISTS manifestations_delete_adult ON manifestations;
  DROP POLICY IF EXISTS manifestations_update_child ON manifestations;
  DROP POLICY IF EXISTS manifestations_update_adult ON manifestations;
  DROP POLICY IF EXISTS manifestations_insert ON manifestations;
  DROP POLICY IF EXISTS manifestations_select_child ON manifestations;
  DROP POLICY IF EXISTS manifestations_select_adult ON manifestations;

  ALTER TABLE manifestations DISABLE ROW LEVEL SECURITY;

  COMMENT ON TABLE manifestations IS NULL;

  DROP INDEX IF EXISTS idx_work_authors_author_id;
  DROP INDEX IF EXISTS idx_series_parent_id;
  DROP INDEX IF EXISTS idx_webhook_deliveries_webhook_id;
  DROP INDEX IF EXISTS idx_webhooks_user_id;
  DROP INDEX IF EXISTS idx_device_tokens_user_id;
  DROP INDEX IF EXISTS idx_shelves_user_id;
  DROP INDEX IF EXISTS idx_api_cache_expires_at;
  DROP INDEX IF EXISTS idx_ingestion_jobs_status;
  DROP INDEX IF EXISTS idx_ingestion_jobs_batch_id;
  DROP INDEX IF EXISTS idx_shelf_items_manifestation_id;
  DROP INDEX IF EXISTS idx_metadata_versions_manifestation_id;
  DROP INDEX IF EXISTS idx_manifestations_isbn_13;
  DROP INDEX IF EXISTS idx_manifestations_work_id;
  DROP INDEX IF EXISTS idx_series_name_trgm;
  DROP INDEX IF EXISTS idx_authors_name_trgm;
  DROP INDEX IF EXISTS idx_works_title_trgm;
  DROP INDEX IF EXISTS idx_works_search_vector;
  ```
- **GOTCHA**: `current_setting('app.current_user_id', true)::uuid` — `current_setting` with `true` as the second argument returns NULL if the variable is not set (instead of throwing an error). `NULL::uuid` is still NULL, and `id = NULL` evaluates to false, so unauthenticated queries see zero rows. The cast direction is important: casting the *setting* to UUID (not the column to text) preserves PK index usage and normalizes case. The per-operation policy split means INSERT is unrestricted for `tome_app` (app logic controls who can trigger creation) while SELECT/UPDATE/DELETE enforce visibility rules. `tome_ingestion` has unconditional access via its own `FOR ALL` policy. The pgvector column is a SQL comment only, not an actual column. The `\c` command in the manual RLS verification only works in interactive psql — when scripting, use separate `psql` invocations per role.
- **VALIDATE**: `\di` shows 17 indexes (4 FTS/trgm + 7 FK/status/expiry + 6 user-scoped/relational FK). `\dp manifestations` shows 8 policies (2 select, 1 insert, 2 update, 2 delete for `tome_app`/`tome_readonly`, plus 1 `FOR ALL` for `tome_ingestion`). Connect as each role and verify access.

### Task 11: Create docs/schema.md

- **ACTION**: Create schema documentation as an exit criterion
- **IMPLEMENT**: Markdown file with:
  - Entity-relationship overview in ASCII/text
  - Table listing with column descriptions
  - Enum type reference
  - Database role architecture (4 roles, their purposes, and grant summary)
  - RLS contract documentation (per-operation policies, session variable requirement)
  - Notes on FRBR model (Works vs Manifestations)
  - Notes on `is_child`/`role` sync constraint
  - Notes on `ingestion_status` vs `job_status` distinction
  - Notes on `updated_at` trigger coverage (active tables vs reserved)
  - Naming convention: all identifiers use `snake_case`, no hyphens
- **GOTCHA**: This is a living document — keep it minimal and accurate. Don't over-document what the migration SQL already says.
- **VALIDATE**: File exists at `docs/schema.md` and accurately reflects the migration SQL

---

## Testing Strategy

### Migration Tests

| Test | Input | Expected Output | Edge Case? |
|---|---|---|---|
| Migrations run on clean DB | Empty database | All tables created, zero errors | No |
| Migrations are reversible | Fully migrated DB | `sqlx migrate revert` succeeds for each migration (run 7 times) | No |
| Re-running is idempotent | Already-migrated DB | `sqlx migrate run` completes with no changes | No |
| Enum values are correct | `SELECT enum_range(NULL::user_role)` | `{admin,adult,child}` | No |
| FTS trigger works | INSERT into works | search_vector populated automatically | No |
| updated_at trigger works | UPDATE a user row | updated_at changes to now() | No |
| is_child/role CHECK works | INSERT user with is_child=true, role='adult' | CHECK constraint violation | Yes |

### Role Isolation Tests

| Test | Role | Action | Expected | Edge Case? |
|---|---|---|---|---|
| App can read users | `tome_app` | SELECT FROM users | Success | No |
| Ingestion cannot read users | `tome_ingestion` | SELECT FROM users | Permission denied | No |
| Ingestion can write manifestations | `tome_ingestion` | INSERT INTO manifestations | Success (permissive policy) | No |
| Ingestion cannot read shelves | `tome_ingestion` | SELECT FROM shelves | Permission denied | No |
| Ingestion cannot read webhooks | `tome_ingestion` | SELECT FROM webhooks | Permission denied | No |
| Readonly can read all tables | `tome_readonly` | SELECT FROM every table | Success | No |
| Readonly cannot write | `tome_readonly` | INSERT INTO users | Permission denied | No |
| Readonly cannot insert manifestations | `tome_readonly` | INSERT INTO manifestations | Permission denied (grant-level denial before RLS) | No |

### RLS Tests

| Test | Role | Setup | Expected | Edge Case? |
|---|---|---|---|---|
| No session var = no rows | `tome_app` | No set_config | SELECT returns 0 rows | Yes |
| Adult sees all | `tome_app` | set_config adult user | SELECT returns all | No |
| Child sees shelf-only | `tome_app` | set_config child user | SELECT returns shelf-assigned only | No |
| Owner bypasses RLS | `tome` | Any/no set_config | SELECT returns all | Yes — confirms role separation |
| Ingestion sees all | `tome_ingestion` | No set_config | SELECT returns all (permissive) | No |
| Readonly adult sees all | `tome_readonly` | set_config adult user | SELECT returns all | No |
| Readonly child filtered | `tome_readonly` | set_config child user | SELECT returns shelf-assigned only | No |
| App can INSERT without session | `tome_app` | No set_config | INSERT succeeds (WITH CHECK true) | Yes |
| App cannot UPDATE without session | `tome_app` | No set_config | UPDATE affects 0 rows (USING fails) | Yes |
| App cannot DELETE without session | `tome_app` | No set_config | DELETE affects 0 rows (USING fails) | Yes |

Note: `sqlx migrate run --check` verifies whether pending migrations exist that haven't been applied. It does **not** verify reversibility. Reversibility is tested by actually running `sqlx migrate revert` for each migration.

### Manual RLS Verification

Note: The `\c` command below only works in interactive `psql`. When scripting, use separate `psql` invocations per role (e.g., `PGPASSWORD=tome_app psql -h localhost -U tome_app -d tome_dev`).

```sql
-- Setup: insert test data as tome (owner, bypasses RLS)
\c tome_dev tome
INSERT INTO users (id, oidc_subject, display_name, role, is_child)
VALUES
    ('aaaaaaaa-0000-0000-0000-000000000001', 'adult1', 'Adult User', 'adult', false),
    ('aaaaaaaa-0000-0000-0000-000000000002', 'child1', 'Child User', 'child', true);

INSERT INTO works (id, title, sort_title)
VALUES ('bbbbbbbb-0000-0000-0000-000000000001', 'Test Book', 'test book');

INSERT INTO manifestations (id, work_id, format, file_path, file_hash, file_size_bytes)
VALUES ('cccccccc-0000-0000-0000-000000000001',
        'bbbbbbbb-0000-0000-0000-000000000001',
        'epub', '/books/test.epub', 'abc123', 1024);

INSERT INTO shelves (id, user_id, name, is_system)
VALUES ('dddddddd-0000-0000-0000-000000000001',
        'aaaaaaaa-0000-0000-0000-000000000002', 'Reading', true);

INSERT INTO shelf_items (shelf_id, manifestation_id)
VALUES ('dddddddd-0000-0000-0000-000000000001',
        'cccccccc-0000-0000-0000-000000000001');

-- Test as tome_app (adult sees all):
\c tome_dev tome_app
BEGIN;
SELECT set_config('app.current_user_id', 'aaaaaaaa-0000-0000-0000-000000000001', true);
SELECT count(*) FROM manifestations; -- Should return 1
COMMIT;

-- Test as tome_app (child sees shelf-assigned only):
BEGIN;
SELECT set_config('app.current_user_id', 'aaaaaaaa-0000-0000-0000-000000000002', true);
SELECT count(*) FROM manifestations; -- Should return 1 (book is in their shelf)
COMMIT;

-- Test as tome_app (no session variable):
BEGIN;
SELECT count(*) FROM manifestations; -- Should return 0
COMMIT;

-- Test as tome_app (INSERT without session — should succeed):
BEGIN;
INSERT INTO manifestations (work_id, format, file_path, file_hash, file_size_bytes)
VALUES ('bbbbbbbb-0000-0000-0000-000000000001', 'pdf', '/books/test.pdf', 'def456', 2048);
-- Should succeed — INSERT policy is WITH CHECK (true)
ROLLBACK;

-- Test as tome_ingestion (sees all, no session needed):
\c tome_dev tome_ingestion
SELECT count(*) FROM manifestations; -- Should return 1 (permissive policy)

-- Test as tome_ingestion (cannot access user tables):
SELECT * FROM users; -- Should fail: permission denied
SELECT * FROM shelves; -- Should fail: permission denied

-- Test as tome_readonly (reads filtered, cannot write):
\c tome_dev tome_readonly
BEGIN;
SELECT set_config('app.current_user_id', 'aaaaaaaa-0000-0000-0000-000000000001', true);
SELECT count(*) FROM manifestations; -- Should return 1
COMMIT;
INSERT INTO users (oidc_subject, display_name) VALUES ('x', 'x'); -- Should fail: permission denied
```

---

## Validation Commands

### Install sqlx-cli
```bash
cargo install sqlx-cli --no-default-features --features postgres
```

### Start Database
```bash
docker compose up -d db
```
EXPECT: PostgreSQL container healthy. All 4 roles can connect.

### Verify All Roles
```bash
PGPASSWORD=tome psql -h localhost -U tome -d tome_dev -c "SELECT current_user"
PGPASSWORD=tome_app psql -h localhost -U tome_app -d tome_dev -c "SELECT current_user"
PGPASSWORD=tome_ingestion psql -h localhost -U tome_ingestion -d tome_dev -c "SELECT current_user"
PGPASSWORD=tome_readonly psql -h localhost -U tome_readonly -d tome_dev -c "SELECT current_user"
```
EXPECT: All four return their respective role name.

### Run Migrations (as owner)
```bash
cd backend && DATABASE_URL=postgres://tome:tome@localhost:5432/tome_dev sqlx migrate run
```
EXPECT: All 7 migrations applied successfully

### Check Reversibility
```bash
cd backend && DATABASE_URL=postgres://tome:tome@localhost:5432/tome_dev sqlx migrate revert
```
EXPECT: Last migration reverted without error. Repeat 7 times to fully revert, then re-run all migrations.

### Verify Tables
```bash
PGPASSWORD=tome psql -h localhost -U tome -d tome_dev -c "\dt"
```
EXPECT: 21 rows — 20 domain tables + `_sqlx_migrations` (users, works, authors, work_authors, manifestations, series, series_works, omnibus_contents, metadata_versions, tags, manifestation_tags, shelves, shelf_items, device_tokens, api_cache, ingestion_jobs, webhooks, webhook_deliveries, reading_sessions, reading_positions, _sqlx_migrations)

### Verify RLS Policies
```bash
PGPASSWORD=tome psql -h localhost -U tome -d tome_dev -c "\dp manifestations"
```
EXPECT: 8 individual POLICY lines in the output — `manifestations_select_adult`, `manifestations_select_child`, `manifestations_insert`, `manifestations_update_adult`, `manifestations_update_child`, `manifestations_delete_adult`, `manifestations_delete_child`, `manifestations_ingestion_full_access`

### Verify Role Isolation
```bash
PGPASSWORD=tome_ingestion psql -h localhost -U tome_ingestion -d tome_dev -c "SELECT * FROM users"
```
EXPECT: Permission denied (ingestion has no access to users table)

### Cargo Check
```bash
cd backend && cargo check
```
EXPECT: Zero errors (no queries yet, just dependency check)

### Cargo Test
```bash
cd backend && cargo test
```
EXPECT: Existing health_returns_ok test still passes

---

## Acceptance Criteria

- [ ] `sqlx migrate run` succeeds on a clean database (run as `tome` owner)
- [ ] `\dt` in psql shows 21 rows (20 domain tables + `_sqlx_migrations`)
- [ ] All 4 roles can connect to the database
- [ ] `tome_ingestion` cannot access `users`, `shelves`, `shelf_items`, `device_tokens`, `webhooks`, `webhook_deliveries`
- [ ] `tome_readonly` can SELECT all tables but cannot INSERT/UPDATE/DELETE
- [ ] RLS policies visible via `\dp manifestations` (8 policies)
- [ ] RLS manually tested: `tome_app` + adult sees all, child sees shelf-only, no session returns empty
- [ ] RLS manually tested: `tome_app` INSERT succeeds without session variable
- [ ] RLS manually tested: `tome_app` UPDATE/DELETE affect 0 rows without session variable
- [ ] RLS manually tested: `tome_ingestion` sees all rows without session variable
- [ ] RLS manually tested: `tome` owner bypasses RLS (confirms role separation)
- [ ] FTS trigger populates search_vector on works insert/update
- [ ] `updated_at` trigger auto-updates on users, works, manifestations
- [ ] `is_child`/`role` CHECK constraint rejects mismatched combinations
- [ ] All 7 migrations are reversible (revert all, re-apply all)
- [ ] `cargo check` passes with new sqlx dependencies
- [ ] Existing `cargo test` passes (no regressions)
- [ ] `docs/schema.md` committed and accurate
- [ ] No hardcoded secrets — DATABASE_URL in .env.example only

## Completion Checklist

- [ ] Migration SQL follows PostgreSQL best practices
- [ ] All enum types defined before tables that reference them
- [ ] Foreign keys use appropriate ON DELETE behavior
- [ ] Indexes created for all FK columns and search paths
- [ ] 4-role architecture implemented with correct grant scoping
- [ ] `tome_ingestion` grants limited to pipeline tables only
- [ ] RLS policies split per-operation (SELECT/INSERT/UPDATE/DELETE)
- [ ] RLS SELECT policies use USING for visibility filtering
- [ ] RLS INSERT policy uses WITH CHECK (true) — app-level authorization
- [ ] RLS UPDATE policies use USING for targeting + WITH CHECK (true)
- [ ] RLS DELETE policies use USING for targeting
- [ ] `tome_ingestion` has unconditional FOR ALL policy
- [ ] RLS contract documented via COMMENT ON
- [ ] `ingestion_status` vs `job_status` distinction documented in migration comments
- [ ] `is_child`/`role` sync enforced by CHECK constraint
- [ ] `updated_at` auto-managed by trigger on active tables (reserved tables deferred)
- [ ] pgvector reserved as SQL comment only
- [ ] docker-compose.yml matches init-roles.sql and .env.example credentials
- [ ] Underscore convention used throughout (no hyphens in identifiers or enum values)
- [ ] No unnecessary scope additions beyond Step 1

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Enum values need future expansion | Medium | Low | PostgreSQL supports `ALTER TYPE ... ADD VALUE` — no migration break |
| RLS performance on large datasets | Low | Medium | Policies use EXISTS with indexed joins; can add materialized views later |
| pgvector column commented-out may be forgotten | Low | Low | Documented in schema.md and migration comments |
| Docker Compose not in original blueprint | Low | Low | Required for local dev; flagged as additive scope |
| `tome_*` roles not created if volume already exists | Medium | Low | Documented: `docker compose down -v` to reinitialize. Init script only runs on fresh volume. |
| Default PUBLIC grants on types revoked in hardened DB | Low | Medium | Document assumption. Add explicit type grants if deploying to hardened PostgreSQL. |

## Notes

- **docker-compose.yml** is not explicitly in the blueprint but is required to run migrations locally. The README already references `docker compose up`. This is additive scope.
- **docker/init-roles.sql** creates the 3 application roles at container initialization. This is a cluster-level operation that can't live in sqlx migrations (which run against a specific database). The init script approach is standard for Docker PostgreSQL setups.
- **Four database roles**: `tome` (owner) runs migrations and bypasses RLS. `tome_app` (web) serves user requests with RLS enforced. `tome_ingestion` (pipeline) processes files with unconditional access. `tome_readonly` (debug) can only SELECT. This architecture covers all MVP and Phase 2+ features without additional roles.
- **Per-operation RLS policies**: Instead of `FOR ALL` with only `USING`, the plan splits into separate SELECT, INSERT, UPDATE, and DELETE policies. This is PostgreSQL best practice because `USING` and `WITH CHECK` have different semantics per operation. INSERT is unrestricted (`WITH CHECK true`) because the ingestion pipeline creates manifestations without user context and app-level authorization controls who can trigger creation. SELECT/UPDATE/DELETE enforce visibility rules.
- **Migration grouping**: The blueprint lists 22 individual tasks but this plan groups them into 7 logical migrations. For a greenfield schema this is cleaner — each migration is a reviewable unit.
- **The `/plans/` directory is gitignored** — the blueprint itself is not version-controlled. The `docs/schema.md` exit criterion IS tracked.
- **No changes to main.rs** — wiring PgPool into Axum is Step 2 ("Application Skeleton").
- **Underscore convention**: All database identifiers use `snake_case`. The blueprint mentions `sub-genre` with a hyphen but the schema uses `sub_genre` — this is a deliberate normalization.
- **All PKs use UUID with `gen_random_uuid()`** — no sequences are involved, so no `GRANT USAGE ON SEQUENCE` is needed. If SERIAL/IDENTITY columns are added in future migrations, remember to grant sequence usage to the appropriate roles.
