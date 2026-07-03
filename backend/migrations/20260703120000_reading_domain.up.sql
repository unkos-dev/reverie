-- Reading domain: status enum, per-user rating, notes, reading dates.
-- Adds columns to the existing reading_state table; the table's RLS policy,
-- grants, and updated_at trigger already cover every column and are
-- untouched by this migration (new columns inherit them for free).

CREATE TYPE public.reading_status AS ENUM (
    'want_to_read',
    'reading',
    'on_hold',
    'finished',
    'abandoned'
);

ALTER TABLE public.reading_state
    ADD COLUMN status public.reading_status,
    ADD COLUMN rating smallint,
    ADD COLUMN notes text,
    ADD COLUMN started_at timestamptz,
    ADD COLUMN finished_at timestamptz,
    ADD CONSTRAINT reading_state_rating_range
        CHECK (rating IS NULL OR (rating >= 1 AND rating <= 5)),
    ADD CONSTRAINT reading_state_notes_len
        CHECK (notes IS NULL OR char_length(notes) <= 10000),
    ADD CONSTRAINT reading_state_started_at_ts_decode_range
        CHECK (started_at >= '0001-01-01 00:00:00+00' AND started_at < '10000-01-01 00:00:00+00'),
    ADD CONSTRAINT reading_state_finished_at_ts_decode_range
        CHECK (finished_at >= '0001-01-01 00:00:00+00' AND finished_at < '10000-01-01 00:00:00+00');
