-- Reverse of the token scope + expiry migration, in inverse order.
--
-- Local-dev reversibility only, not a production rollback (matches the S1
-- identity-schema down migration's caveat): this targets a clean-ish dev DB.

ALTER TABLE public.device_tokens DROP CONSTRAINT device_tokens_expires_at_ts_decode_range;
ALTER TABLE public.device_tokens DROP COLUMN expires_at;

-- Revert scopes off the `scope` type before DROP TYPE, or the drop fails on
-- the column's dependency. Direct cast, no subquery (see up.sql).
ALTER TABLE public.device_tokens ALTER COLUMN scopes DROP DEFAULT;
ALTER TABLE public.device_tokens
    ALTER COLUMN scopes
    TYPE text[]
    USING scopes::text[];
ALTER TABLE public.device_tokens ALTER COLUMN scopes SET DEFAULT '{read}';

DROP TYPE public.scope;
