-- CASCADE on every DROP TABLE: manifestations and metadata_versions form a
-- circular FK pair, and RLS policies create cross-table dependencies, so no
-- ordering unwinds these without it.
DROP TABLE IF EXISTS public.api_cache CASCADE;
DROP TABLE IF EXISTS public.authors CASCADE;
DROP TABLE IF EXISTS public.device_tokens CASCADE;
DROP TABLE IF EXISTS public.field_locks CASCADE;
DROP TABLE IF EXISTS public.genres CASCADE;
DROP TABLE IF EXISTS public.identifier_schemes CASCADE;
DROP TABLE IF EXISTS public.ingestion_jobs CASCADE;
DROP TABLE IF EXISTS public.instance_bootstrap CASCADE;
DROP TABLE IF EXISTS public.local_credentials CASCADE;
DROP TABLE IF EXISTS public.local_login_throttle CASCADE;
DROP TABLE IF EXISTS public.manifestation_external_identifiers CASCADE;
DROP TABLE IF EXISTS public.manifestation_external_ratings CASCADE;
DROP TABLE IF EXISTS public.manifestation_genres CASCADE;
DROP TABLE IF EXISTS public.manifestation_moods CASCADE;
DROP TABLE IF EXISTS public.manifestation_tags CASCADE;
DROP TABLE IF EXISTS public.manifestations CASCADE;
DROP TABLE IF EXISTS public.metadata_sources CASCADE;
DROP TABLE IF EXISTS public.metadata_versions CASCADE;
DROP TABLE IF EXISTS public.moods CASCADE;
DROP TABLE IF EXISTS public.omnibus_contents CASCADE;
DROP TABLE IF EXISTS public.password_reset_pins CASCADE;
DROP TABLE IF EXISTS public.rating_sources CASCADE;
DROP TABLE IF EXISTS public.reading_sessions CASCADE;
DROP TABLE IF EXISTS public.reading_state CASCADE;
DROP TABLE IF EXISTS public.series CASCADE;
DROP TABLE IF EXISTS public.series_works CASCADE;
DROP TABLE IF EXISTS public.settings CASCADE;
DROP TABLE IF EXISTS public.shelf_items CASCADE;
DROP TABLE IF EXISTS public.shelves CASCADE;
DROP TABLE IF EXISTS public.tags CASCADE;
DROP TABLE IF EXISTS public.user_identities CASCADE;
DROP TABLE IF EXISTS public.user_preferences CASCADE;
DROP TABLE IF EXISTS public.users CASCADE;
DROP TABLE IF EXISTS public.webhook_deliveries CASCADE;
DROP TABLE IF EXISTS public.webhook_event_dedupe CASCADE;
DROP TABLE IF EXISTS public.webhooks CASCADE;
DROP TABLE IF EXISTS public.work_authors CASCADE;
DROP TABLE IF EXISTS public.work_external_identifiers CASCADE;
DROP TABLE IF EXISTS public.works CASCADE;
DROP TABLE IF EXISTS public.writeback_jobs CASCADE;
DROP TABLE IF EXISTS tower_sessions.session CASCADE;

DROP FUNCTION IF EXISTS public.notify_settings_changed();
DROP FUNCTION IF EXISTS public.works_search_vector_update();
DROP FUNCTION IF EXISTS public.set_updated_at();

-- Dropped before the unaccent extension the configuration's dictionary
-- belongs to, and before the wrappers the search indexes call.
DROP TEXT SEARCH CONFIGURATION IF EXISTS public.unaccent_english;
DROP FUNCTION IF EXISTS public.immutable_unaccent(text);
DROP FUNCTION IF EXISTS public.immutable_unaccent_like(text);

DROP SCHEMA IF EXISTS tower_sessions;

DROP TYPE IF EXISTS public.api_cache_kind;
DROP TYPE IF EXISTS public.author_role;
DROP TYPE IF EXISTS public.content_rating;
DROP TYPE IF EXISTS public.enrichment_status;
DROP TYPE IF EXISTS public.identity_provider;
DROP TYPE IF EXISTS public.ingestion_status;
DROP TYPE IF EXISTS public.job_status;
DROP TYPE IF EXISTS public.library_density;
DROP TYPE IF EXISTS public.library_view;
DROP TYPE IF EXISTS public.manifestation_format;
DROP TYPE IF EXISTS public.metadata_review_status;
DROP TYPE IF EXISTS public.reading_status;
DROP TYPE IF EXISTS public.scope;
DROP TYPE IF EXISTS public.theme_preference;
DROP TYPE IF EXISTS public.user_role;
DROP TYPE IF EXISTS public.validation_status;
DROP TYPE IF EXISTS public.writeback_status;

DROP EXTENSION IF EXISTS unaccent;
DROP EXTENSION IF EXISTS pgcrypto;
DROP EXTENSION IF EXISTS pg_trgm;

-- The roles outlive the schema, so their grants are withdrawn explicitly.
-- REVOKE on a role that was never granted is a no-op, but the roles
-- themselves are created outside the migrations and may be absent.
DO $$
BEGIN
    REVOKE SELECT ON TABLE public._sqlx_migrations FROM reverie_app;
    REVOKE USAGE ON SCHEMA public
    FROM reverie_app, reverie_ingestion, reverie_readonly;
EXCEPTION WHEN undefined_object THEN NULL;
END $$;
