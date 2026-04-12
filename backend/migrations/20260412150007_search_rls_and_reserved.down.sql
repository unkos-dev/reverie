REVOKE ALL ON reading_positions FROM tome_readonly;
REVOKE ALL ON reading_sessions FROM tome_readonly;
REVOKE ALL ON reading_positions FROM tome_app;
REVOKE ALL ON reading_sessions FROM tome_app;

DROP INDEX IF EXISTS idx_reading_positions_manifestation_id;
DROP INDEX IF EXISTS idx_reading_sessions_manifestation_id;
DROP INDEX IF EXISTS idx_reading_sessions_user_id;

DROP TABLE IF EXISTS reading_positions;
DROP TABLE IF EXISTS reading_sessions;

DROP POLICY IF EXISTS manifestations_ingestion_full_access ON manifestations;
DROP POLICY IF EXISTS manifestations_delete ON manifestations;
DROP POLICY IF EXISTS manifestations_update ON manifestations;
DROP POLICY IF EXISTS manifestations_insert ON manifestations;
DROP POLICY IF EXISTS manifestations_select_child ON manifestations;
DROP POLICY IF EXISTS manifestations_select_adult ON manifestations;

ALTER TABLE manifestations DISABLE ROW LEVEL SECURITY;

COMMENT ON TABLE manifestations IS NULL;

DROP INDEX IF EXISTS idx_metadata_versions_resolved_by;
DROP INDEX IF EXISTS idx_manifestation_tags_tag_id;
DROP INDEX IF EXISTS idx_omnibus_contents_contained_work_id;
DROP INDEX IF EXISTS idx_series_works_work_id;
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
