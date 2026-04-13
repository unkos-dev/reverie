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
CREATE INDEX idx_series_works_work_id ON series_works (work_id);
CREATE INDEX idx_omnibus_contents_contained_work_id ON omnibus_contents (contained_work_id);
CREATE INDEX idx_manifestation_tags_tag_id ON manifestation_tags (tag_id);
CREATE INDEX idx_metadata_versions_resolved_by ON metadata_versions (resolved_by) WHERE resolved_by IS NOT NULL;

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

-- UPDATE: restricted to admin/adult only. Children manage visibility through
-- shelf_items, not by modifying shared manifestation records.
CREATE POLICY manifestations_update ON manifestations
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

-- DELETE: restricted to admin/adult only. Same rationale as UPDATE.
CREATE POLICY manifestations_delete ON manifestations
    FOR DELETE
    TO tome_app
    USING (
        EXISTS (
            SELECT 1 FROM users
            WHERE id = current_setting('app.current_user_id', true)::uuid
            AND role IN ('admin', 'adult')
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

-- FK indexes for reserved tables (add now while tables are empty)
CREATE INDEX idx_reading_sessions_user_id ON reading_sessions (user_id);
CREATE INDEX idx_reading_sessions_manifestation_id ON reading_sessions (manifestation_id);
CREATE INDEX idx_reading_positions_manifestation_id ON reading_positions (manifestation_id);
-- reading_positions(user_id) is covered by the UNIQUE(user_id, manifestation_id) index

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
