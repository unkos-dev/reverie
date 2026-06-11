-- Guard every first-party TIMESTAMPTZ column to the range the Rust side can
-- decode. `time` is built without `large-dates` (Cargo.toml), so
-- `OffsetDateTime` only represents years -9999..=9999 — but Postgres
-- TIMESTAMPTZ reaches year 294276 plus the `infinity`/`-infinity` specials.
-- A row outside the decodable range (reachable only by out-of-band mutation;
-- all first-party write paths go through `OffsetDateTime`) panics at the
-- first `row.get::<OffsetDateTime>` / macro decode, turning into a bare 500
-- that bypasses the Problem Details envelope (UNK-310).
--
-- Fail closed at the boundary that creates the value: the bad INSERT/UPDATE
-- is rejected, so no read path can ever see an undecodable row.
--
-- Bounds: upper bound `< 10000-01-01` is the hard decode limit. Lower bound
-- `>= 0001-01-01` is deliberately tighter than the decode limit (-9999):
-- no Reverie timestamp can legitimately predate the common era, and the
-- finite lower bound also rejects `-infinity` (the upper bound likewise
-- rejects `infinity`). NULLs pass CHECK constraints by definition, so
-- nullable columns keep their NULL semantics.
--
-- `_sqlx_migrations.installed_on` is excluded: the table is sqlx-managed and
-- only ever written by the migrator with `now()`.

ALTER TABLE public.users
    ADD CONSTRAINT users_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT users_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.works
    ADD CONSTRAINT works_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT works_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.authors
    ADD CONSTRAINT authors_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.series
    ADD CONSTRAINT series_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.metadata_sources
    ADD CONSTRAINT metadata_sources_added_at_ts_decode_range
        CHECK (added_at >= '0001-01-01 00:00:00+00' AND added_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.manifestations
    ADD CONSTRAINT manifestations_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT manifestations_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT manifestations_enrichment_attempted_at_ts_decode_range
        CHECK (enrichment_attempted_at >= '0001-01-01 00:00:00+00' AND enrichment_attempted_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.metadata_versions
    ADD CONSTRAINT metadata_versions_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT metadata_versions_resolved_at_ts_decode_range
        CHECK (resolved_at >= '0001-01-01 00:00:00+00' AND resolved_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT metadata_versions_first_seen_at_ts_decode_range
        CHECK (first_seen_at >= '0001-01-01 00:00:00+00' AND first_seen_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT metadata_versions_last_seen_at_ts_decode_range
        CHECK (last_seen_at >= '0001-01-01 00:00:00+00' AND last_seen_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.field_locks
    ADD CONSTRAINT field_locks_locked_at_ts_decode_range
        CHECK (locked_at >= '0001-01-01 00:00:00+00' AND locked_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.shelves
    ADD CONSTRAINT shelves_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT shelves_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.shelf_items
    ADD CONSTRAINT shelf_items_added_at_ts_decode_range
        CHECK (added_at >= '0001-01-01 00:00:00+00' AND added_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.device_tokens
    ADD CONSTRAINT device_tokens_last_used_at_ts_decode_range
        CHECK (last_used_at >= '0001-01-01 00:00:00+00' AND last_used_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT device_tokens_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT device_tokens_revoked_at_ts_decode_range
        CHECK (revoked_at >= '0001-01-01 00:00:00+00' AND revoked_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.api_cache
    ADD CONSTRAINT api_cache_fetched_at_ts_decode_range
        CHECK (fetched_at >= '0001-01-01 00:00:00+00' AND fetched_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT api_cache_expires_at_ts_decode_range
        CHECK (expires_at >= '0001-01-01 00:00:00+00' AND expires_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.ingestion_jobs
    ADD CONSTRAINT ingestion_jobs_started_at_ts_decode_range
        CHECK (started_at >= '0001-01-01 00:00:00+00' AND started_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT ingestion_jobs_completed_at_ts_decode_range
        CHECK (completed_at >= '0001-01-01 00:00:00+00' AND completed_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT ingestion_jobs_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.webhooks
    ADD CONSTRAINT webhooks_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.webhook_deliveries
    ADD CONSTRAINT webhook_deliveries_delivered_at_ts_decode_range
        CHECK (delivered_at >= '0001-01-01 00:00:00+00' AND delivered_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.reading_sessions
    ADD CONSTRAINT reading_sessions_started_at_ts_decode_range
        CHECK (started_at >= '0001-01-01 00:00:00+00' AND started_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT reading_sessions_ended_at_ts_decode_range
        CHECK (ended_at >= '0001-01-01 00:00:00+00' AND ended_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.reading_state
    ADD CONSTRAINT reading_state_last_read_at_ts_decode_range
        CHECK (last_read_at >= '0001-01-01 00:00:00+00' AND last_read_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT reading_state_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT reading_state_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.writeback_jobs
    ADD CONSTRAINT writeback_jobs_last_attempted_at_ts_decode_range
        CHECK (last_attempted_at >= '0001-01-01 00:00:00+00' AND last_attempted_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT writeback_jobs_completed_at_ts_decode_range
        CHECK (completed_at >= '0001-01-01 00:00:00+00' AND completed_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT writeback_jobs_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00');

ALTER TABLE public.settings
    ADD CONSTRAINT settings_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00');

-- The tower_sessions schema is created by our own initial_schema migration
-- and read by the first-party session store (auth/store.rs), so its
-- timestamp gets the same guard.
ALTER TABLE tower_sessions.session
    ADD CONSTRAINT session_expiry_date_ts_decode_range
        CHECK (expiry_date >= '0001-01-01 00:00:00+00' AND expiry_date < '10000-01-01 00:00:00+00');
