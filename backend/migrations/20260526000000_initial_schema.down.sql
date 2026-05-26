-- Reverse of 20260526000000_initial_schema.up.sql
-- Drops everything created by the consolidated schema.
-- CASCADE on all DROP TABLE: manifestations ↔ metadata_versions is a circular
-- FK pair, and RLS policies create cross-table dependencies (e.g.
-- manifestations_select_child references shelf_items). No sequential ordering
-- can unwind these without CASCADE.

-- 1. Triggers (before dropping functions they reference)
DROP TRIGGER IF EXISTS settings_changed_trigger ON public.settings;
DROP TRIGGER IF EXISTS shelves_set_updated_at ON public.shelves;
DROP TRIGGER IF EXISTS trg_reading_state_updated_at ON public.reading_state;
DROP TRIGGER IF EXISTS trg_works_search_vector ON public.works;
DROP TRIGGER IF EXISTS trg_manifestations_updated_at ON public.manifestations;
DROP TRIGGER IF EXISTS trg_works_updated_at ON public.works;
DROP TRIGGER IF EXISTS trg_users_updated_at ON public.users;

-- 2. Functions
DROP FUNCTION IF EXISTS public.notify_settings_changed();
DROP FUNCTION IF EXISTS public.works_search_vector_update();
DROP FUNCTION IF EXISTS public.set_updated_at();

-- 3. Tables (CASCADE required for circular FKs and RLS cross-refs)
DROP TABLE IF EXISTS public.settings CASCADE;
DROP TABLE IF EXISTS tower_sessions.session CASCADE;
DROP TABLE IF EXISTS public.writeback_jobs CASCADE;
DROP TABLE IF EXISTS public.reading_state CASCADE;
DROP TABLE IF EXISTS public.reading_sessions CASCADE;
DROP TABLE IF EXISTS public.webhook_deliveries CASCADE;
DROP TABLE IF EXISTS public.webhooks CASCADE;
DROP TABLE IF EXISTS public.ingestion_jobs CASCADE;
DROP TABLE IF EXISTS public.api_cache CASCADE;
DROP TABLE IF EXISTS public.device_tokens CASCADE;
DROP TABLE IF EXISTS public.shelf_items CASCADE;
DROP TABLE IF EXISTS public.shelves CASCADE;
DROP TABLE IF EXISTS public.field_locks CASCADE;
DROP TABLE IF EXISTS public.manifestation_tags CASCADE;
DROP TABLE IF EXISTS public.tags CASCADE;
DROP TABLE IF EXISTS public.omnibus_contents CASCADE;
DROP TABLE IF EXISTS public.series_works CASCADE;
DROP TABLE IF EXISTS public.work_authors CASCADE;
DROP TABLE IF EXISTS public.metadata_versions CASCADE;
DROP TABLE IF EXISTS public.manifestations CASCADE;
DROP TABLE IF EXISTS public.series CASCADE;
DROP TABLE IF EXISTS public.metadata_sources CASCADE;
DROP TABLE IF EXISTS public.authors CASCADE;
DROP TABLE IF EXISTS public.works CASCADE;
DROP TABLE IF EXISTS public.users CASCADE;

-- 4. Schemas
DO $$ BEGIN
    REVOKE USAGE ON SCHEMA tower_sessions FROM reverie_app, reverie_readonly;
EXCEPTION WHEN undefined_object THEN NULL;
END $$;
DROP SCHEMA IF EXISTS tower_sessions;

-- 5. Enum types
DROP TYPE IF EXISTS public.theme_preference;
DROP TYPE IF EXISTS public.writeback_status;
DROP TYPE IF EXISTS public.api_cache_kind;
DROP TYPE IF EXISTS public.enrichment_status;
DROP TYPE IF EXISTS public.job_status;
DROP TYPE IF EXISTS public.tag_type;
DROP TYPE IF EXISTS public.metadata_review_status;
DROP TYPE IF EXISTS public.ingestion_status;
DROP TYPE IF EXISTS public.validation_status;
DROP TYPE IF EXISTS public.manifestation_format;
DROP TYPE IF EXISTS public.author_role;
DROP TYPE IF EXISTS public.user_role;

-- 6. Extensions
DROP EXTENSION IF EXISTS "pgcrypto";
DROP EXTENSION IF EXISTS "pg_trgm";

-- 7. Revoke schema-level grants
DO $$ BEGIN
    REVOKE USAGE ON SCHEMA public FROM reverie_app, reverie_ingestion, reverie_readonly;
EXCEPTION WHEN undefined_object THEN NULL;
END $$;
