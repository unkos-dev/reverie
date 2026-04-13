-- Database role provisioning for Tome.
-- This script runs once when the PostgreSQL container is first created
-- (empty pgdata volume). It creates the application roles that sqlx
-- migrations will grant privileges to.
--
-- Role architecture:
--   tome           — schema owner (created by POSTGRES_USER). Runs migrations.
--                    Bypasses RLS. Never used by the application at runtime.
--   tome_app       — web application service account. RLS enforced (user-scoped).
--   tome_ingestion — background pipeline service account. Has own permissive
--                    RLS policy on manifestations. Scoped to pipeline tables.
--   tome_readonly  — debugging and reporting. SELECT only. RLS enforced.

-- Web application service account
CREATE ROLE tome_app WITH LOGIN PASSWORD 'tome_app';
GRANT CONNECT ON DATABASE tome_dev TO tome_app;

-- Background ingestion pipeline service account
CREATE ROLE tome_ingestion WITH LOGIN PASSWORD 'tome_ingestion';
GRANT CONNECT ON DATABASE tome_dev TO tome_ingestion;

-- Read-only account for debugging and reporting
CREATE ROLE tome_readonly WITH LOGIN PASSWORD 'tome_readonly';
GRANT CONNECT ON DATABASE tome_dev TO tome_readonly;
