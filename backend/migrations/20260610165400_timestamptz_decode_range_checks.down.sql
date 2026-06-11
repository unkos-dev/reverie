ALTER TABLE public.users
    DROP CONSTRAINT users_created_at_ts_decode_range,
    DROP CONSTRAINT users_updated_at_ts_decode_range;

ALTER TABLE public.works
    DROP CONSTRAINT works_created_at_ts_decode_range,
    DROP CONSTRAINT works_updated_at_ts_decode_range;

ALTER TABLE public.authors
    DROP CONSTRAINT authors_created_at_ts_decode_range;

ALTER TABLE public.series
    DROP CONSTRAINT series_created_at_ts_decode_range;

ALTER TABLE public.metadata_sources
    DROP CONSTRAINT metadata_sources_added_at_ts_decode_range;

ALTER TABLE public.manifestations
    DROP CONSTRAINT manifestations_created_at_ts_decode_range,
    DROP CONSTRAINT manifestations_updated_at_ts_decode_range,
    DROP CONSTRAINT manifestations_enrichment_attempted_at_ts_decode_range;

ALTER TABLE public.metadata_versions
    DROP CONSTRAINT metadata_versions_created_at_ts_decode_range,
    DROP CONSTRAINT metadata_versions_resolved_at_ts_decode_range,
    DROP CONSTRAINT metadata_versions_first_seen_at_ts_decode_range,
    DROP CONSTRAINT metadata_versions_last_seen_at_ts_decode_range;

ALTER TABLE public.field_locks
    DROP CONSTRAINT field_locks_locked_at_ts_decode_range;

ALTER TABLE public.shelves
    DROP CONSTRAINT shelves_created_at_ts_decode_range,
    DROP CONSTRAINT shelves_updated_at_ts_decode_range;

ALTER TABLE public.shelf_items
    DROP CONSTRAINT shelf_items_added_at_ts_decode_range;

ALTER TABLE public.device_tokens
    DROP CONSTRAINT device_tokens_last_used_at_ts_decode_range,
    DROP CONSTRAINT device_tokens_created_at_ts_decode_range,
    DROP CONSTRAINT device_tokens_revoked_at_ts_decode_range;

ALTER TABLE public.api_cache
    DROP CONSTRAINT api_cache_fetched_at_ts_decode_range,
    DROP CONSTRAINT api_cache_expires_at_ts_decode_range;

ALTER TABLE public.ingestion_jobs
    DROP CONSTRAINT ingestion_jobs_started_at_ts_decode_range,
    DROP CONSTRAINT ingestion_jobs_completed_at_ts_decode_range,
    DROP CONSTRAINT ingestion_jobs_created_at_ts_decode_range;

ALTER TABLE public.webhooks
    DROP CONSTRAINT webhooks_created_at_ts_decode_range;

ALTER TABLE public.webhook_deliveries
    DROP CONSTRAINT webhook_deliveries_delivered_at_ts_decode_range;

ALTER TABLE public.reading_sessions
    DROP CONSTRAINT reading_sessions_started_at_ts_decode_range,
    DROP CONSTRAINT reading_sessions_ended_at_ts_decode_range;

ALTER TABLE public.reading_state
    DROP CONSTRAINT reading_state_last_read_at_ts_decode_range,
    DROP CONSTRAINT reading_state_created_at_ts_decode_range,
    DROP CONSTRAINT reading_state_updated_at_ts_decode_range;

ALTER TABLE public.writeback_jobs
    DROP CONSTRAINT writeback_jobs_last_attempted_at_ts_decode_range,
    DROP CONSTRAINT writeback_jobs_completed_at_ts_decode_range,
    DROP CONSTRAINT writeback_jobs_created_at_ts_decode_range;

ALTER TABLE public.settings
    DROP CONSTRAINT settings_updated_at_ts_decode_range;

ALTER TABLE tower_sessions.session
    DROP CONSTRAINT session_expiry_date_ts_decode_range;
