-- Step 7: Metadata Enrichment Pipeline — three-layer schema.
--
-- Replaces the flat "one draft per source per field" model with:
--   (1) metadata_sources  — registry of known providers (FK target)
--   (2) metadata_versions — append-only journal keyed on value_hash
--   (3) canonical pointer columns (*_version_id) on works/manifestations/joins
--
-- See plans/BLUEPRINT.md Step 7 and
-- ~/.claude/projects/-home-coder-Tome/memory/project_enrichment_architecture.md
-- for the architecture rationale.

-- Required for the value_hash backfill (see section 4). Safe no-op if already
-- enabled; cannot be reversed in down.sql without breaking any other consumer,
-- so we leave the extension in place on revert.
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

---------------------------------------------------------------------------
-- 1. Source registry
---------------------------------------------------------------------------

CREATE TABLE metadata_sources (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    base_priority INTEGER NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    added_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO metadata_sources (id, display_name, kind, base_priority) VALUES
    ('opf',         'OPF Metadata',    'file', 100),
    ('manual',      'Manual Override', 'user', 10),
    ('openlibrary', 'Open Library',    'api',  100),
    ('googlebooks', 'Google Books',    'api',  100),
    ('hardcover',   'Hardcover',       'api',  90),
    ('ai',          'AI-assisted',     'ai',   500);

---------------------------------------------------------------------------
-- 2. Enrichment queue + cache enums
---------------------------------------------------------------------------

CREATE TYPE enrichment_status AS ENUM ('pending', 'in_progress', 'complete', 'failed', 'skipped');
CREATE TYPE api_cache_kind   AS ENUM ('hit', 'miss', 'error');

---------------------------------------------------------------------------
-- 3. Simplify metadata_review_status: draft|accepted|rejected → pending|rejected
--    Canonical pointer columns become the source of truth for "accepted".
--    ENUM_REBUILD pattern (rename → recreate → alter → drop old).
---------------------------------------------------------------------------

ALTER TYPE metadata_review_status RENAME TO metadata_review_status_old;

CREATE TYPE metadata_review_status AS ENUM ('pending', 'rejected');

-- Drop the column default BEFORE altering the column type — Postgres cannot
-- auto-cast a typed default across enum rebuilds.
ALTER TABLE metadata_versions ALTER COLUMN status DROP DEFAULT;

ALTER TABLE metadata_versions
    ALTER COLUMN status TYPE metadata_review_status
    USING CASE status::text
        WHEN 'draft'    THEN 'pending'::metadata_review_status
        WHEN 'accepted' THEN 'pending'::metadata_review_status
        WHEN 'rejected' THEN 'rejected'::metadata_review_status
    END;

ALTER TABLE metadata_versions ALTER COLUMN status SET DEFAULT 'pending';

DROP TYPE metadata_review_status_old;

---------------------------------------------------------------------------
-- 4. Rewrite metadata_versions to the journal shape.
--
--    Tombstone note: the one-draft-per-source-per-field unique constraint
--    added in 20260415000003 is dropped here. The journal dedupes on value_hash
--    so the same (manifestation, source, field) may hold many distinct values
--    across time; duplicates of the SAME value are collapsed via observation
--    counting on the (manifestation, source, field, value_hash) unique.
---------------------------------------------------------------------------

ALTER TABLE metadata_versions
    DROP CONSTRAINT IF EXISTS metadata_versions_manifestation_source_field_unique;

-- New columns. value_hash + match_type need defaults temporarily so the ADD
-- COLUMN succeeds against existing rows; we drop the defaults after backfill.
ALTER TABLE metadata_versions
    ADD COLUMN value_hash        BYTEA       NOT NULL DEFAULT '\x00',
    ADD COLUMN match_type        TEXT        NOT NULL DEFAULT 'title',
    ADD COLUMN first_seen_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN last_seen_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN observation_count INTEGER     NOT NULL DEFAULT 1;

-- Backfill: a coarse SHA-256 over new_value's JSON text form. Acceptable
-- because existing rows are OPF drafts only and this is the same hash shape
-- the value_hash module will produce going forward (see services/enrichment/
-- value_hash.rs). The app may compute slightly different hashes for some
-- normalised fields post-migration; those rows will dedup-merge on next
-- observation via the UNIQUE constraint's ON CONFLICT path.
UPDATE metadata_versions
   SET value_hash = digest(COALESCE(new_value::text, ''), 'sha256');

ALTER TABLE metadata_versions ALTER COLUMN value_hash DROP DEFAULT;
ALTER TABLE metadata_versions ALTER COLUMN match_type DROP DEFAULT;

-- Convert source column from enum to TEXT with FK into the registry.
-- The existing enum labels are all present in the seeded registry.
ALTER TABLE metadata_versions ALTER COLUMN source TYPE TEXT USING source::text;

ALTER TABLE metadata_versions
    ADD CONSTRAINT metadata_versions_source_fk
    FOREIGN KEY (source) REFERENCES metadata_sources(id);

-- The metadata_source enum is now unused.
DROP TYPE metadata_source;

-- New uniqueness: dedup on value_hash within (manifestation, source, field).
ALTER TABLE metadata_versions
    ADD CONSTRAINT metadata_versions_mfs_hash_unique
    UNIQUE (manifestation_id, source, field_name, value_hash);

CREATE INDEX idx_mv_manifestation_field ON metadata_versions (manifestation_id, field_name);
CREATE INDEX idx_mv_last_seen           ON metadata_versions (last_seen_at);

---------------------------------------------------------------------------
-- 5. Canonical pointer columns on works / manifestations / join tables.
--    Every displayed value traces back to its metadata_versions row.
---------------------------------------------------------------------------

ALTER TABLE works
    ADD COLUMN title_version_id       UUID REFERENCES metadata_versions(id) ON DELETE SET NULL,
    ADD COLUMN description_version_id UUID REFERENCES metadata_versions(id) ON DELETE SET NULL,
    ADD COLUMN language_version_id    UUID REFERENCES metadata_versions(id) ON DELETE SET NULL;

ALTER TABLE manifestations
    ADD COLUMN publisher_version_id  UUID REFERENCES metadata_versions(id) ON DELETE SET NULL,
    ADD COLUMN pub_date_version_id   UUID REFERENCES metadata_versions(id) ON DELETE SET NULL,
    ADD COLUMN isbn_10_version_id    UUID REFERENCES metadata_versions(id) ON DELETE SET NULL,
    ADD COLUMN isbn_13_version_id    UUID REFERENCES metadata_versions(id) ON DELETE SET NULL,
    ADD COLUMN cover_path            TEXT,
    ADD COLUMN cover_sha256          BYTEA,
    ADD COLUMN cover_size_bytes      BIGINT,
    ADD COLUMN cover_source          TEXT REFERENCES metadata_sources(id),
    ADD COLUMN cover_version_id      UUID REFERENCES metadata_versions(id) ON DELETE SET NULL;

ALTER TABLE work_authors
    ADD COLUMN source_version_id UUID REFERENCES metadata_versions(id) ON DELETE SET NULL;

ALTER TABLE manifestation_tags
    ADD COLUMN source_version_id UUID REFERENCES metadata_versions(id) ON DELETE SET NULL;

---------------------------------------------------------------------------
-- 6. Work rematch pointer (set when ISBN correction reveals a duplicate).
---------------------------------------------------------------------------

ALTER TABLE manifestations
    ADD COLUMN suspected_duplicate_work_id UUID REFERENCES works(id) ON DELETE SET NULL;

---------------------------------------------------------------------------
-- 7. Field locks (user-declared "do not touch").
---------------------------------------------------------------------------

CREATE TABLE field_locks (
    manifestation_id UUID        NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
    entity_type      TEXT        NOT NULL,
    field_name       TEXT        NOT NULL,
    locked_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_by        UUID        REFERENCES users(id) ON DELETE SET NULL,
    PRIMARY KEY (manifestation_id, entity_type, field_name)
);

---------------------------------------------------------------------------
-- 8. Enrichment queue columns on manifestations.
---------------------------------------------------------------------------

ALTER TABLE manifestations
    ADD COLUMN enrichment_status        enrichment_status NOT NULL DEFAULT 'pending',
    ADD COLUMN enrichment_attempted_at  TIMESTAMPTZ,
    ADD COLUMN enrichment_attempt_count INTEGER           NOT NULL DEFAULT 0,
    ADD COLUMN enrichment_error         TEXT;

-- Partial index for the claim CTE: only rows the queue might pick up.
CREATE INDEX idx_manifestations_enrichment_queue
    ON manifestations (enrichment_status, enrichment_attempted_at NULLS FIRST)
    WHERE enrichment_status IN ('pending', 'failed');

---------------------------------------------------------------------------
-- 9. API cache kind + http_status columns.
---------------------------------------------------------------------------

ALTER TABLE api_cache
    ADD COLUMN response_kind api_cache_kind NOT NULL DEFAULT 'hit',
    ADD COLUMN http_status   INT;

---------------------------------------------------------------------------
-- 10. Grants (PER_ROLE_GRANTS).
---------------------------------------------------------------------------

-- metadata_sources: FK-readable to pipeline + readonly; full DML to app.
GRANT SELECT, INSERT, UPDATE, DELETE ON metadata_sources TO tome_app;
GRANT SELECT                          ON metadata_sources TO tome_ingestion;
GRANT SELECT                          ON metadata_sources TO tome_readonly;

-- field_locks: user-owned; no ingestion access; readable to readonly.
GRANT SELECT, INSERT, UPDATE, DELETE ON field_locks TO tome_app;
GRANT SELECT                          ON field_locks TO tome_readonly;

-- No grant changes on existing rewritten tables (metadata_versions, manifestations,
-- works, work_authors, manifestation_tags, api_cache) — existing grants remain valid.
