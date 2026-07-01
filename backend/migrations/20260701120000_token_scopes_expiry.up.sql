-- Token scope + expiry storage (S4 of the auth/authz consolidation arc).
--
-- Type + column seam only: converts the column-only `device_tokens.scopes
-- text[]` placeholder (added in S1) to a Postgres-enum-backed `scope[]`, and
-- adds an optional `expires_at`. No Rust struct/query reads either yet -- this
-- commit exists solely so the migration can be applied ahead of the query
-- code that reads it (sqlx same-commit migration+query deadlock, see
-- CLAUDE.local.md). See adr/2026-06-23-api-authorization-orthogonal-axes.md.

-- Credential capability, orthogonal to role. Mirrors the `user_role` /
-- `identity_provider` PG-enum convention so an invalid scope is
-- unrepresentable at the DB, not just the app boundary.
CREATE TYPE public.scope AS ENUM ('read', 'write', 'admin');

-- Enum-rebuild dance (DROP DEFAULT before ALTER COLUMN TYPE, SET DEFAULT
-- after -- backend/CLAUDE.md). A direct `text[]::scope[]` cast is not
-- guaranteed to be registered for enum arrays, so the USING clause goes
-- through unnest+cast+array_agg, which is total over any text[] holding
-- valid label text (existing rows are all `{read}`, the column default).
ALTER TABLE public.device_tokens ALTER COLUMN scopes DROP DEFAULT;
ALTER TABLE public.device_tokens
    ALTER COLUMN scopes
    TYPE public.scope[]
    USING (SELECT array_agg(x::public.scope) FROM unnest(scopes) AS x);
ALTER TABLE public.device_tokens ALTER COLUMN scopes SET DEFAULT '{read}'::public.scope[];

-- Optional token expiry. NULL = never expires, preserving current behaviour
-- for tokens minted before this column existed.
ALTER TABLE public.device_tokens ADD COLUMN expires_at timestamptz;

ALTER TABLE public.device_tokens
    ADD CONSTRAINT device_tokens_expires_at_ts_decode_range
        CHECK (expires_at >= '0001-01-01 00:00:00+00' AND expires_at < '10000-01-01 00:00:00+00');
