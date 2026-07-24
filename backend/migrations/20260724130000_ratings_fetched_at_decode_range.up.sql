-- The ratings cache's fetched_at joined the schema without the TIMESTAMPTZ
-- decode-range guard every first-party timestamp column carries (see
-- 20260610165400 for the full rationale: `time` is built without
-- `large-dates`, so an out-of-range row would panic at decode). Bring the
-- column under the same invariant; the schema drift test enumerates every
-- TIMESTAMPTZ column and fails without this.

ALTER TABLE public.manifestation_external_ratings
    ADD CONSTRAINT manifestation_external_ratings_fetched_at_ts_decode_range
        CHECK (fetched_at >= '0001-01-01 00:00:00+00' AND fetched_at < '10000-01-01 00:00:00+00');
