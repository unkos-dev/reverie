-- Database role provisioning for Reverie.
-- Runs once when the PostgreSQL container is first created (uninitialized
-- data directory, no `PG_VERSION` file). The postgres entrypoint skips
-- /docker-entrypoint-initdb.d/* on subsequent restarts.
--
-- The script is DB-name-agnostic and password-env-driven so the same
-- script works for dev (POSTGRES_DB=reverie_dev, default trivial passwords)
-- and staging (POSTGRES_DB=reverie, deploy-supplied passwords). The
-- postgres entrypoint connects the init psql session to POSTGRES_DB and
-- exposes container env vars to the psql session, so:
--   * `current_database()` resolves to whichever DB POSTGRES_DB names.
--   * `\getenv` reads runtime env directly into psql variables (no shell
--     subprocess, so passwords containing `$`, backticks, or `$(...)`
--     are passed through verbatim instead of being silently expanded).
--   * Empty/unset env values fall back to the role name (dev default),
--     resolved via a server-side SELECT + `\gset`.
--
-- Role architecture:
--   reverie           — cluster bootstrap superuser (created by POSTGRES_USER).
--                    Provisions roles here; NOT the migration identity. Never
--                    used by the application at runtime.
--   reverie_migrator  — dedicated least-privilege migration identity
--                    (NOSUPERUSER NOCREATEROLE NOBYPASSRLS). Runs `reverie
--                    migrate`; owns the schema objects it creates. See
--                    docs/adr/0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md.
--   reverie_app       — web application service account. RLS enforced (user-scoped).
--   reverie_ingestion — background pipeline service account. Has own permissive
--                    RLS policy on manifestations. Scoped to pipeline tables.
--   reverie_readonly  — debugging and reporting. SELECT only. RLS enforced.

-- Read passwords from container env directly (no shell). `\getenv` only
-- sets the target psql variable when the env var exists, so we
-- pre-initialise to the empty string to guarantee `:'name'` substitution
-- below resolves to a literal even when the deploy doesn't supply the
-- var (dev workflow).
\set app_password ''
\set ingestion_password ''
\set readonly_password ''
\set migrator_password ''
\getenv app_password REVERIE_APP_PASSWORD
\getenv ingestion_password REVERIE_INGESTION_PASSWORD
\getenv readonly_password REVERIE_READONLY_PASSWORD
\getenv migrator_password REVERIE_MIGRATOR_PASSWORD

-- Resolve dev fallbacks server-side. NULLIF strips empty strings,
-- COALESCE substitutes the role name. `\gset` captures the resolved
-- values back into psql variables for safe quoting in CREATE ROLE
-- below. Passwords containing SQL metacharacters (single quotes,
-- backslashes) are correctly escaped by `:'name'` substitution.
SELECT
  COALESCE(NULLIF(:'app_password', ''), 'reverie_app')             AS app_pw,
  COALESCE(NULLIF(:'ingestion_password', ''), 'reverie_ingestion') AS ing_pw,
  COALESCE(NULLIF(:'readonly_password', ''), 'reverie_readonly')   AS ro_pw,
  COALESCE(NULLIF(:'migrator_password', ''), 'reverie_migrator')   AS mig_pw
\gset

CREATE ROLE reverie_app       WITH LOGIN PASSWORD :'app_pw';
CREATE ROLE reverie_ingestion WITH LOGIN PASSWORD :'ing_pw';
CREATE ROLE reverie_readonly  WITH LOGIN PASSWORD :'ro_pw';
-- Dedicated migration identity. Explicitly NOSUPERUSER NOCREATEROLE
-- NOBYPASSRLS so a least-privilege audit can confirm the migrator holds no
-- cluster-wide authority — it owns only the schema objects it creates and
-- runs migrations under RLS like any other role.
CREATE ROLE reverie_migrator  WITH LOGIN PASSWORD :'mig_pw'
  NOSUPERUSER NOCREATEROLE NOBYPASSRLS;

-- CONNECT grants are kept explicit so they remain load-bearing if a
-- future migration ever issues `REVOKE CONNECT ON DATABASE … FROM PUBLIC`
-- (a common hardening step that would otherwise lock the runtime roles
-- out). `current_database()` adapts to whichever DB POSTGRES_DB names.
DO $$
DECLARE
  db text := current_database();
BEGIN
  EXECUTE format('GRANT CONNECT ON DATABASE %I TO reverie_app', db);
  EXECUTE format('GRANT CONNECT ON DATABASE %I TO reverie_ingestion', db);
  EXECUTE format('GRANT CONNECT ON DATABASE %I TO reverie_readonly', db);
  EXECUTE format('GRANT CONNECT ON DATABASE %I TO reverie_migrator', db);
  -- Database-level CREATE is REQUIRED and is NOT redundant with the
  -- schema-level CREATE granted below: the initial migration runs
  -- `CREATE SCHEMA IF NOT EXISTS tower_sessions`, and creating a *schema*
  -- needs database CREATE. `CREATE ON SCHEMA public` only authorises objects
  -- *within* public. A least-privilege audit must keep both.
  EXECUTE format('GRANT CREATE ON DATABASE %I TO reverie_migrator', db);
END $$;

-- Schema-level grants for the migrator. PG15+ removed the implicit CREATE on
-- schema public from PUBLIC, so `CREATE EXTENSION ... WITH SCHEMA public` and
-- `CREATE TABLE` in public both REQUIRE an explicit CREATE here — database
-- CREATE alone is insufficient. The public schema exists at init time.
GRANT USAGE, CREATE ON SCHEMA public TO reverie_migrator;
