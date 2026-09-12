--
-- PostgreSQL database dump
--

\restrict reverie


SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: tower_sessions; Type: SCHEMA; Schema: -; Owner: reverie_migrator
--

CREATE SCHEMA tower_sessions;


ALTER SCHEMA tower_sessions OWNER TO reverie_migrator;

--
-- Name: pg_trgm; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;


--
-- Name: EXTENSION pg_trgm; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION pg_trgm IS 'text similarity measurement and index searching based on trigrams';


--
-- Name: pgcrypto; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;


--
-- Name: EXTENSION pgcrypto; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION pgcrypto IS 'cryptographic functions';


--
-- Name: unaccent; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA public;


--
-- Name: EXTENSION unaccent; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION unaccent IS 'text search dictionary that removes accents';


--
-- Name: api_cache_kind; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.api_cache_kind AS ENUM (
    'hit',
    'miss',
    'error'
);


ALTER TYPE public.api_cache_kind OWNER TO reverie_migrator;

--
-- Name: author_role; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.author_role AS ENUM (
    'author',
    'editor',
    'translator',
    'narrator'
);


ALTER TYPE public.author_role OWNER TO reverie_migrator;

--
-- Name: content_rating; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.content_rating AS ENUM (
    'everyone',
    'teen',
    'mature',
    'adult',
    'explicit'
);


ALTER TYPE public.content_rating OWNER TO reverie_migrator;

--
-- Name: enrichment_status; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.enrichment_status AS ENUM (
    'pending',
    'in_progress',
    'complete',
    'failed',
    'skipped'
);


ALTER TYPE public.enrichment_status OWNER TO reverie_migrator;

--
-- Name: identity_provider; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.identity_provider AS ENUM (
    'oidc'
);


ALTER TYPE public.identity_provider OWNER TO reverie_migrator;

--
-- Name: ingestion_status; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.ingestion_status AS ENUM (
    'pending',
    'processing',
    'complete',
    'failed',
    'skipped'
);


ALTER TYPE public.ingestion_status OWNER TO reverie_migrator;

--
-- Name: job_status; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.job_status AS ENUM (
    'queued',
    'running',
    'complete',
    'failed',
    'skipped'
);


ALTER TYPE public.job_status OWNER TO reverie_migrator;

--
-- Name: library_density; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.library_density AS ENUM (
    'comfortable',
    'compact'
);


ALTER TYPE public.library_density OWNER TO reverie_migrator;

--
-- Name: library_view; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.library_view AS ENUM (
    'grid',
    'table'
);


ALTER TYPE public.library_view OWNER TO reverie_migrator;

--
-- Name: manifestation_format; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.manifestation_format AS ENUM (
    'epub',
    'pdf',
    'mobi',
    'azw3',
    'cbz',
    'cbr'
);


ALTER TYPE public.manifestation_format OWNER TO reverie_migrator;

--
-- Name: metadata_review_status; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.metadata_review_status AS ENUM (
    'pending',
    'rejected'
);


ALTER TYPE public.metadata_review_status OWNER TO reverie_migrator;

--
-- Name: reading_status; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.reading_status AS ENUM (
    'want_to_read',
    'reading',
    'on_hold',
    'finished',
    'abandoned'
);


ALTER TYPE public.reading_status OWNER TO reverie_migrator;

--
-- Name: scope; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.scope AS ENUM (
    'read',
    'write',
    'admin'
);


ALTER TYPE public.scope OWNER TO reverie_migrator;

--
-- Name: theme_preference; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.theme_preference AS ENUM (
    'system',
    'light',
    'dark'
);


ALTER TYPE public.theme_preference OWNER TO reverie_migrator;

--
-- Name: user_role; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.user_role AS ENUM (
    'admin',
    'adult',
    'child'
);


ALTER TYPE public.user_role OWNER TO reverie_migrator;

--
-- Name: validation_status; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.validation_status AS ENUM (
    'pending',
    'clean',
    'repaired',
    'degraded',
    'failed'
);


ALTER TYPE public.validation_status OWNER TO reverie_migrator;

--
-- Name: writeback_status; Type: TYPE; Schema: public; Owner: reverie_migrator
--

CREATE TYPE public.writeback_status AS ENUM (
    'pending',
    'in_progress',
    'complete',
    'failed',
    'skipped'
);


ALTER TYPE public.writeback_status OWNER TO reverie_migrator;

--
-- Name: immutable_unaccent(text); Type: FUNCTION; Schema: public; Owner: reverie_migrator
--

CREATE FUNCTION public.immutable_unaccent(text) RETURNS text
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    AS $_$
        SELECT public.unaccent('public.unaccent', $1)
    $_$;


ALTER FUNCTION public.immutable_unaccent(text) OWNER TO reverie_migrator;

--
-- Name: immutable_unaccent_like(text); Type: FUNCTION; Schema: public; Owner: reverie_migrator
--

CREATE FUNCTION public.immutable_unaccent_like(text) RETURNS text
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    AS $_$
        SELECT regexp_replace(public.immutable_unaccent($1), '([\\%_])', '\\\1', 'g')
    $_$;


ALTER FUNCTION public.immutable_unaccent_like(text) OWNER TO reverie_migrator;

--
-- Name: notify_settings_changed(); Type: FUNCTION; Schema: public; Owner: reverie_migrator
--

CREATE FUNCTION public.notify_settings_changed() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    PERFORM pg_notify('settings_changed', '');
    RETURN NEW;
END;
$$;


ALTER FUNCTION public.notify_settings_changed() OWNER TO reverie_migrator;

--
-- Name: set_updated_at(); Type: FUNCTION; Schema: public; Owner: reverie_migrator
--

CREATE FUNCTION public.set_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;


ALTER FUNCTION public.set_updated_at() OWNER TO reverie_migrator;

--
-- Name: works_search_vector_update(); Type: FUNCTION; Schema: public; Owner: reverie_migrator
--

CREATE FUNCTION public.works_search_vector_update() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    NEW.search_vector := to_tsvector('public.unaccent_english', COALESCE(NEW.title, '') || ' ' || COALESCE(NEW.description, ''));
    RETURN NEW;
END;
$$;


ALTER FUNCTION public.works_search_vector_update() OWNER TO reverie_migrator;

--
-- Name: unaccent_english; Type: TEXT SEARCH CONFIGURATION; Schema: public; Owner: reverie_migrator
--

CREATE TEXT SEARCH CONFIGURATION public.unaccent_english (
    PARSER = pg_catalog."default" );

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR asciiword WITH english_stem;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR word WITH public.unaccent, english_stem;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR numword WITH public.unaccent, simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR email WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR url WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR host WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR sfloat WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR version WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR hword_numpart WITH public.unaccent, simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR hword_part WITH public.unaccent, english_stem;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR hword_asciipart WITH english_stem;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR numhword WITH public.unaccent, simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR asciihword WITH english_stem;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR hword WITH public.unaccent, english_stem;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR url_path WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR file WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR "float" WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR "int" WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ADD MAPPING FOR uint WITH simple;


ALTER TEXT SEARCH CONFIGURATION public.unaccent_english OWNER TO reverie_migrator;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: _sqlx_migrations; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public._sqlx_migrations (
    version bigint NOT NULL,
    description text NOT NULL,
    installed_on timestamp with time zone DEFAULT now() NOT NULL,
    success boolean NOT NULL,
    checksum bytea NOT NULL,
    execution_time bigint NOT NULL
);


ALTER TABLE public._sqlx_migrations OWNER TO reverie_migrator;

