-- Token scope and expiry storage for device tokens.
--
-- Converts the placeholder `device_tokens.scopes text[]` column to a
-- Postgres-enum-backed `scope[]`, and adds an optional `expires_at`.
-- See adr/2026-06-23-api-authorization-orthogonal-axes.md.

-- Credential capability, orthogonal to role. Mirrors the `user_role` /
-- `identity_provider` PG-enum convention so an invalid scope is
-- unrepresentable at the DB, not just the app boundary.
CREATE TYPE public.scope AS ENUM ('read', 'write', 'admin');

-- Enum-rebuild dance (DROP DEFAULT before ALTER COLUMN TYPE, SET DEFAULT
-- after -- backend/CLAUDE.md). The double cast applies the text->scope
-- element cast across the array (ArrayCoerceExpr); a subquery-based USING
-- (e.g. unnest+array_agg) is rejected by Postgres as an invalid transform
-- expression for ALTER COLUMN TYPE. Existing rows are all `{read}`, so the
-- cast is total over the enum's label set.
ALTER TABLE public.device_tokens ALTER COLUMN scopes DROP DEFAULT;
ALTER TABLE public.device_tokens
    ALTER COLUMN scopes
    TYPE public.scope[]
    USING scopes::text[]::public.scope[];
ALTER TABLE public.device_tokens ALTER COLUMN scopes SET DEFAULT '{read}'::public.scope[];

-- Optional token expiry. NULL = never expires, preserving current behaviour
-- for tokens minted before this column existed.
ALTER TABLE public.device_tokens ADD COLUMN expires_at timestamptz;

ALTER TABLE public.device_tokens
    ADD CONSTRAINT device_tokens_expires_at_ts_decode_range
        CHECK (expires_at >= '0001-01-01 00:00:00+00' AND expires_at < '10000-01-01 00:00:00+00');
