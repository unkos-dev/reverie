-- Step 8 follow-up: enforce per-manifestation writeback serialisation at the
-- database layer.
--
-- The claim CTE in services::writeback::queue uses NOT EXISTS + FOR UPDATE
-- SKIP LOCKED to try to prevent two workers running writebacks on the same
-- manifestation concurrently.  Under READ COMMITTED, that predicate cannot
-- see a peer worker's uncommitted `status = 'in_progress'` UPDATE, so two
-- workers can simultaneously claim DIFFERENT pending jobs for the same
-- manifestation.  The NOT EXISTS is a soft filter that happens to work in
-- single-replica, single-process MVP only because the in-process serial
-- `claim_next` loop collapses the race.
--
-- This partial unique index gives the database-level guarantee the module
-- claims to provide: at most one `in_progress` row per manifestation, full
-- stop.  When two workers race, the second one's UPDATE blocks on the first
-- one's uncommitted index tuple, then fails with SQLSTATE 23505
-- (unique_violation) once the first commits.  The worker's `claim_next`
-- translates that into `Ok(None)` so the runaway caller just polls again
-- cleanly.
--
-- We keep the NOT EXISTS clause and `idx_writeback_jobs_manifestation_status`
-- index: they remain useful as a cheap filter on the common-case path where
-- a sibling is already in_progress, so we don't pay a unique-violation
-- round-trip on every claim attempt.
CREATE UNIQUE INDEX idx_writeback_jobs_in_progress_unique
    ON writeback_jobs (manifestation_id)
    WHERE status = 'in_progress';

COMMENT ON INDEX idx_writeback_jobs_in_progress_unique IS
  'Enforces at most one in_progress writeback job per manifestation. Load-bearing for multi-replica correctness; see services::writeback::queue::claim_next for the retry path on SQLSTATE 23505.';
