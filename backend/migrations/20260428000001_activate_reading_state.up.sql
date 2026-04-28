-- Activate reading_state. Replaces the reserved `reading_positions` table
-- (added at 20260412150007 with a future reader's CFI cursor in mind) with
-- the per-(user, manifestation) progress + last-read shape that Step 10
-- D4 hero screens commit to. See UNK-122.
--
-- The reserved `reading_sessions` table (also at 20260412150007) is a
-- separate feature (per-session analytics) and is intentionally left
-- untouched here.

DROP INDEX IF EXISTS idx_reading_positions_manifestation_id;
DROP TABLE IF EXISTS reading_positions;

CREATE TABLE reading_state (
    user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    manifestation_id  UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
    progress_pct      REAL,
    last_read_at      TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, manifestation_id),
    CONSTRAINT reading_state_progress_pct_range
        CHECK (progress_pct IS NULL OR (progress_pct >= 0 AND progress_pct <= 100)),
    CONSTRAINT reading_state_progress_paired_with_timestamp
        CHECK ((progress_pct IS NULL) = (last_read_at IS NULL))
);

CREATE INDEX idx_reading_state_manifestation_id
    ON reading_state (manifestation_id);

-- Supports "Continue Reading": most recently opened books for a user.
CREATE INDEX idx_reading_state_user_last_read
    ON reading_state (user_id, last_read_at DESC NULLS LAST);

CREATE TRIGGER trg_reading_state_updated_at
    BEFORE UPDATE ON reading_state
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

GRANT SELECT, INSERT, UPDATE, DELETE ON reading_state TO reverie_app;
GRANT SELECT ON reading_state TO reverie_readonly;

-- RLS: each authenticated user sees only their own rows. Mirrors the
-- existing manifestations RLS contract — caller must SET LOCAL
-- app.current_user_id inside a transaction. See db::acquire_with_rls.
ALTER TABLE reading_state ENABLE ROW LEVEL SECURITY;

COMMENT ON TABLE reading_state IS
    'RLS enabled. reverie_app and reverie_readonly must SET LOCAL app.current_user_id in a transaction. Each user sees only rows where user_id matches the GUC. reverie (owner) bypasses RLS.';

CREATE POLICY reading_state_owner ON reading_state
    FOR ALL
    TO reverie_app, reverie_readonly
    USING (user_id = current_setting('app.current_user_id', true)::uuid)
    WITH CHECK (user_id = current_setting('app.current_user_id', true)::uuid);
