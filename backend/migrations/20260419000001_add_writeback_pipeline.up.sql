-- Step 8: Metadata Writeback Pipeline — queue table + file-hash split.
--
-- Adds the `writeback_jobs` queue (drained by the background worker spawned
-- from main.rs) and splits `manifestations.file_hash` into two columns so
-- Step 11 (Library Health) can surface divergence between ingestion-time
-- state and post-writeback state.
--
-- See plans/BLUEPRINT.md Step 8 lines 929–1281.

---------------------------------------------------------------------------
-- 1. Writeback status enum
---------------------------------------------------------------------------

-- Mirrors `enrichment_status` shape.  `skipped` here means one of two
-- things, both terminal: (a) the manifestation format is not supported
-- for writeback (e.g. non-EPUB) so we never tried; (b) the retry limit
-- was exhausted (mirrors Step 7's reuse of the label for "we tried").
CREATE TYPE writeback_status AS ENUM ('pending', 'in_progress', 'complete', 'failed', 'skipped');

---------------------------------------------------------------------------
-- 2. Writeback queue table
---------------------------------------------------------------------------

CREATE TABLE writeback_jobs (
    id                 UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    manifestation_id   UUID             NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
    reason             TEXT             NOT NULL,
    status             writeback_status NOT NULL DEFAULT 'pending',
    attempt_count      INTEGER          NOT NULL DEFAULT 0,
    last_attempted_at  TIMESTAMPTZ,
    completed_at       TIMESTAMPTZ,
    error              TEXT,
    created_at         TIMESTAMPTZ      NOT NULL DEFAULT now(),
    CONSTRAINT writeback_jobs_reason_chk CHECK (reason IN ('metadata', 'cover'))
);

COMMENT ON TABLE  writeback_jobs IS
  'Queue of pending/in-flight OPF writeback operations. One row per canonical pointer move. Drained by services::writeback::queue.';
COMMENT ON COLUMN writeback_jobs.reason IS
  '''metadata'' for text/field pointer moves; ''cover'' when a new cover sidecar needs embedding.';
COMMENT ON COLUMN writeback_jobs.status IS
  'pending → in_progress → (complete | failed | skipped). ''skipped'' is terminal; see enum comment.';

-- Partial index for the claim CTE: only rows the worker might pick up.
-- Mirrors the Step 7 `idx_manifestations_enrichment_queue` shape.
CREATE INDEX idx_writeback_jobs_queue
    ON writeback_jobs (last_attempted_at NULLS FIRST, created_at)
    WHERE status IN ('pending', 'failed');

-- Look-up index for the claim CTE's manifestation-aware NOT EXISTS clause
-- (serialise writes-for-the-same-manifestation).
CREATE INDEX idx_writeback_jobs_manifestation_status
    ON writeback_jobs (manifestation_id, status);

---------------------------------------------------------------------------
-- 3. Split manifestations.file_hash into ingestion + current.
--
--    `ingestion_file_hash` is immutable after ingestion — an audit trail.
--    `current_file_hash` tracks the on-disk state after each successful
--    writeback; Step 11 health surfaces divergence from sha256(file_path).
--    Type is TEXT (hex-encoded SHA-256) to match the existing `file_hash`
--    column and the downstream code that reads/writes it via `String`.
---------------------------------------------------------------------------

ALTER TABLE manifestations RENAME COLUMN file_hash TO ingestion_file_hash;

ALTER TABLE manifestations
    ADD COLUMN current_file_hash TEXT NOT NULL DEFAULT '';

UPDATE manifestations SET current_file_hash = ingestion_file_hash;

ALTER TABLE manifestations ALTER COLUMN current_file_hash DROP DEFAULT;

COMMENT ON COLUMN manifestations.ingestion_file_hash IS
  'SHA-256 of file at ingestion time. Immutable after initial insert — audit trail.';
COMMENT ON COLUMN manifestations.current_file_hash IS
  'SHA-256 of file as of last successful writeback. Equals ingestion_file_hash until first writeback. Step 11 health surfaces divergence from on-disk hash.';

---------------------------------------------------------------------------
-- 4. Per-role grants.
--
--    Worker runs on reverie_app.  Step 7's enrichment orchestrator opens
--    its tx on the ingestion pool (reverie_ingestion) and emits jobs from
--    inside that tx (see services/enrichment/orchestrator.rs `Decision::Apply`);
--    thus reverie_ingestion needs INSERT to emit + SELECT so INSERT..RETURNING
--    works — but no UPDATE/DELETE because it never drains jobs.  The
--    invariant "ingestion never writes back to managed files" is scoped to
--    file-mutation, not job-emission bookkeeping.
---------------------------------------------------------------------------

GRANT SELECT, INSERT, UPDATE, DELETE ON writeback_jobs TO reverie_app;
GRANT SELECT, INSERT                 ON writeback_jobs TO reverie_ingestion;
GRANT SELECT                         ON writeback_jobs TO reverie_readonly;

---------------------------------------------------------------------------
-- 5. RLS: system-context access for the writeback worker.
--
--    The writeback worker runs on `reverie_app` (full DML on writeback_jobs
--    + UPDATE on manifestations.current_file_hash) but has no end-user
--    context, so the existing user-facing SELECT/UPDATE policies on
--    `manifestations` filter out every row.  Add a "system" policy that
--    unblocks the worker when `app.current_user_id` is unset — user-facing
--    handlers continue to SET LOCAL that variable and hit the user
--    policies.
---------------------------------------------------------------------------

CREATE POLICY manifestations_select_system ON manifestations
    FOR SELECT
    TO reverie_app
    USING (
        current_setting('app.current_user_id', true) IS NULL
        OR current_setting('app.current_user_id', true) = ''
    );

CREATE POLICY manifestations_update_system ON manifestations
    FOR UPDATE
    TO reverie_app
    USING (
        current_setting('app.current_user_id', true) IS NULL
        OR current_setting('app.current_user_id', true) = ''
    )
    WITH CHECK (true);