--
-- Name: api_cache; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.api_cache (
    id uuid DEFAULT uuidv7() NOT NULL,
    source text NOT NULL,
    lookup_key text NOT NULL,
    response jsonb NOT NULL,
    fetched_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    response_kind public.api_cache_kind DEFAULT 'hit'::public.api_cache_kind NOT NULL,
    http_status integer,
    CONSTRAINT api_cache_expires_at_ts_decode_range CHECK (((expires_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (expires_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT api_cache_fetched_at_ts_decode_range CHECK (((fetched_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (fetched_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.api_cache OWNER TO reverie_migrator;

--
-- Name: authors; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.authors (
    id uuid DEFAULT uuidv7() NOT NULL,
    name text NOT NULL,
    sort_name text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT authors_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.authors OWNER TO reverie_migrator;

--
-- Name: device_tokens; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.device_tokens (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    token_hash text NOT NULL,
    last_used_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_at timestamp with time zone,
    scopes public.scope[] DEFAULT '{read}'::public.scope[] NOT NULL,
    expires_at timestamp with time zone,
    CONSTRAINT device_tokens_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT device_tokens_expires_at_ts_decode_range CHECK (((expires_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (expires_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT device_tokens_last_used_at_ts_decode_range CHECK (((last_used_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (last_used_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT device_tokens_revoked_at_ts_decode_range CHECK (((revoked_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (revoked_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.device_tokens OWNER TO reverie_migrator;

--
-- Name: TABLE device_tokens; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.device_tokens IS 'No RLS. Ownership enforced at application layer — all queries scope by user_id.';


--
-- Name: field_locks; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.field_locks (
    manifestation_id uuid NOT NULL,
    entity_type text NOT NULL,
    field_name text NOT NULL,
    locked_at timestamp with time zone DEFAULT now() NOT NULL,
    locked_by uuid,
    CONSTRAINT field_locks_locked_at_ts_decode_range CHECK (((locked_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (locked_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.field_locks OWNER TO reverie_migrator;

--
-- Name: genres; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.genres (
    id uuid DEFAULT uuidv7() NOT NULL,
    name text NOT NULL
);


ALTER TABLE public.genres OWNER TO reverie_migrator;

--
-- Name: identifier_schemes; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.identifier_schemes (
    id text NOT NULL,
    display_name text NOT NULL
);


ALTER TABLE public.identifier_schemes OWNER TO reverie_migrator;

--
-- Name: ingestion_jobs; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.ingestion_jobs (
    id uuid DEFAULT uuidv7() NOT NULL,
    batch_id uuid NOT NULL,
    source_path text NOT NULL,
    status public.job_status DEFAULT 'queued'::public.job_status NOT NULL,
    error_message text,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT ingestion_jobs_completed_at_ts_decode_range CHECK (((completed_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (completed_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT ingestion_jobs_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT ingestion_jobs_started_at_ts_decode_range CHECK (((started_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (started_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.ingestion_jobs OWNER TO reverie_migrator;

--
-- Name: instance_bootstrap; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.instance_bootstrap (
    id boolean DEFAULT true NOT NULL,
    bootstrapped_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT instance_bootstrap_bootstrapped_at_ts_decode_range CHECK (((bootstrapped_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (bootstrapped_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT instance_bootstrap_singleton CHECK (id)
);


ALTER TABLE public.instance_bootstrap OWNER TO reverie_migrator;

--
-- Name: local_credentials; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.local_credentials (
    user_id uuid NOT NULL,
    password_hash text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT local_credentials_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT local_credentials_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.local_credentials OWNER TO reverie_migrator;

--
-- Name: local_login_throttle; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.local_login_throttle (
    email_lower text NOT NULL,
    fail_count integer DEFAULT 0 NOT NULL,
    locked_until timestamp with time zone,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT local_login_throttle_locked_until_ts_decode_range CHECK (((locked_until >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (locked_until < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT local_login_throttle_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.local_login_throttle OWNER TO reverie_migrator;

--
-- Name: manifestation_external_identifiers; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.manifestation_external_identifiers (
    manifestation_id uuid NOT NULL,
    scheme text NOT NULL,
    external_id text NOT NULL,
    source_version_id uuid,
    CONSTRAINT manifestation_external_identifiers_external_id_check CHECK ((external_id ~ '^[A-Za-z0-9._-]{1,255}$'::text))
);


ALTER TABLE public.manifestation_external_identifiers OWNER TO reverie_migrator;

--
-- Name: manifestation_external_ratings; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.manifestation_external_ratings (
    manifestation_id uuid NOT NULL,
    source text NOT NULL,
    rating real NOT NULL,
    rating_scale real NOT NULL,
    review_count integer DEFAULT 0 NOT NULL,
    fetched_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT manifestation_external_ratings_fetched_at_ts_decode_range CHECK (((fetched_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (fetched_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT manifestation_external_ratings_rating_range CHECK (((rating >= (0)::double precision) AND (rating <= rating_scale))),
    CONSTRAINT manifestation_external_ratings_rating_scale_positive CHECK ((rating_scale > (0)::double precision)),
    CONSTRAINT manifestation_external_ratings_review_count_nonneg CHECK ((review_count >= 0))
);


ALTER TABLE public.manifestation_external_ratings OWNER TO reverie_migrator;

--
-- Name: manifestation_genres; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.manifestation_genres (
    manifestation_id uuid NOT NULL,
    genre_id uuid NOT NULL,
    source_version_id uuid
);


ALTER TABLE public.manifestation_genres OWNER TO reverie_migrator;

--
-- Name: manifestation_moods; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.manifestation_moods (
    manifestation_id uuid NOT NULL,
    mood_id uuid NOT NULL,
    source_version_id uuid
);


ALTER TABLE public.manifestation_moods OWNER TO reverie_migrator;

--
-- Name: manifestation_tags; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.manifestation_tags (
    manifestation_id uuid NOT NULL,
    tag_id uuid NOT NULL,
    source_version_id uuid
);


ALTER TABLE public.manifestation_tags OWNER TO reverie_migrator;

--
-- Name: manifestations; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.manifestations (
    id uuid DEFAULT uuidv7() NOT NULL,
    work_id uuid NOT NULL,
    isbn_10 text,
    isbn_13 text,
    publisher text,
    pub_date date,
    format public.manifestation_format NOT NULL,
    file_path text NOT NULL,
    ingestion_file_hash text CONSTRAINT manifestations_file_hash_not_null NOT NULL,
    file_size_bytes bigint NOT NULL,
    validation_status public.validation_status DEFAULT 'pending'::public.validation_status NOT NULL,
    ingestion_status public.ingestion_status DEFAULT 'pending'::public.ingestion_status NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    accessibility_metadata jsonb,
    publisher_version_id uuid,
    pub_date_version_id uuid,
    isbn_10_version_id uuid,
    isbn_13_version_id uuid,
    cover_path text,
    cover_sha256 bytea,
    cover_size_bytes bigint,
    cover_source text,
    cover_version_id uuid,
    suspected_duplicate_work_id uuid,
    enrichment_status public.enrichment_status DEFAULT 'pending'::public.enrichment_status NOT NULL,
    enrichment_attempted_at timestamp with time zone,
    enrichment_attempt_count integer DEFAULT 0 NOT NULL,
    enrichment_error text,
    current_file_hash text NOT NULL,
    pages integer,
    pages_version_id uuid,
    content_rating public.content_rating,
    content_rating_version_id uuid,
    enrichment_rerun_requested boolean DEFAULT false NOT NULL,
    has_embedded_cover boolean,
    CONSTRAINT manifestations_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT manifestations_enrichment_attempted_at_ts_decode_range CHECK (((enrichment_attempted_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (enrichment_attempted_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT manifestations_pages_positive CHECK (((pages IS NULL) OR (pages > 0))),
    CONSTRAINT manifestations_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.manifestations OWNER TO reverie_migrator;

--
-- Name: TABLE manifestations; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.manifestations IS 'RLS enabled. reverie_app and reverie_readonly must call set_config(''app.current_user_id'', ..., true) (transaction-local) before queries — see db::acquire_with_rls. The writeback worker connects via a dedicated pool that sets app.system_context = ''writeback'' (session-scoped via after_connect). reverie_ingestion has unconditional access. reverie (owner) bypasses RLS.';


--
-- Name: COLUMN manifestations.ingestion_file_hash; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON COLUMN public.manifestations.ingestion_file_hash IS 'SHA-256 of file at ingestion time. App-layer invariant: never updated after initial insert — audit trail. No schema constraint enforces this; see services::writeback::orchestrator.';


--
-- Name: COLUMN manifestations.current_file_hash; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON COLUMN public.manifestations.current_file_hash IS 'SHA-256 of file as of last successful writeback. Equals ingestion_file_hash until first writeback. Step 11 health surfaces divergence from on-disk hash.';


--
-- Name: metadata_sources; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.metadata_sources (
    id text NOT NULL,
    display_name text NOT NULL,
    kind text NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    base_priority integer NOT NULL,
    config jsonb DEFAULT '{}'::jsonb NOT NULL,
    added_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT metadata_sources_added_at_ts_decode_range CHECK (((added_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (added_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.metadata_sources OWNER TO reverie_migrator;

--
-- Name: metadata_versions; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.metadata_versions (
    id uuid DEFAULT uuidv7() NOT NULL,
    manifestation_id uuid NOT NULL,
    source text NOT NULL,
    field_name text NOT NULL,
    old_value jsonb,
    new_value jsonb NOT NULL,
    status public.metadata_review_status DEFAULT 'pending'::public.metadata_review_status NOT NULL,
    confidence_score real NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    resolved_at timestamp with time zone,
    resolved_by uuid,
    value_hash bytea NOT NULL,
    match_type text NOT NULL,
    first_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    observation_count integer DEFAULT 1 NOT NULL,
    CONSTRAINT metadata_versions_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT metadata_versions_first_seen_at_ts_decode_range CHECK (((first_seen_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (first_seen_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT metadata_versions_last_seen_at_ts_decode_range CHECK (((last_seen_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (last_seen_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT metadata_versions_resolved_at_ts_decode_range CHECK (((resolved_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (resolved_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.metadata_versions OWNER TO reverie_migrator;

--
-- Name: moods; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.moods (
    id uuid DEFAULT uuidv7() NOT NULL,
    name text NOT NULL
);


ALTER TABLE public.moods OWNER TO reverie_migrator;

--
-- Name: omnibus_contents; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.omnibus_contents (
    omnibus_manifestation_id uuid NOT NULL,
    contained_work_id uuid NOT NULL,
    "position" integer DEFAULT 0 NOT NULL
);


ALTER TABLE public.omnibus_contents OWNER TO reverie_migrator;

--
-- Name: password_reset_pins; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.password_reset_pins (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    pin_hash text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT password_reset_pins_consumed_at_ts_decode_range CHECK (((consumed_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (consumed_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT password_reset_pins_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT password_reset_pins_expires_at_ts_decode_range CHECK (((expires_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (expires_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.password_reset_pins OWNER TO reverie_migrator;

--
-- Name: rating_sources; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.rating_sources (
    id text NOT NULL,
    display_name text NOT NULL
);


ALTER TABLE public.rating_sources OWNER TO reverie_migrator;

--
-- Name: reading_sessions; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.reading_sessions (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    manifestation_id uuid NOT NULL,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    ended_at timestamp with time zone,
    duration_seconds integer,
    CONSTRAINT reading_sessions_ended_at_ts_decode_range CHECK (((ended_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (ended_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT reading_sessions_started_at_ts_decode_range CHECK (((started_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (started_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.reading_sessions OWNER TO reverie_migrator;

--
-- Name: TABLE reading_sessions; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.reading_sessions IS 'RLS enabled (default deny). No handlers implemented yet. Define policies before implementing handlers.';


--
-- Name: reading_state; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.reading_state (
    user_id uuid NOT NULL,
    manifestation_id uuid NOT NULL,
    progress_pct real,
    last_read_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    status public.reading_status,
    rating smallint,
    notes text,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    CONSTRAINT reading_state_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT reading_state_finished_at_ts_decode_range CHECK (((finished_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (finished_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT reading_state_last_read_at_ts_decode_range CHECK (((last_read_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (last_read_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT reading_state_notes_len CHECK (((notes IS NULL) OR (char_length(notes) <= 10000))),
    CONSTRAINT reading_state_progress_paired_with_timestamp CHECK (((progress_pct IS NULL) = (last_read_at IS NULL))),
    CONSTRAINT reading_state_progress_pct_range CHECK (((progress_pct IS NULL) OR ((progress_pct >= (0)::double precision) AND (progress_pct <= (100)::double precision)))),
    CONSTRAINT reading_state_rating_range CHECK (((rating IS NULL) OR ((rating >= 1) AND (rating <= 5)))),
    CONSTRAINT reading_state_started_at_ts_decode_range CHECK (((started_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (started_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT reading_state_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.reading_state OWNER TO reverie_migrator;

--
-- Name: TABLE reading_state; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.reading_state IS 'RLS enabled. reverie_app and reverie_readonly must call set_config(''app.current_user_id'', ..., true) (transaction-local) before queries — see db::acquire_with_rls. Each user sees only rows where user_id matches the GUC. reverie (owner) bypasses RLS.';


--
-- Name: series; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.series (
    id uuid DEFAULT uuidv7() NOT NULL,
    name text NOT NULL,
    sort_name text NOT NULL,
    parent_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT series_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.series OWNER TO reverie_migrator;

--
-- Name: series_works; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.series_works (
    series_id uuid NOT NULL,
    work_id uuid NOT NULL,
    "position" double precision,
    is_omnibus boolean DEFAULT false NOT NULL,
    note text
);


ALTER TABLE public.series_works OWNER TO reverie_migrator;

--
-- Name: settings; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.settings (
    id boolean DEFAULT true NOT NULL,
    enrichment_enabled boolean DEFAULT true NOT NULL,
    enrichment_concurrency integer DEFAULT 2 NOT NULL,
    enrichment_poll_idle_secs integer DEFAULT 30 NOT NULL,
    enrichment_fetch_budget_secs integer DEFAULT 15 NOT NULL,
    cover_max_bytes bigint DEFAULT 10485760 NOT NULL,
    cover_download_timeout_secs integer DEFAULT 30 NOT NULL,
    cover_min_long_edge_px integer DEFAULT 1000 NOT NULL,
    cover_redirect_limit integer DEFAULT 3 NOT NULL,
    writeback_enabled boolean DEFAULT true NOT NULL,
    writeback_concurrency integer DEFAULT 2 NOT NULL,
    writeback_poll_idle_secs integer DEFAULT 5 NOT NULL,
    writeback_max_attempts integer DEFAULT 3 NOT NULL,
    opds_enabled boolean DEFAULT true NOT NULL,
    opds_page_size integer DEFAULT 50 NOT NULL,
    format_priority text[] DEFAULT '{epub,pdf,mobi,azw3,cbz,cbr}'::text[] NOT NULL,
    cleanup_mode text DEFAULT 'all'::text NOT NULL,
    openlibrary_base_url text DEFAULT 'https://openlibrary.org'::text NOT NULL,
    googlebooks_base_url text DEFAULT 'https://www.googleapis.com/books/v1'::text NOT NULL,
    hardcover_base_url text DEFAULT 'https://api.hardcover.app/v1/graphql'::text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    provider_visibility jsonb DEFAULT '{}'::jsonb NOT NULL,
    revision bigint DEFAULT 0 NOT NULL,
    CONSTRAINT settings_cleanup_mode_check CHECK ((cleanup_mode = ANY (ARRAY['all'::text, 'ingested'::text, 'none'::text]))),
    CONSTRAINT settings_cover_download_timeout_secs_check CHECK ((cover_download_timeout_secs >= 1)),
    CONSTRAINT settings_cover_max_bytes_check CHECK ((cover_max_bytes >= 1)),
    CONSTRAINT settings_cover_min_long_edge_px_check CHECK ((cover_min_long_edge_px >= 1)),
    CONSTRAINT settings_cover_redirect_limit_check CHECK ((cover_redirect_limit >= 0)),
    CONSTRAINT settings_enrichment_concurrency_check CHECK (((enrichment_concurrency >= 1) AND (enrichment_concurrency <= 10))),
    CONSTRAINT settings_enrichment_fetch_budget_secs_check CHECK ((enrichment_fetch_budget_secs >= 1)),
    CONSTRAINT settings_enrichment_poll_idle_secs_check CHECK ((enrichment_poll_idle_secs >= 1)),
    CONSTRAINT settings_opds_page_size_check CHECK (((opds_page_size >= 1) AND (opds_page_size <= 500))),
    CONSTRAINT settings_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT settings_writeback_concurrency_check CHECK (((writeback_concurrency >= 1) AND (writeback_concurrency <= 10))),
    CONSTRAINT settings_writeback_max_attempts_check CHECK ((writeback_max_attempts >= 1)),
    CONSTRAINT settings_writeback_poll_idle_secs_check CHECK ((writeback_poll_idle_secs >= 1)),
    CONSTRAINT singleton CHECK ((id = true))
);


ALTER TABLE public.settings OWNER TO reverie_migrator;

--
-- Name: shelf_items; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.shelf_items (
    shelf_id uuid NOT NULL,
    manifestation_id uuid NOT NULL,
    added_at timestamp with time zone DEFAULT now() NOT NULL,
    "position" integer DEFAULT 0 NOT NULL,
    CONSTRAINT shelf_items_added_at_ts_decode_range CHECK (((added_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (added_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.shelf_items OWNER TO reverie_migrator;

--
-- Name: shelves; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.shelves (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    is_system boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT shelves_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT shelves_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.shelves OWNER TO reverie_migrator;

--
-- Name: TABLE shelves; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.shelves IS 'No RLS. Ownership enforced at application layer — all queries scope by user_id. See routes/shelves/ THREAT annotation.';


--
-- Name: tags; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.tags (
    id uuid DEFAULT uuidv7() NOT NULL,
    name text NOT NULL
);


ALTER TABLE public.tags OWNER TO reverie_migrator;

--
-- Name: user_identities; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.user_identities (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    provider public.identity_provider NOT NULL,
    issuer text NOT NULL,
    subject text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT user_identities_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT user_identities_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.user_identities OWNER TO reverie_migrator;

--
-- Name: user_preferences; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.user_preferences (
    user_id uuid NOT NULL,
    hidden_columns text[],
    density public.library_density,
    view public.library_view,
    sort_stack text,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT user_preferences_hidden_columns_bounded CHECK ((array_length(hidden_columns, 1) <= 64)),
    CONSTRAINT user_preferences_sort_stack_shape CHECK ((sort_stack ~ '^-?[a-z_]+(,-?[a-z_]+){0,2}$'::text)),
    CONSTRAINT user_preferences_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.user_preferences OWNER TO reverie_migrator;

--
-- Name: users; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.users (
    id uuid DEFAULT uuidv7() NOT NULL,
    oidc_subject text,
    display_name text NOT NULL,
    email text,
    role public.user_role DEFAULT 'adult'::public.user_role NOT NULL,
    is_child boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    session_version integer DEFAULT 0 NOT NULL,
    theme_preference public.theme_preference DEFAULT 'system'::public.theme_preference NOT NULL,
    disabled_at timestamp with time zone,
    CONSTRAINT chk_child_role_sync CHECK ((((is_child = true) AND (role = 'child'::public.user_role)) OR ((is_child = false) AND (role <> 'child'::public.user_role)))),
    CONSTRAINT users_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT users_disabled_at_ts_decode_range CHECK (((disabled_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (disabled_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT users_session_version_nonneg CHECK ((session_version >= 0)),
    CONSTRAINT users_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.users OWNER TO reverie_migrator;

--
-- Name: webhook_deliveries; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.webhook_deliveries (
    id uuid DEFAULT uuidv7() NOT NULL,
    webhook_id uuid NOT NULL,
    event_type text NOT NULL,
    payload jsonb NOT NULL,
    response_status integer,
    delivered_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT webhook_deliveries_delivered_at_ts_decode_range CHECK (((delivered_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (delivered_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.webhook_deliveries OWNER TO reverie_migrator;

--
-- Name: TABLE webhook_deliveries; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.webhook_deliveries IS 'RLS enabled (default deny). No handlers implemented yet. Access scoped through webhooks ownership. Define policies before implementing handlers.';


--
-- Name: webhook_event_dedupe; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.webhook_event_dedupe (
    event_id text NOT NULL,
    seen_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT webhook_event_dedupe_seen_at_ts_decode_range CHECK (((seen_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (seen_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.webhook_event_dedupe OWNER TO reverie_migrator;

--
-- Name: TABLE webhook_event_dedupe; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.webhook_event_dedupe IS 'Short-TTL dedupe set for terminal webhook events (UNK-98). Keyed by stable event id (writeback:{job_id}:{outcome}). Rows past the TTL are purged opportunistically on dispatch; see services::writeback::events::dispatch.';


--
-- Name: webhooks; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.webhooks (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    url text NOT NULL,
    events jsonb DEFAULT '[]'::jsonb NOT NULL,
    payload_template text,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT webhooks_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.webhooks OWNER TO reverie_migrator;

--
-- Name: TABLE webhooks; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.webhooks IS 'RLS enabled (default deny). No handlers implemented yet. Define policies before implementing handlers.';


--
-- Name: work_authors; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.work_authors (
    work_id uuid NOT NULL,
    author_id uuid NOT NULL,
    role public.author_role DEFAULT 'author'::public.author_role NOT NULL,
    "position" integer DEFAULT 0 NOT NULL,
    source_version_id uuid
);


ALTER TABLE public.work_authors OWNER TO reverie_migrator;

--
-- Name: work_external_identifiers; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.work_external_identifiers (
    work_id uuid NOT NULL,
    scheme text NOT NULL,
    external_id text NOT NULL,
    source_version_id uuid,
    CONSTRAINT work_external_identifiers_external_id_check CHECK ((external_id ~ '^[A-Za-z0-9._-]{1,255}$'::text))
);


ALTER TABLE public.work_external_identifiers OWNER TO reverie_migrator;

--
-- Name: works; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.works (
    id uuid DEFAULT uuidv7() NOT NULL,
    title text NOT NULL,
    sort_title text NOT NULL,
    description text,
    language text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    search_vector tsvector,
    title_version_id uuid,
    description_version_id uuid,
    language_version_id uuid,
    subtitle text,
    subtitle_version_id uuid,
    first_author_sort_name text,
    CONSTRAINT works_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT works_updated_at_ts_decode_range CHECK (((updated_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (updated_at < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE public.works OWNER TO reverie_migrator;

--
-- Name: writeback_jobs; Type: TABLE; Schema: public; Owner: reverie_migrator
--

CREATE TABLE public.writeback_jobs (
    id uuid DEFAULT uuidv7() NOT NULL,
    manifestation_id uuid NOT NULL,
    reason text NOT NULL,
    status public.writeback_status DEFAULT 'pending'::public.writeback_status NOT NULL,
    attempt_count integer DEFAULT 0 NOT NULL,
    last_attempted_at timestamp with time zone,
    completed_at timestamp with time zone,
    error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT writeback_jobs_completed_at_ts_decode_range CHECK (((completed_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (completed_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT writeback_jobs_created_at_ts_decode_range CHECK (((created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT writeback_jobs_last_attempted_at_ts_decode_range CHECK (((last_attempted_at >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (last_attempted_at < '10000-01-01 00:00:00+00'::timestamp with time zone))),
    CONSTRAINT writeback_jobs_reason_chk CHECK ((reason = ANY (ARRAY['metadata'::text, 'cover'::text])))
);


ALTER TABLE public.writeback_jobs OWNER TO reverie_migrator;

--
-- Name: TABLE writeback_jobs; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON TABLE public.writeback_jobs IS 'Queue of pending/in-flight OPF writeback operations. One row per canonical pointer move. Drained by services::writeback::queue.';


--
-- Name: COLUMN writeback_jobs.reason; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON COLUMN public.writeback_jobs.reason IS '''metadata'' for text/field pointer moves; ''cover'' when a new cover sidecar needs embedding.';


--
-- Name: COLUMN writeback_jobs.status; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON COLUMN public.writeback_jobs.status IS 'pending → in_progress → (complete | failed | skipped). ''skipped'' is terminal — means max retries exhausted or non-retryable error.';


--
-- Name: session; Type: TABLE; Schema: tower_sessions; Owner: reverie_migrator
--

CREATE TABLE tower_sessions.session (
    id text NOT NULL,
    data bytea NOT NULL,
    expiry_date timestamp with time zone NOT NULL,
    CONSTRAINT session_expiry_date_ts_decode_range CHECK (((expiry_date >= '0001-01-01 00:00:00+00'::timestamp with time zone) AND (expiry_date < '10000-01-01 00:00:00+00'::timestamp with time zone)))
);


ALTER TABLE tower_sessions.session OWNER TO reverie_migrator;

--
-- Name: _sqlx_migrations _sqlx_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public._sqlx_migrations
    ADD CONSTRAINT _sqlx_migrations_pkey PRIMARY KEY (version);


--
-- Name: api_cache api_cache_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.api_cache
    ADD CONSTRAINT api_cache_pkey PRIMARY KEY (id);


--
-- Name: api_cache api_cache_source_lookup_key_key; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.api_cache
    ADD CONSTRAINT api_cache_source_lookup_key_key UNIQUE (source, lookup_key);


--
-- Name: authors authors_name_unique; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.authors
    ADD CONSTRAINT authors_name_unique UNIQUE (name);


--
-- Name: authors authors_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.authors
    ADD CONSTRAINT authors_pkey PRIMARY KEY (id);


--
-- Name: device_tokens device_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.device_tokens
    ADD CONSTRAINT device_tokens_pkey PRIMARY KEY (id);


--
-- Name: field_locks field_locks_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.field_locks
    ADD CONSTRAINT field_locks_pkey PRIMARY KEY (manifestation_id, entity_type, field_name);


--
-- Name: genres genres_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.genres
    ADD CONSTRAINT genres_pkey PRIMARY KEY (id);


--
-- Name: identifier_schemes identifier_schemes_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.identifier_schemes
    ADD CONSTRAINT identifier_schemes_pkey PRIMARY KEY (id);


--
-- Name: ingestion_jobs ingestion_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.ingestion_jobs
    ADD CONSTRAINT ingestion_jobs_pkey PRIMARY KEY (id);


--
-- Name: instance_bootstrap instance_bootstrap_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.instance_bootstrap
    ADD CONSTRAINT instance_bootstrap_pkey PRIMARY KEY (id);


--
-- Name: local_credentials local_credentials_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.local_credentials
    ADD CONSTRAINT local_credentials_pkey PRIMARY KEY (user_id);


--
-- Name: local_login_throttle local_login_throttle_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.local_login_throttle
    ADD CONSTRAINT local_login_throttle_pkey PRIMARY KEY (email_lower);


--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_external_identifiers
    ADD CONSTRAINT manifestation_external_identifiers_pkey PRIMARY KEY (manifestation_id, scheme);


--
-- Name: manifestation_external_ratings manifestation_external_ratings_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_external_ratings
    ADD CONSTRAINT manifestation_external_ratings_pkey PRIMARY KEY (manifestation_id, source);


--
-- Name: manifestation_genres manifestation_genres_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_genres
    ADD CONSTRAINT manifestation_genres_pkey PRIMARY KEY (manifestation_id, genre_id);


--
-- Name: manifestation_moods manifestation_moods_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_moods
    ADD CONSTRAINT manifestation_moods_pkey PRIMARY KEY (manifestation_id, mood_id);


--
-- Name: manifestation_tags manifestation_tags_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_tags
    ADD CONSTRAINT manifestation_tags_pkey PRIMARY KEY (manifestation_id, tag_id);


--
-- Name: manifestations manifestations_file_hash_unique; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_file_hash_unique UNIQUE (ingestion_file_hash);


--
-- Name: manifestations manifestations_file_path_key; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_file_path_key UNIQUE (file_path);


--
-- Name: manifestations manifestations_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_pkey PRIMARY KEY (id);


--
-- Name: metadata_sources metadata_sources_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.metadata_sources
    ADD CONSTRAINT metadata_sources_pkey PRIMARY KEY (id);


--
-- Name: metadata_versions metadata_versions_mfs_hash_unique; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.metadata_versions
    ADD CONSTRAINT metadata_versions_mfs_hash_unique UNIQUE (manifestation_id, source, field_name, value_hash);


--
-- Name: metadata_versions metadata_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.metadata_versions
    ADD CONSTRAINT metadata_versions_pkey PRIMARY KEY (id);


--
-- Name: moods moods_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.moods
    ADD CONSTRAINT moods_pkey PRIMARY KEY (id);


--
-- Name: omnibus_contents omnibus_contents_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.omnibus_contents
    ADD CONSTRAINT omnibus_contents_pkey PRIMARY KEY (omnibus_manifestation_id, contained_work_id);


--
-- Name: password_reset_pins password_reset_pins_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.password_reset_pins
    ADD CONSTRAINT password_reset_pins_pkey PRIMARY KEY (id);


--
-- Name: rating_sources rating_sources_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.rating_sources
    ADD CONSTRAINT rating_sources_pkey PRIMARY KEY (id);


--
-- Name: reading_sessions reading_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.reading_sessions
    ADD CONSTRAINT reading_sessions_pkey PRIMARY KEY (id);


--
-- Name: reading_state reading_state_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.reading_state
    ADD CONSTRAINT reading_state_pkey PRIMARY KEY (user_id, manifestation_id);


--
-- Name: series series_name_unique; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.series
    ADD CONSTRAINT series_name_unique UNIQUE (name);


--
-- Name: series series_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.series
    ADD CONSTRAINT series_pkey PRIMARY KEY (id);


--
-- Name: series_works series_works_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.series_works
    ADD CONSTRAINT series_works_pkey PRIMARY KEY (series_id, work_id);


--
-- Name: settings settings_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.settings
    ADD CONSTRAINT settings_pkey PRIMARY KEY (id);


--
-- Name: shelf_items shelf_items_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.shelf_items
    ADD CONSTRAINT shelf_items_pkey PRIMARY KEY (shelf_id, manifestation_id);


--
-- Name: shelves shelves_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.shelves
    ADD CONSTRAINT shelves_pkey PRIMARY KEY (id);


--
-- Name: tags tags_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.tags
    ADD CONSTRAINT tags_pkey PRIMARY KEY (id);


--
-- Name: user_identities user_identities_issuer_subject_key; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.user_identities
    ADD CONSTRAINT user_identities_issuer_subject_key UNIQUE (issuer, subject);


--
-- Name: user_identities user_identities_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.user_identities
    ADD CONSTRAINT user_identities_pkey PRIMARY KEY (id);


--
-- Name: user_preferences user_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT user_preferences_pkey PRIMARY KEY (user_id);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: webhook_deliveries webhook_deliveries_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.webhook_deliveries
    ADD CONSTRAINT webhook_deliveries_pkey PRIMARY KEY (id);


--
-- Name: webhook_event_dedupe webhook_event_dedupe_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.webhook_event_dedupe
    ADD CONSTRAINT webhook_event_dedupe_pkey PRIMARY KEY (event_id);


--
-- Name: webhooks webhooks_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.webhooks
    ADD CONSTRAINT webhooks_pkey PRIMARY KEY (id);


--
-- Name: work_authors work_authors_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.work_authors
    ADD CONSTRAINT work_authors_pkey PRIMARY KEY (work_id, author_id, role);


--
-- Name: work_external_identifiers work_external_identifiers_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.work_external_identifiers
    ADD CONSTRAINT work_external_identifiers_pkey PRIMARY KEY (work_id, scheme);


--
-- Name: works works_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.works
    ADD CONSTRAINT works_pkey PRIMARY KEY (id);


--
-- Name: writeback_jobs writeback_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.writeback_jobs
    ADD CONSTRAINT writeback_jobs_pkey PRIMARY KEY (id);


--
-- Name: session session_pkey; Type: CONSTRAINT; Schema: tower_sessions; Owner: reverie_migrator
--

ALTER TABLE ONLY tower_sessions.session
    ADD CONSTRAINT session_pkey PRIMARY KEY (id);


--
-- Name: idx_api_cache_expires_at; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_api_cache_expires_at ON public.api_cache USING btree (expires_at);


--
-- Name: idx_authors_name_trgm; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_authors_name_trgm ON public.authors USING gist (public.immutable_unaccent(name) public.gist_trgm_ops);


--
-- Name: idx_authors_sort_name_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_authors_sort_name_id ON public.authors USING btree (sort_name, id);


--
-- Name: idx_device_tokens_user_active; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_device_tokens_user_active ON public.device_tokens USING btree (user_id, created_at DESC) WHERE (revoked_at IS NULL);


--
-- Name: idx_field_locks_locked_by; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_field_locks_locked_by ON public.field_locks USING btree (locked_by) WHERE (locked_by IS NOT NULL);


--
-- Name: idx_genres_name_lower; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE UNIQUE INDEX idx_genres_name_lower ON public.genres USING btree (lower(name));


--
-- Name: idx_genres_name_trgm; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_genres_name_trgm ON public.genres USING gin (public.immutable_unaccent(name) public.gin_trgm_ops);


--
-- Name: idx_ingestion_jobs_batch_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_ingestion_jobs_batch_id ON public.ingestion_jobs USING btree (batch_id);


--
-- Name: idx_ingestion_jobs_status; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_ingestion_jobs_status ON public.ingestion_jobs USING btree (status);


--
-- Name: idx_manifestation_external_identifiers_source_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestation_external_identifiers_source_version_id ON public.manifestation_external_identifiers USING btree (source_version_id) WHERE (source_version_id IS NOT NULL);


--
-- Name: idx_manifestation_genres_genre_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestation_genres_genre_id ON public.manifestation_genres USING btree (genre_id);


--
-- Name: idx_manifestation_genres_source_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestation_genres_source_version_id ON public.manifestation_genres USING btree (source_version_id) WHERE (source_version_id IS NOT NULL);


--
-- Name: idx_manifestation_moods_mood_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestation_moods_mood_id ON public.manifestation_moods USING btree (mood_id);


--
-- Name: idx_manifestation_moods_source_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestation_moods_source_version_id ON public.manifestation_moods USING btree (source_version_id) WHERE (source_version_id IS NOT NULL);


--
-- Name: idx_manifestation_tags_source_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestation_tags_source_version_id ON public.manifestation_tags USING btree (source_version_id) WHERE (source_version_id IS NOT NULL);


--
-- Name: idx_manifestation_tags_tag_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestation_tags_tag_id ON public.manifestation_tags USING btree (tag_id);


--
-- Name: idx_manifestations_content_rating_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_content_rating_version_id ON public.manifestations USING btree (content_rating_version_id) WHERE (content_rating_version_id IS NOT NULL);


--
-- Name: idx_manifestations_cover_source; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_cover_source ON public.manifestations USING btree (cover_source) WHERE (cover_source IS NOT NULL);


--
-- Name: idx_manifestations_cover_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_cover_version_id ON public.manifestations USING btree (cover_version_id) WHERE (cover_version_id IS NOT NULL);


--
-- Name: idx_manifestations_enrichment_queue; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_enrichment_queue ON public.manifestations USING btree (enrichment_status, enrichment_attempted_at NULLS FIRST) WHERE (enrichment_status = ANY (ARRAY['pending'::public.enrichment_status, 'failed'::public.enrichment_status]));


--
-- Name: idx_manifestations_isbn_10_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_isbn_10_version_id ON public.manifestations USING btree (isbn_10_version_id) WHERE (isbn_10_version_id IS NOT NULL);


--
-- Name: idx_manifestations_isbn_13; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_isbn_13 ON public.manifestations USING btree (isbn_13) WHERE (isbn_13 IS NOT NULL);


--
-- Name: idx_manifestations_isbn_13_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_isbn_13_version_id ON public.manifestations USING btree (isbn_13_version_id) WHERE (isbn_13_version_id IS NOT NULL);


--
-- Name: idx_manifestations_pages_keyset; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_pages_keyset ON public.manifestations USING btree (pages, id);


--
-- Name: idx_manifestations_pages_keyset_desc; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_pages_keyset_desc ON public.manifestations USING btree (pages DESC NULLS LAST, id DESC);


--
-- Name: idx_manifestations_pages_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_pages_version_id ON public.manifestations USING btree (pages_version_id) WHERE (pages_version_id IS NOT NULL);


--
-- Name: idx_manifestations_pub_date_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_pub_date_version_id ON public.manifestations USING btree (pub_date_version_id) WHERE (pub_date_version_id IS NOT NULL);


--
-- Name: idx_manifestations_publisher_trgm; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_publisher_trgm ON public.manifestations USING gin (public.immutable_unaccent(publisher) public.gin_trgm_ops) WHERE (publisher IS NOT NULL);


--
-- Name: idx_manifestations_publisher_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_publisher_version_id ON public.manifestations USING btree (publisher_version_id) WHERE (publisher_version_id IS NOT NULL);


--
-- Name: idx_manifestations_recent_keyset; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_recent_keyset ON public.manifestations USING btree (created_at DESC, id DESC);


--
-- Name: idx_manifestations_suspected_duplicate_work_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_suspected_duplicate_work_id ON public.manifestations USING btree (suspected_duplicate_work_id) WHERE (suspected_duplicate_work_id IS NOT NULL);


--
-- Name: idx_manifestations_work_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_manifestations_work_id ON public.manifestations USING btree (work_id);


--
-- Name: idx_metadata_versions_resolved_by; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_metadata_versions_resolved_by ON public.metadata_versions USING btree (resolved_by) WHERE (resolved_by IS NOT NULL);


--
-- Name: idx_metadata_versions_source; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_metadata_versions_source ON public.metadata_versions USING btree (source);


--
-- Name: idx_moods_name_lower; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE UNIQUE INDEX idx_moods_name_lower ON public.moods USING btree (lower(name));


--
-- Name: idx_moods_name_trgm; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_moods_name_trgm ON public.moods USING gin (public.immutable_unaccent(name) public.gin_trgm_ops);


--
-- Name: idx_mv_last_seen; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_mv_last_seen ON public.metadata_versions USING btree (last_seen_at);


--
-- Name: idx_mv_manifestation_field; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_mv_manifestation_field ON public.metadata_versions USING btree (manifestation_id, field_name);


--
-- Name: idx_mv_pending_last_seen; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_mv_pending_last_seen ON public.metadata_versions USING btree (manifestation_id, last_seen_at DESC) WHERE (status = 'pending'::public.metadata_review_status);


--
-- Name: idx_omnibus_contents_contained_work_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_omnibus_contents_contained_work_id ON public.omnibus_contents USING btree (contained_work_id);


--
-- Name: idx_password_reset_pins_active_unique; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE UNIQUE INDEX idx_password_reset_pins_active_unique ON public.password_reset_pins USING btree (user_id) WHERE (consumed_at IS NULL);


--
-- Name: INDEX idx_password_reset_pins_active_unique; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON INDEX public.idx_password_reset_pins_active_unique IS 'Enforces at most one active (unconsumed) recovery PIN per user. Load-bearing for the single-active invariant under concurrency; see models::password_reset_pin::rotate for the retry path on SQLSTATE 23505.';


--
-- Name: idx_password_reset_pins_user_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_password_reset_pins_user_id ON public.password_reset_pins USING btree (user_id);


--
-- Name: idx_reading_sessions_manifestation_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_reading_sessions_manifestation_id ON public.reading_sessions USING btree (manifestation_id);


--
-- Name: idx_reading_sessions_user_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_reading_sessions_user_id ON public.reading_sessions USING btree (user_id);


--
-- Name: idx_reading_state_manifestation_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_reading_state_manifestation_id ON public.reading_state USING btree (manifestation_id);


--
-- Name: idx_reading_state_user_last_read; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_reading_state_user_last_read ON public.reading_state USING btree (user_id, last_read_at DESC NULLS LAST);


--
-- Name: idx_series_name_trgm; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_series_name_trgm ON public.series USING gist (public.immutable_unaccent(name) public.gist_trgm_ops);


--
-- Name: idx_series_parent_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_series_parent_id ON public.series USING btree (parent_id) WHERE (parent_id IS NOT NULL);


--
-- Name: idx_series_sort_name_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_series_sort_name_id ON public.series USING btree (sort_name, id);


--
-- Name: idx_series_works_series_position; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_series_works_series_position ON public.series_works USING btree (series_id, "position");


--
-- Name: idx_series_works_work_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_series_works_work_id ON public.series_works USING btree (work_id);


--
-- Name: idx_shelf_items_manifestation_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_shelf_items_manifestation_id ON public.shelf_items USING btree (manifestation_id);


--
-- Name: idx_shelf_items_shelf_keyset; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_shelf_items_shelf_keyset ON public.shelf_items USING btree (shelf_id, "position", added_at, manifestation_id);


--
-- Name: idx_shelves_user_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_shelves_user_id ON public.shelves USING btree (user_id);


--
-- Name: idx_shelves_user_keyset; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_shelves_user_keyset ON public.shelves USING btree (user_id, is_system DESC, name, id);


--
-- Name: idx_tags_name_lower; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE UNIQUE INDEX idx_tags_name_lower ON public.tags USING btree (lower(name));


--
-- Name: idx_tags_name_trgm; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_tags_name_trgm ON public.tags USING gin (public.immutable_unaccent(name) public.gin_trgm_ops);


--
-- Name: idx_user_identities_user_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_user_identities_user_id ON public.user_identities USING btree (user_id);


--
-- Name: idx_users_email_lower; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE UNIQUE INDEX idx_users_email_lower ON public.users USING btree (lower(email)) WHERE (email IS NOT NULL);


--
-- Name: idx_webhook_deliveries_webhook_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_webhook_deliveries_webhook_id ON public.webhook_deliveries USING btree (webhook_id);


--
-- Name: idx_webhook_event_dedupe_seen_at; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_webhook_event_dedupe_seen_at ON public.webhook_event_dedupe USING btree (seen_at);


--
-- Name: idx_webhooks_user_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_webhooks_user_id ON public.webhooks USING btree (user_id);


--
-- Name: idx_work_authors_author_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_work_authors_author_id ON public.work_authors USING btree (author_id);


--
-- Name: idx_work_authors_source_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_work_authors_source_version_id ON public.work_authors USING btree (source_version_id) WHERE (source_version_id IS NOT NULL);


--
-- Name: idx_work_authors_work_position; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_work_authors_work_position ON public.work_authors USING btree (work_id, "position");


--
-- Name: idx_work_external_identifiers_source_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_work_external_identifiers_source_version_id ON public.work_external_identifiers USING btree (source_version_id) WHERE (source_version_id IS NOT NULL);


--
-- Name: idx_works_description_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_description_version_id ON public.works USING btree (description_version_id) WHERE (description_version_id IS NOT NULL);


--
-- Name: idx_works_first_author_sort_desc; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_first_author_sort_desc ON public.works USING btree (first_author_sort_name DESC NULLS LAST, id DESC);


--
-- Name: idx_works_first_author_sort_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_first_author_sort_id ON public.works USING btree (first_author_sort_name, id);


--
-- Name: idx_works_language_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_language_version_id ON public.works USING btree (language_version_id) WHERE (language_version_id IS NOT NULL);


--
-- Name: idx_works_search_vector; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_search_vector ON public.works USING gin (search_vector);


--
-- Name: idx_works_sort_title_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_sort_title_id ON public.works USING btree (sort_title, id);


--
-- Name: idx_works_subtitle_trgm; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_subtitle_trgm ON public.works USING gin (public.immutable_unaccent(subtitle) public.gin_trgm_ops);


--
-- Name: idx_works_subtitle_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_subtitle_version_id ON public.works USING btree (subtitle_version_id) WHERE (subtitle_version_id IS NOT NULL);


--
-- Name: idx_works_title_trgm; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_title_trgm ON public.works USING gist (public.immutable_unaccent(title) public.gist_trgm_ops);


--
-- Name: idx_works_title_version_id; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_works_title_version_id ON public.works USING btree (title_version_id) WHERE (title_version_id IS NOT NULL);


--
-- Name: idx_writeback_jobs_in_progress_unique; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE UNIQUE INDEX idx_writeback_jobs_in_progress_unique ON public.writeback_jobs USING btree (manifestation_id) WHERE (status = 'in_progress'::public.writeback_status);


--
-- Name: INDEX idx_writeback_jobs_in_progress_unique; Type: COMMENT; Schema: public; Owner: reverie_migrator
--

COMMENT ON INDEX public.idx_writeback_jobs_in_progress_unique IS 'Enforces at most one in_progress writeback job per manifestation. Load-bearing for multi-replica correctness; see services::writeback::queue::claim_next for the retry path on SQLSTATE 23505.';


--
-- Name: idx_writeback_jobs_manifestation_status; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_writeback_jobs_manifestation_status ON public.writeback_jobs USING btree (manifestation_id, status);


--
-- Name: idx_writeback_jobs_queue; Type: INDEX; Schema: public; Owner: reverie_migrator
--

CREATE INDEX idx_writeback_jobs_queue ON public.writeback_jobs USING btree (last_attempted_at NULLS FIRST, created_at) WHERE (status = ANY (ARRAY['pending'::public.writeback_status, 'failed'::public.writeback_status]));


--
-- Name: session_expiry_date_idx; Type: INDEX; Schema: tower_sessions; Owner: reverie_migrator
--

CREATE INDEX session_expiry_date_idx ON tower_sessions.session USING btree (expiry_date);


--
-- Name: settings settings_changed_trigger; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER settings_changed_trigger AFTER UPDATE ON public.settings FOR EACH ROW EXECUTE FUNCTION public.notify_settings_changed();


--
-- Name: shelves shelves_set_updated_at; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER shelves_set_updated_at BEFORE UPDATE ON public.shelves FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: local_credentials trg_local_credentials_updated_at; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER trg_local_credentials_updated_at BEFORE UPDATE ON public.local_credentials FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: local_login_throttle trg_local_login_throttle_updated_at; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER trg_local_login_throttle_updated_at BEFORE UPDATE ON public.local_login_throttle FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: manifestations trg_manifestations_updated_at; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER trg_manifestations_updated_at BEFORE UPDATE ON public.manifestations FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: reading_state trg_reading_state_updated_at; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER trg_reading_state_updated_at BEFORE UPDATE ON public.reading_state FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: user_identities trg_user_identities_updated_at; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER trg_user_identities_updated_at BEFORE UPDATE ON public.user_identities FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: users trg_users_updated_at; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER trg_users_updated_at BEFORE UPDATE ON public.users FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: works trg_works_search_vector; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER trg_works_search_vector BEFORE INSERT OR UPDATE OF title, description ON public.works FOR EACH ROW EXECUTE FUNCTION public.works_search_vector_update();


--
-- Name: works trg_works_updated_at; Type: TRIGGER; Schema: public; Owner: reverie_migrator
--

CREATE TRIGGER trg_works_updated_at BEFORE UPDATE ON public.works FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: device_tokens device_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.device_tokens
    ADD CONSTRAINT device_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: field_locks field_locks_locked_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.field_locks
    ADD CONSTRAINT field_locks_locked_by_fkey FOREIGN KEY (locked_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: field_locks field_locks_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.field_locks
    ADD CONSTRAINT field_locks_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: local_credentials local_credentials_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.local_credentials
    ADD CONSTRAINT local_credentials_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_external_identifiers
    ADD CONSTRAINT manifestation_external_identifiers_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_scheme_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_external_identifiers
    ADD CONSTRAINT manifestation_external_identifiers_scheme_fkey FOREIGN KEY (scheme) REFERENCES public.identifier_schemes(id);


--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_source_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_external_identifiers
    ADD CONSTRAINT manifestation_external_identifiers_source_version_id_fkey FOREIGN KEY (source_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestation_external_ratings manifestation_external_ratings_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_external_ratings
    ADD CONSTRAINT manifestation_external_ratings_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: manifestation_external_ratings manifestation_external_ratings_source_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_external_ratings
    ADD CONSTRAINT manifestation_external_ratings_source_fkey FOREIGN KEY (source) REFERENCES public.rating_sources(id);


--
-- Name: manifestation_genres manifestation_genres_genre_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_genres
    ADD CONSTRAINT manifestation_genres_genre_id_fkey FOREIGN KEY (genre_id) REFERENCES public.genres(id) ON DELETE CASCADE;


--
-- Name: manifestation_genres manifestation_genres_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_genres
    ADD CONSTRAINT manifestation_genres_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: manifestation_genres manifestation_genres_source_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_genres
    ADD CONSTRAINT manifestation_genres_source_version_id_fkey FOREIGN KEY (source_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestation_moods manifestation_moods_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_moods
    ADD CONSTRAINT manifestation_moods_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: manifestation_moods manifestation_moods_mood_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_moods
    ADD CONSTRAINT manifestation_moods_mood_id_fkey FOREIGN KEY (mood_id) REFERENCES public.moods(id) ON DELETE CASCADE;


--
-- Name: manifestation_moods manifestation_moods_source_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_moods
    ADD CONSTRAINT manifestation_moods_source_version_id_fkey FOREIGN KEY (source_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestation_tags manifestation_tags_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_tags
    ADD CONSTRAINT manifestation_tags_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: manifestation_tags manifestation_tags_source_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_tags
    ADD CONSTRAINT manifestation_tags_source_version_id_fkey FOREIGN KEY (source_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestation_tags manifestation_tags_tag_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestation_tags
    ADD CONSTRAINT manifestation_tags_tag_id_fkey FOREIGN KEY (tag_id) REFERENCES public.tags(id) ON DELETE CASCADE;


--
-- Name: manifestations manifestations_content_rating_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_content_rating_version_id_fkey FOREIGN KEY (content_rating_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestations manifestations_cover_source_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_cover_source_fkey FOREIGN KEY (cover_source) REFERENCES public.metadata_sources(id);


--
-- Name: manifestations manifestations_cover_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_cover_version_id_fkey FOREIGN KEY (cover_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestations manifestations_isbn_10_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_isbn_10_version_id_fkey FOREIGN KEY (isbn_10_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestations manifestations_isbn_13_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_isbn_13_version_id_fkey FOREIGN KEY (isbn_13_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestations manifestations_pages_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_pages_version_id_fkey FOREIGN KEY (pages_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestations manifestations_pub_date_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_pub_date_version_id_fkey FOREIGN KEY (pub_date_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestations manifestations_publisher_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_publisher_version_id_fkey FOREIGN KEY (publisher_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: manifestations manifestations_suspected_duplicate_work_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_suspected_duplicate_work_id_fkey FOREIGN KEY (suspected_duplicate_work_id) REFERENCES public.works(id) ON DELETE SET NULL;


--
-- Name: manifestations manifestations_work_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.manifestations
    ADD CONSTRAINT manifestations_work_id_fkey FOREIGN KEY (work_id) REFERENCES public.works(id) ON DELETE CASCADE;


--
-- Name: metadata_versions metadata_versions_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.metadata_versions
    ADD CONSTRAINT metadata_versions_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: metadata_versions metadata_versions_resolved_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.metadata_versions
    ADD CONSTRAINT metadata_versions_resolved_by_fkey FOREIGN KEY (resolved_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: metadata_versions metadata_versions_source_fk; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.metadata_versions
    ADD CONSTRAINT metadata_versions_source_fk FOREIGN KEY (source) REFERENCES public.metadata_sources(id);


--
-- Name: omnibus_contents omnibus_contents_contained_work_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.omnibus_contents
    ADD CONSTRAINT omnibus_contents_contained_work_id_fkey FOREIGN KEY (contained_work_id) REFERENCES public.works(id) ON DELETE CASCADE;


--
-- Name: omnibus_contents omnibus_contents_omnibus_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.omnibus_contents
    ADD CONSTRAINT omnibus_contents_omnibus_manifestation_id_fkey FOREIGN KEY (omnibus_manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: password_reset_pins password_reset_pins_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.password_reset_pins
    ADD CONSTRAINT password_reset_pins_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: rating_sources rating_sources_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.rating_sources
    ADD CONSTRAINT rating_sources_id_fkey FOREIGN KEY (id) REFERENCES public.metadata_sources(id);


--
-- Name: reading_sessions reading_sessions_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.reading_sessions
    ADD CONSTRAINT reading_sessions_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: reading_sessions reading_sessions_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.reading_sessions
    ADD CONSTRAINT reading_sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: reading_state reading_state_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.reading_state
    ADD CONSTRAINT reading_state_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: reading_state reading_state_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.reading_state
    ADD CONSTRAINT reading_state_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: series series_parent_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.series
    ADD CONSTRAINT series_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES public.series(id) ON DELETE SET NULL;


--
-- Name: series_works series_works_series_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.series_works
    ADD CONSTRAINT series_works_series_id_fkey FOREIGN KEY (series_id) REFERENCES public.series(id) ON DELETE CASCADE;


--
-- Name: series_works series_works_work_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.series_works
    ADD CONSTRAINT series_works_work_id_fkey FOREIGN KEY (work_id) REFERENCES public.works(id) ON DELETE CASCADE;


--
-- Name: shelf_items shelf_items_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.shelf_items
    ADD CONSTRAINT shelf_items_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: shelf_items shelf_items_shelf_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.shelf_items
    ADD CONSTRAINT shelf_items_shelf_id_fkey FOREIGN KEY (shelf_id) REFERENCES public.shelves(id) ON DELETE CASCADE;


--
-- Name: shelves shelves_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.shelves
    ADD CONSTRAINT shelves_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_identities user_identities_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.user_identities
    ADD CONSTRAINT user_identities_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_preferences user_preferences_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT user_preferences_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: webhook_deliveries webhook_deliveries_webhook_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.webhook_deliveries
    ADD CONSTRAINT webhook_deliveries_webhook_id_fkey FOREIGN KEY (webhook_id) REFERENCES public.webhooks(id) ON DELETE CASCADE;


--
-- Name: webhooks webhooks_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.webhooks
    ADD CONSTRAINT webhooks_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: work_authors work_authors_author_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.work_authors
    ADD CONSTRAINT work_authors_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.authors(id) ON DELETE CASCADE;


--
-- Name: work_authors work_authors_source_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.work_authors
    ADD CONSTRAINT work_authors_source_version_id_fkey FOREIGN KEY (source_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: work_authors work_authors_work_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.work_authors
    ADD CONSTRAINT work_authors_work_id_fkey FOREIGN KEY (work_id) REFERENCES public.works(id) ON DELETE CASCADE;


--
-- Name: work_external_identifiers work_external_identifiers_scheme_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.work_external_identifiers
    ADD CONSTRAINT work_external_identifiers_scheme_fkey FOREIGN KEY (scheme) REFERENCES public.identifier_schemes(id);


--
-- Name: work_external_identifiers work_external_identifiers_source_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.work_external_identifiers
    ADD CONSTRAINT work_external_identifiers_source_version_id_fkey FOREIGN KEY (source_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: work_external_identifiers work_external_identifiers_work_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.work_external_identifiers
    ADD CONSTRAINT work_external_identifiers_work_id_fkey FOREIGN KEY (work_id) REFERENCES public.works(id) ON DELETE CASCADE;


--
-- Name: works works_description_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.works
    ADD CONSTRAINT works_description_version_id_fkey FOREIGN KEY (description_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: works works_language_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.works
    ADD CONSTRAINT works_language_version_id_fkey FOREIGN KEY (language_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: works works_subtitle_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.works
    ADD CONSTRAINT works_subtitle_version_id_fkey FOREIGN KEY (subtitle_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: works works_title_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.works
    ADD CONSTRAINT works_title_version_id_fkey FOREIGN KEY (title_version_id) REFERENCES public.metadata_versions(id) ON DELETE SET NULL;


--
-- Name: writeback_jobs writeback_jobs_manifestation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: reverie_migrator
--

ALTER TABLE ONLY public.writeback_jobs
    ADD CONSTRAINT writeback_jobs_manifestation_id_fkey FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE;


--
-- Name: manifestation_external_identifiers; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.manifestation_external_identifiers ENABLE ROW LEVEL SECURITY;

--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_delete; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_external_identifiers_delete ON public.manifestation_external_identifiers FOR DELETE TO reverie_app USING (((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.id = manifestation_external_identifiers.manifestation_id))) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_ingestion_full_access; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_external_identifiers_ingestion_full_access ON public.manifestation_external_identifiers TO reverie_ingestion USING (true) WITH CHECK (true);


--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_insert; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_external_identifiers_insert ON public.manifestation_external_identifiers FOR INSERT TO reverie_app WITH CHECK (((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.id = manifestation_external_identifiers.manifestation_id))) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_select; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_external_identifiers_select ON public.manifestation_external_identifiers FOR SELECT TO reverie_app, reverie_readonly USING ((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.id = manifestation_external_identifiers.manifestation_id))));


--
-- Name: manifestation_external_identifiers manifestation_external_identifiers_update; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_external_identifiers_update ON public.manifestation_external_identifiers FOR UPDATE TO reverie_app USING (((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.id = manifestation_external_identifiers.manifestation_id))) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))))) WITH CHECK (((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.id = manifestation_external_identifiers.manifestation_id))) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_external_ratings; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.manifestation_external_ratings ENABLE ROW LEVEL SECURITY;

--
-- Name: manifestation_external_ratings manifestation_external_ratings_ingestion_full_access; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_external_ratings_ingestion_full_access ON public.manifestation_external_ratings TO reverie_ingestion USING (true) WITH CHECK (true);


--
-- Name: manifestation_external_ratings manifestation_external_ratings_select; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_external_ratings_select ON public.manifestation_external_ratings FOR SELECT TO reverie_app, reverie_readonly USING ((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.id = manifestation_external_ratings.manifestation_id))));


--
-- Name: manifestation_genres; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.manifestation_genres ENABLE ROW LEVEL SECURITY;

--
-- Name: manifestation_genres manifestation_genres_delete; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_genres_delete ON public.manifestation_genres FOR DELETE TO reverie_app USING (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_genres manifestation_genres_ingestion_full_access; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_genres_ingestion_full_access ON public.manifestation_genres TO reverie_ingestion USING (true) WITH CHECK (true);


--
-- Name: manifestation_genres manifestation_genres_insert; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_genres_insert ON public.manifestation_genres FOR INSERT TO reverie_app WITH CHECK (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_genres manifestation_genres_select; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_genres_select ON public.manifestation_genres FOR SELECT TO reverie_app, reverie_readonly USING ((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)));


--
-- Name: manifestation_genres manifestation_genres_update; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_genres_update ON public.manifestation_genres FOR UPDATE TO reverie_app USING (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_moods; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.manifestation_moods ENABLE ROW LEVEL SECURITY;

--
-- Name: manifestation_moods manifestation_moods_delete; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_moods_delete ON public.manifestation_moods FOR DELETE TO reverie_app USING (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_moods manifestation_moods_ingestion_full_access; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_moods_ingestion_full_access ON public.manifestation_moods TO reverie_ingestion USING (true) WITH CHECK (true);


--
-- Name: manifestation_moods manifestation_moods_insert; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_moods_insert ON public.manifestation_moods FOR INSERT TO reverie_app WITH CHECK (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_moods manifestation_moods_select; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_moods_select ON public.manifestation_moods FOR SELECT TO reverie_app, reverie_readonly USING ((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)));


--
-- Name: manifestation_moods manifestation_moods_update; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_moods_update ON public.manifestation_moods FOR UPDATE TO reverie_app USING (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_tags; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.manifestation_tags ENABLE ROW LEVEL SECURITY;

--
-- Name: manifestation_tags manifestation_tags_delete; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_tags_delete ON public.manifestation_tags FOR DELETE TO reverie_app USING (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_tags manifestation_tags_ingestion_full_access; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_tags_ingestion_full_access ON public.manifestation_tags TO reverie_ingestion USING (true) WITH CHECK (true);


--
-- Name: manifestation_tags manifestation_tags_insert; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_tags_insert ON public.manifestation_tags FOR INSERT TO reverie_app WITH CHECK (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestation_tags manifestation_tags_select; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_tags_select ON public.manifestation_tags FOR SELECT TO reverie_app, reverie_readonly USING ((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)));


--
-- Name: manifestation_tags manifestation_tags_update; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestation_tags_update ON public.manifestation_tags FOR UPDATE TO reverie_app USING (((manifestation_id IN ( SELECT manifestations.id
   FROM public.manifestations)) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = ( SELECT (current_setting('app.current_user_id'::text, true))::uuid AS current_setting)) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: manifestations; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.manifestations ENABLE ROW LEVEL SECURITY;

--
-- Name: manifestations manifestations_delete; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestations_delete ON public.manifestations FOR DELETE TO reverie_app USING ((EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));


--
-- Name: manifestations manifestations_ingestion_full_access; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestations_ingestion_full_access ON public.manifestations TO reverie_ingestion USING (true) WITH CHECK (true);


--
-- Name: manifestations manifestations_insert; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestations_insert ON public.manifestations FOR INSERT TO reverie_app WITH CHECK ((EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));


--
-- Name: manifestations manifestations_select_adult; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestations_select_adult ON public.manifestations FOR SELECT TO reverie_app, reverie_readonly USING ((EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));


--
-- Name: manifestations manifestations_select_child; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestations_select_child ON public.manifestations FOR SELECT TO reverie_app, reverie_readonly USING (((EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = 'child'::public.user_role)))) AND (EXISTS ( SELECT 1
   FROM (public.shelf_items si
     JOIN public.shelves s ON ((s.id = si.shelf_id)))
  WHERE ((si.manifestation_id = manifestations.id) AND (s.user_id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid))))));


--
-- Name: manifestations manifestations_select_system; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestations_select_system ON public.manifestations FOR SELECT TO reverie_app USING ((( SELECT current_setting('app.system_context'::text, true) AS current_setting) = 'writeback'::text));


--
-- Name: manifestations manifestations_update; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestations_update ON public.manifestations FOR UPDATE TO reverie_app USING ((EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))) WITH CHECK (true);


--
-- Name: manifestations manifestations_update_system; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY manifestations_update_system ON public.manifestations FOR UPDATE TO reverie_app USING ((( SELECT current_setting('app.system_context'::text, true) AS current_setting) = 'writeback'::text)) WITH CHECK (true);


--
-- Name: reading_sessions; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.reading_sessions ENABLE ROW LEVEL SECURITY;

--
-- Name: reading_state; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.reading_state ENABLE ROW LEVEL SECURITY;

--
-- Name: reading_state reading_state_owner; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY reading_state_owner ON public.reading_state TO reverie_app, reverie_readonly USING ((user_id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid)) WITH CHECK ((user_id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid));


--
-- Name: user_preferences; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.user_preferences ENABLE ROW LEVEL SECURITY;

--
-- Name: user_preferences user_preferences_owner; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY user_preferences_owner ON public.user_preferences TO reverie_app, reverie_readonly USING ((user_id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid)) WITH CHECK ((user_id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid));


--
-- Name: webhook_deliveries; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.webhook_deliveries ENABLE ROW LEVEL SECURITY;

--
-- Name: webhooks; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.webhooks ENABLE ROW LEVEL SECURITY;

--
-- Name: work_external_identifiers; Type: ROW SECURITY; Schema: public; Owner: reverie_migrator
--

ALTER TABLE public.work_external_identifiers ENABLE ROW LEVEL SECURITY;

--
-- Name: work_external_identifiers work_external_identifiers_delete; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY work_external_identifiers_delete ON public.work_external_identifiers FOR DELETE TO reverie_app USING (((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.work_id = work_external_identifiers.work_id))) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: work_external_identifiers work_external_identifiers_ingestion_full_access; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY work_external_identifiers_ingestion_full_access ON public.work_external_identifiers TO reverie_ingestion USING (true) WITH CHECK (true);


--
-- Name: work_external_identifiers work_external_identifiers_insert; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY work_external_identifiers_insert ON public.work_external_identifiers FOR INSERT TO reverie_app WITH CHECK (((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.work_id = work_external_identifiers.work_id))) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: work_external_identifiers work_external_identifiers_select; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY work_external_identifiers_select ON public.work_external_identifiers FOR SELECT TO reverie_app, reverie_readonly USING ((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.work_id = work_external_identifiers.work_id))));


--
-- Name: work_external_identifiers work_external_identifiers_update; Type: POLICY; Schema: public; Owner: reverie_migrator
--

CREATE POLICY work_external_identifiers_update ON public.work_external_identifiers FOR UPDATE TO reverie_app USING (((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.work_id = work_external_identifiers.work_id))) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))))) WITH CHECK (((EXISTS ( SELECT 1
   FROM public.manifestations m
  WHERE (m.work_id = work_external_identifiers.work_id))) AND (EXISTS ( SELECT 1
   FROM public.users
  WHERE ((users.id = (( SELECT current_setting('app.current_user_id'::text, true) AS current_setting))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role])))))));


--
-- Name: SCHEMA public; Type: ACL; Schema: -; Owner: pg_database_owner
--

GRANT ALL ON SCHEMA public TO reverie_migrator;


--
-- Name: SCHEMA tower_sessions; Type: ACL; Schema: -; Owner: reverie_migrator
--

GRANT USAGE ON SCHEMA tower_sessions TO reverie_app;
GRANT USAGE ON SCHEMA tower_sessions TO reverie_readonly;


--
-- Name: TABLE _sqlx_migrations; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT ON TABLE public._sqlx_migrations TO reverie_app;


--
-- Name: TABLE api_cache; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.api_cache TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.api_cache TO reverie_ingestion;
GRANT SELECT ON TABLE public.api_cache TO reverie_readonly;


--
-- Name: TABLE authors; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.authors TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.authors TO reverie_ingestion;
GRANT SELECT ON TABLE public.authors TO reverie_readonly;


--
-- Name: TABLE device_tokens; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.device_tokens TO reverie_app;


--
-- Name: TABLE field_locks; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.field_locks TO reverie_app;
GRANT SELECT ON TABLE public.field_locks TO reverie_readonly;
GRANT SELECT ON TABLE public.field_locks TO reverie_ingestion;


--
-- Name: TABLE genres; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.genres TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.genres TO reverie_ingestion;
GRANT SELECT ON TABLE public.genres TO reverie_readonly;


--
-- Name: TABLE identifier_schemes; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT ON TABLE public.identifier_schemes TO reverie_app;
GRANT SELECT ON TABLE public.identifier_schemes TO reverie_ingestion;
GRANT SELECT ON TABLE public.identifier_schemes TO reverie_readonly;


--
-- Name: TABLE ingestion_jobs; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.ingestion_jobs TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.ingestion_jobs TO reverie_ingestion;
GRANT SELECT ON TABLE public.ingestion_jobs TO reverie_readonly;


--
-- Name: TABLE instance_bootstrap; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT ON TABLE public.instance_bootstrap TO reverie_app;


--
-- Name: TABLE local_credentials; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.local_credentials TO reverie_app;


--
-- Name: TABLE local_login_throttle; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.local_login_throttle TO reverie_app;


--
-- Name: TABLE manifestation_external_identifiers; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_external_identifiers TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_external_identifiers TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_external_identifiers TO reverie_readonly;


--
-- Name: TABLE manifestation_external_ratings; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT ON TABLE public.manifestation_external_ratings TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_external_ratings TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_external_ratings TO reverie_readonly;


--
-- Name: TABLE manifestation_genres; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_genres TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_genres TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_genres TO reverie_readonly;


--
-- Name: TABLE manifestation_moods; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_moods TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_moods TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_moods TO reverie_readonly;


--
-- Name: TABLE manifestation_tags; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_tags TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_tags TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_tags TO reverie_readonly;


--
-- Name: TABLE manifestations; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestations TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestations TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestations TO reverie_readonly;


--
-- Name: TABLE metadata_sources; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT ON TABLE public.metadata_sources TO reverie_app;
GRANT SELECT ON TABLE public.metadata_sources TO reverie_ingestion;
GRANT SELECT ON TABLE public.metadata_sources TO reverie_readonly;


--
-- Name: TABLE metadata_versions; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.metadata_versions TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.metadata_versions TO reverie_ingestion;
GRANT SELECT ON TABLE public.metadata_versions TO reverie_readonly;


--
-- Name: TABLE moods; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.moods TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.moods TO reverie_ingestion;
GRANT SELECT ON TABLE public.moods TO reverie_readonly;


--
-- Name: TABLE omnibus_contents; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.omnibus_contents TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.omnibus_contents TO reverie_ingestion;
GRANT SELECT ON TABLE public.omnibus_contents TO reverie_readonly;


--
-- Name: TABLE password_reset_pins; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.password_reset_pins TO reverie_app;


--
-- Name: TABLE rating_sources; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT ON TABLE public.rating_sources TO reverie_app;
GRANT SELECT ON TABLE public.rating_sources TO reverie_ingestion;
GRANT SELECT ON TABLE public.rating_sources TO reverie_readonly;


--
-- Name: TABLE reading_sessions; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.reading_sessions TO reverie_app;
GRANT SELECT ON TABLE public.reading_sessions TO reverie_readonly;


--
-- Name: TABLE reading_state; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.reading_state TO reverie_app;
GRANT SELECT ON TABLE public.reading_state TO reverie_readonly;


--
-- Name: TABLE series; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.series TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.series TO reverie_ingestion;
GRANT SELECT ON TABLE public.series TO reverie_readonly;


--
-- Name: TABLE series_works; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.series_works TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.series_works TO reverie_ingestion;
GRANT SELECT ON TABLE public.series_works TO reverie_readonly;


--
-- Name: TABLE settings; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,UPDATE ON TABLE public.settings TO reverie_app;
GRANT SELECT ON TABLE public.settings TO reverie_readonly;


--
-- Name: TABLE shelf_items; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.shelf_items TO reverie_app;
GRANT SELECT ON TABLE public.shelf_items TO reverie_readonly;


--
-- Name: TABLE shelves; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.shelves TO reverie_app;
GRANT SELECT ON TABLE public.shelves TO reverie_readonly;


--
-- Name: TABLE tags; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.tags TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.tags TO reverie_ingestion;
GRANT SELECT ON TABLE public.tags TO reverie_readonly;


--
-- Name: TABLE user_identities; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.user_identities TO reverie_app;
GRANT SELECT ON TABLE public.user_identities TO reverie_readonly;


--
-- Name: TABLE user_preferences; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,UPDATE ON TABLE public.user_preferences TO reverie_app;
GRANT SELECT ON TABLE public.user_preferences TO reverie_readonly;


--
-- Name: TABLE users; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.users TO reverie_app;
GRANT SELECT ON TABLE public.users TO reverie_readonly;


--
-- Name: TABLE webhook_deliveries; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.webhook_deliveries TO reverie_app;
GRANT SELECT ON TABLE public.webhook_deliveries TO reverie_readonly;


--
-- Name: TABLE webhook_event_dedupe; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.webhook_event_dedupe TO reverie_app;
GRANT SELECT ON TABLE public.webhook_event_dedupe TO reverie_readonly;


--
-- Name: TABLE webhooks; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.webhooks TO reverie_app;
GRANT SELECT ON TABLE public.webhooks TO reverie_readonly;


--
-- Name: TABLE work_authors; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.work_authors TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.work_authors TO reverie_ingestion;
GRANT SELECT ON TABLE public.work_authors TO reverie_readonly;


--
-- Name: TABLE work_external_identifiers; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.work_external_identifiers TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.work_external_identifiers TO reverie_ingestion;
GRANT SELECT ON TABLE public.work_external_identifiers TO reverie_readonly;


--
-- Name: TABLE works; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.works TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.works TO reverie_ingestion;
GRANT SELECT ON TABLE public.works TO reverie_readonly;


--
-- Name: TABLE writeback_jobs; Type: ACL; Schema: public; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.writeback_jobs TO reverie_app;
GRANT SELECT,INSERT ON TABLE public.writeback_jobs TO reverie_ingestion;
GRANT SELECT ON TABLE public.writeback_jobs TO reverie_readonly;


--
-- Name: TABLE session; Type: ACL; Schema: tower_sessions; Owner: reverie_migrator
--

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE tower_sessions.session TO reverie_app;


--
-- Name: COLUMN session.expiry_date; Type: ACL; Schema: tower_sessions; Owner: reverie_migrator
--

GRANT SELECT(expiry_date) ON TABLE tower_sessions.session TO reverie_readonly;


--
-- PostgreSQL database dump complete
--

\unrestrict reverie

