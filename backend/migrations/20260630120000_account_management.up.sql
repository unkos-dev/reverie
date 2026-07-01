-- Account management: soft-disable column on users (S3 of the auth/authz
-- consolidation arc).
--
-- Adds a nullable `disabled_at` to back soft-disable: an admin disables an
-- account without hard-deleting it, and every auth-resolution path rejects a
-- user whose `disabled_at` is set. NULL means active; a timestamp means the
-- account was disabled at that instant. Soft-disable is reversible (clear the
-- column to re-enable) and preserves the row's referential history.
--
-- No Rust query code ships in this commit (column only): the query path and the
-- regenerated .sqlx cache land after this migration is applied, to avoid the
-- same-commit migration+query deadlock.
-- See adr/2026-06-23-auth-identity-pluggable-providers.md.

ALTER TABLE public.users
    ADD COLUMN disabled_at timestamptz,
    ADD CONSTRAINT users_disabled_at_ts_decode_range
        CHECK (disabled_at >= '0001-01-01 00:00:00+00' AND disabled_at < '10000-01-01 00:00:00+00');
