-- Database role provisioning for Reverie.
-- Runs once when the PostgreSQL container is first created (uninitialized
-- data directory, no `PG_VERSION` file). The postgres entrypoint skips
-- /docker-entrypoint-initdb.d/* on subsequent restarts.
--
-- The script is DB-name-agnostic and password-env-driven so the same
-- script works for dev (POSTGRES_DB=reverie_dev, generated passwords)
-- and staging (POSTGRES_DB=reverie, deploy-supplied passwords). The
-- postgres entrypoint connects the init psql session to POSTGRES_DB and
-- exposes container env vars to the psql session, so:
--   * `current_database()` resolves to whichever DB POSTGRES_DB names.
--   * `\getenv` reads runtime env directly into psql variables (no shell
--     subprocess, so passwords containing `$`, backticks, or `$(...)`
--     are passed through verbatim instead of being silently expanded).
--   * Every role password must be supplied before any role is created.
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

\set ON_ERROR_STOP on
-- THREAT: Password-bearing provisioning statements must not be echoed.
\set ECHO none
\set ECHO_HIDDEN off
\set app_pw ''
\set ing_pw ''
\set ro_pw ''
\set mig_pw ''
\getenv app_pw REVERIE_APP_PASSWORD
\getenv ing_pw REVERIE_INGESTION_PASSWORD
\getenv ro_pw REVERIE_READONLY_PASSWORD
\getenv mig_pw REVERIE_MIGRATOR_PASSWORD
SELECT :'app_pw' <> '' AND :'ing_pw' <> '' AND :'ro_pw' <> '' AND :'mig_pw' <> '' AS passwords_complete
\gset
\if :passwords_complete
\else
  DO $$ BEGIN
    RAISE EXCEPTION 'role provisioning requires all four nonempty password inputs';
  END $$;
\endif

CREATE ROLE reverie_app       WITH LOGIN PASSWORD :'app_pw';
CREATE ROLE reverie_ingestion WITH LOGIN PASSWORD :'ing_pw';
CREATE ROLE reverie_readonly  WITH LOGIN PASSWORD :'ro_pw';
-- Dedicated migration identity. Explicitly NOSUPERUSER NOCREATEROLE
-- NOBYPASSRLS so a least-privilege audit can confirm the migrator holds no
-- cluster-wide authority — it owns only the schema objects it creates and
-- runs migrations under RLS like any other role.
CREATE ROLE reverie_migrator  WITH LOGIN PASSWORD :'mig_pw'
  NOSUPERUSER NOCREATEROLE NOBYPASSRLS;

-- scripts/schema-dump.sh replays this file from the DO block below to the end
-- against a scratch database, so per-database statements belong inside or
-- after that block and cluster-wide ones above it.
--
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
