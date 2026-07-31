-- Accent-insensitive matching for every fuzzy/contains/similarity text
-- surface (search, suggest, typed filters). Stored values stay accented;
-- folding happens at match time on both sides of each comparison.

CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA public;

-- unaccent() is STABLE, not IMMUTABLE (Postgres cannot prove the installed
-- dictionary never changes), so it cannot back an expression index directly.
-- The dictionary is fixed by the extension version, so treating it as
-- immutable is the standard, documented workaround: a thin SQL wrapper that
-- pins the dictionary explicitly, which both the expression indexes below and
-- their matching queries call so the expressions stay index-eligible.
CREATE FUNCTION public.immutable_unaccent(text)
    RETURNS text
    LANGUAGE sql
    IMMUTABLE
    PARALLEL SAFE
    STRICT
    AS $$
        SELECT public.unaccent('public.unaccent', $1)
    $$;

-- Text search configuration chaining unaccent before the english stemmer, so
-- to_tsvector/to_tsquery fold accents the same way immutable_unaccent folds
-- trigram input. Token types per the unaccent extension's documented recipe:
-- `word`/`hword`/`hword_part` are the only token classes that can carry a
-- non-ASCII letter, so the ascii-only classes keep the plain english mapping.
CREATE TEXT SEARCH CONFIGURATION public.unaccent_english (COPY = pg_catalog.english);
ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ALTER MAPPING FOR hword, hword_part, word
    WITH unaccent, english_stem;

CREATE OR REPLACE FUNCTION public.works_search_vector_update() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.search_vector := to_tsvector('unaccent_english', COALESCE(NEW.title, '') || ' ' || COALESCE(NEW.description, ''));
    RETURN NEW;
END;
$$;

-- Backfill: the trigger only fires on INSERT or UPDATE OF title/description,
-- so existing rows need an explicit recompute under the new configuration.
UPDATE public.works
   SET search_vector = to_tsvector('unaccent_english', COALESCE(title, '') || ' ' || COALESCE(description, ''));

REINDEX INDEX public.idx_works_search_vector;

-- Replace the raw-column trigram indexes with folded ones: a folded query
-- expression only rides an index built on that same expression. REPLACE, not
-- duplicate — the raw indexes go dead once every caller matches folded.
DROP INDEX IF EXISTS public.idx_authors_name_trgm;
CREATE INDEX idx_authors_name_trgm
    ON public.authors USING gist (public.immutable_unaccent(name) public.gist_trgm_ops);

DROP INDEX IF EXISTS public.idx_series_name_trgm;
CREATE INDEX idx_series_name_trgm
    ON public.series USING gist (public.immutable_unaccent(name) public.gist_trgm_ops);

DROP INDEX IF EXISTS public.idx_works_title_trgm;
CREATE INDEX idx_works_title_trgm
    ON public.works USING gist (public.immutable_unaccent(title) public.gist_trgm_ops);

DROP INDEX IF EXISTS public.idx_genres_name_trgm;
CREATE INDEX idx_genres_name_trgm
    ON public.genres USING gin (public.immutable_unaccent(name) public.gin_trgm_ops);

DROP INDEX IF EXISTS public.idx_moods_name_trgm;
CREATE INDEX idx_moods_name_trgm
    ON public.moods USING gin (public.immutable_unaccent(name) public.gin_trgm_ops);

DROP INDEX IF EXISTS public.idx_tags_name_trgm;
CREATE INDEX idx_tags_name_trgm
    ON public.tags USING gin (public.immutable_unaccent(name) public.gin_trgm_ops);

DROP INDEX IF EXISTS public.idx_manifestations_publisher_trgm;
CREATE INDEX idx_manifestations_publisher_trgm
    ON public.manifestations USING gin (public.immutable_unaccent(publisher) public.gin_trgm_ops)
    WHERE (publisher IS NOT NULL);

DROP INDEX IF EXISTS public.idx_works_subtitle_trgm;
CREATE INDEX idx_works_subtitle_trgm
    ON public.works USING gin (public.immutable_unaccent(subtitle) public.gin_trgm_ops);
