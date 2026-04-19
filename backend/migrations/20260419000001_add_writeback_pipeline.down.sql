-- Reverses 20260419000001_add_writeback_pipeline.up.sql
--
-- NOTE: `DROP TYPE writeback_status` will fail if any `writeback_jobs` rows
-- still reference it via their `status` column. The down migration drops the
-- table first, which in turn drops all rows, so this ordering is safe.
-- Same hazard pattern as 20260414100001 (the `'skipped'` enum addition).

---------------------------------------------------------------------------
-- 1. Reverse RLS system-context policies.
---------------------------------------------------------------------------

DROP POLICY IF EXISTS manifestations_update_system ON manifestations;
DROP POLICY IF EXISTS manifestations_select_system ON manifestations;

---------------------------------------------------------------------------
-- 2. Reverse grants
---------------------------------------------------------------------------

REVOKE ALL ON writeback_jobs FROM reverie_readonly;
REVOKE ALL ON writeback_jobs FROM reverie_ingestion;
REVOKE ALL ON writeback_jobs FROM reverie_app;

---------------------------------------------------------------------------
-- 2. Restore single file_hash column.
---------------------------------------------------------------------------

ALTER TABLE manifestations DROP COLUMN current_file_hash;
ALTER TABLE manifestations RENAME COLUMN ingestion_file_hash TO file_hash;

---------------------------------------------------------------------------
-- 3. Drop writeback_jobs table + supporting indexes (dropped implicitly).
---------------------------------------------------------------------------

DROP INDEX IF EXISTS idx_writeback_jobs_manifestation_status;
DROP INDEX IF EXISTS idx_writeback_jobs_queue;
DROP TABLE IF EXISTS writeback_jobs;

---------------------------------------------------------------------------
-- 4. Drop the enum type last (only possible once no rows reference it).
---------------------------------------------------------------------------

DROP TYPE writeback_status;
