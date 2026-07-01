-- Reverse of the account-management soft-disable migration. Local-dev
-- reversibility only, not a production rollback; these migrations roll up into
-- the base schema before the first release.

ALTER TABLE public.users
    DROP CONSTRAINT users_disabled_at_ts_decode_range,
    DROP COLUMN disabled_at;
