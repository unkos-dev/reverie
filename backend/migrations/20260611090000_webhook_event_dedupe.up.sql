-- UNK-98: webhook terminal-event dedupe set.
--
-- services::writeback::queue::finish emits terminal webhooks BEFORE the DB
-- bookkeeping UPDATE so a transient DB failure cannot silently drop the
-- event. The cost is a double-dispatch risk: DB failure on the mark_*
-- UPDATE followed by crash-recovery retry re-fires the same terminal event.
-- The dispatcher dedupes on a stable event id ('writeback:{job_id}:{outcome}')
-- within a TTL window before delivering (services::writeback::events).
--
-- Postgres-backed rather than in-memory: the duplicate arrives after a
-- process crash, so an in-memory set would be empty exactly when it is
-- needed. See adr/2026-06-08-postgres-backed-crash-safe-state.md.
--
-- No FK to writeback_jobs: job rows can vanish (manifestation CASCADE)
-- while their terminal event must still dedupe.
CREATE TABLE public.webhook_event_dedupe (
    event_id text PRIMARY KEY,
    seen_at timestamptz DEFAULT now() NOT NULL,
    -- Decode-range guard, mandated for every first-party TIMESTAMPTZ column
    -- (see 20260610165400 and db::tests::every_timestamptz_column_has_decode_range_check).
    CONSTRAINT webhook_event_dedupe_seen_at_ts_decode_range
        CHECK (seen_at >= '0001-01-01 00:00:00+00' AND seen_at < '10000-01-01 00:00:00+00')
);

-- The dispatcher's opportunistic purge filters and the dedupe check both
-- predicate on seen_at; keeps the TTL scan off a seq scan as the set grows.
CREATE INDEX idx_webhook_event_dedupe_seen_at ON public.webhook_event_dedupe USING btree (seen_at);

COMMENT ON TABLE public.webhook_event_dedupe IS 'Short-TTL dedupe set for terminal webhook events (UNK-98). Keyed by stable event id (writeback:{job_id}:{outcome}). Rows past the TTL are purged opportunistically on dispatch; see services::writeback::events::dispatch.';

-- Not user-scoped (no user_id column) — RLS-exempt like tower_sessions;
-- access controlled at the role-grant boundary. The writeback worker
-- connects as reverie_app (system-context pool).
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.webhook_event_dedupe TO reverie_app;
GRANT SELECT ON TABLE public.webhook_event_dedupe TO reverie_readonly;
