-- Reverse of the reading-domain migration. Local-dev reversibility only, not
-- a production rollback; these migrations roll up into the base schema
-- before the first release.

ALTER TABLE public.reading_state
    DROP CONSTRAINT reading_state_finished_at_ts_decode_range,
    DROP CONSTRAINT reading_state_started_at_ts_decode_range,
    DROP CONSTRAINT reading_state_rating_range,
    DROP COLUMN finished_at,
    DROP COLUMN started_at,
    DROP COLUMN notes,
    DROP COLUMN rating,
    DROP COLUMN status;

DROP TYPE public.reading_status;
