-- Add `updated_at` to `shelves` to back the ETag/If-Match
-- optimistic-concurrency contract used by the shelves CRUD + items
-- reorder endpoints (sub-phase 11d).
--
-- Default backfills existing rows to `now()` so the ETag is well-
-- defined immediately after deploy; thereafter the trigger keeps it
-- in step on every UPDATE. The shelf items endpoints (`POST /items`,
-- `DELETE /items/{mid}`, `PUT /items`) explicitly issue
-- `UPDATE shelves SET updated_at = now() WHERE id = $1` in the same
-- transaction so item-mutation events also bump the ETag — without
-- that, an items add/remove would not move the ETag and the next
-- reorder PUT's `If-Match` would be a stale-window false positive.
ALTER TABLE shelves
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE OR REPLACE FUNCTION set_shelves_updated_at() RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER shelves_set_updated_at
    BEFORE UPDATE ON shelves
    FOR EACH ROW
    EXECUTE FUNCTION set_shelves_updated_at();
