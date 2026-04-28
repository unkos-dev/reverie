-- Reverse 20260428000001_activate_reading_state.up.sql.
-- Restores the reserved `reading_positions` table to the shape it had at
-- 20260412150007_search_rls_and_reserved.up.sql.

DROP TABLE IF EXISTS reading_state;

CREATE TABLE reading_positions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
    position_cfi TEXT,
    percentage REAL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, manifestation_id)
);

CREATE INDEX idx_reading_positions_manifestation_id
    ON reading_positions (manifestation_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON reading_positions TO reverie_app;
GRANT SELECT ON reading_positions TO reverie_readonly;
