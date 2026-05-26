-- Reverse of 20260526000000_initial_schema.up.sql
-- Drops everything created by the consolidated schema.

-- 1. Triggers (before dropping functions they reference)
DROP TRIGGER IF EXISTS settings_changed_trigger ON settings;
DROP TRIGGER IF EXISTS shelves_set_updated_at ON shelves;
DROP TRIGGER IF EXISTS trg_reading_state_updated_at ON reading_state;
DROP TRIGGER IF EXISTS trg_works_search_vector ON works;
DROP TRIGGER IF EXISTS trg_manifestations_updated_at ON manifestations;
DROP TRIGGER IF EXISTS trg_works_updated_at ON works;
DROP TRIGGER IF EXISTS trg_users_updated_at ON users;

-- 2. Functions
DROP FUNCTION IF EXISTS notify_settings_changed();
DROP FUNCTION IF EXISTS works_search_vector_update();
DROP FUNCTION IF EXISTS set_updated_at();

-- 3. Tables (FK-reverse order)
DROP TABLE IF EXISTS settings;
DROP TABLE IF EXISTS tower_sessions.session;
DROP TABLE IF EXISTS writeback_jobs;
DROP TABLE IF EXISTS reading_state;
DROP TABLE IF EXISTS reading_sessions;
DROP TABLE IF EXISTS webhook_deliveries;
DROP TABLE IF EXISTS webhooks;
DROP TABLE IF EXISTS ingestion_jobs;
DROP TABLE IF EXISTS api_cache;
DROP TABLE IF EXISTS device_tokens;
DROP TABLE IF EXISTS shelf_items;
DROP TABLE IF EXISTS shelves;
DROP TABLE IF EXISTS field_locks;
DROP TABLE IF EXISTS manifestation_tags;
DROP TABLE IF EXISTS tags;
DROP TABLE IF EXISTS metadata_versions;
DROP TABLE IF EXISTS metadata_sources;
DROP TABLE IF EXISTS omnibus_contents;
DROP TABLE IF EXISTS series_works;
DROP TABLE IF EXISTS series;
DROP TABLE IF EXISTS manifestations;
DROP TABLE IF EXISTS work_authors;
DROP TABLE IF EXISTS authors;
DROP TABLE IF EXISTS works;
DROP TABLE IF EXISTS users;

-- 4. Schemas
REVOKE USAGE ON SCHEMA tower_sessions FROM reverie_app, reverie_readonly;
DROP SCHEMA IF EXISTS tower_sessions;

-- 5. Enum types
DROP TYPE IF EXISTS theme_preference;
DROP TYPE IF EXISTS writeback_status;
DROP TYPE IF EXISTS api_cache_kind;
DROP TYPE IF EXISTS enrichment_status;
DROP TYPE IF EXISTS job_status;
DROP TYPE IF EXISTS tag_type;
DROP TYPE IF EXISTS metadata_review_status;
DROP TYPE IF EXISTS ingestion_status;
DROP TYPE IF EXISTS validation_status;
DROP TYPE IF EXISTS manifestation_format;
DROP TYPE IF EXISTS author_role;
DROP TYPE IF EXISTS user_role;

-- 6. Extensions
DROP EXTENSION IF EXISTS "pgcrypto";
DROP EXTENSION IF EXISTS "pg_trgm";

-- 7. Revoke schema-level grants
REVOKE USAGE ON SCHEMA public FROM reverie_app, reverie_ingestion, reverie_readonly;
