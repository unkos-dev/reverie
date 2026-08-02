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
-- OR REPLACE (here and below): the down migration deliberately preserves the
-- wrappers and the text search configuration (see its header), so a
-- revert-then-reapply must tolerate the leftovers.
CREATE OR REPLACE FUNCTION public.immutable_unaccent(text)
    RETURNS text
    LANGUAGE sql
    IMMUTABLE
    PARALLEL SAFE
    STRICT
    AS $$
        SELECT public.unaccent('public.unaccent', $1)
    $$;

-- LIKE-pattern companion: fold first, then backslash-escape `\`, `%`, and
-- `_`. The unaccent dictionary maps several Unicode characters (fullwidth and
-- small-form punctuation) INTO those metacharacters, so escaping done before
-- folding is undone by it; the only safe order is fold-then-escape, and only
-- SQL sees the folded text. Callers bind the raw needle, wrap the result in
-- their own live wildcards, and must not pre-escape.
CREATE OR REPLACE FUNCTION public.immutable_unaccent_like(text)
    RETURNS text
    LANGUAGE sql
    IMMUTABLE
    PARALLEL SAFE
    STRICT
    AS $$
        SELECT regexp_replace(public.immutable_unaccent($1), '([\\%_])', '\\\1', 'g')
    $$;

-- Text search configuration chaining unaccent ahead of each class's base
-- dictionary, so to_tsvector/to_tsquery fold accents the same way
-- immutable_unaccent folds trigram input. Six token classes can carry a
-- non-ASCII letter: the pure-letter `word`/`hword`/`hword_part` (the
-- unaccent extension's documented recipe) and their digit-carrying
-- `numword`/`numhword`/`hword_numpart` siblings, which the default parser
-- splits off whenever a token mixes letters and digits ('1ère', 'Café2').
-- Each keeps its base-config dictionary after the fold: english_stem for the
-- letter classes, simple for the numeric ones, exactly as pg_catalog.english
-- maps them. The ascii-only classes cannot fold and keep the plain mapping.
DROP TEXT SEARCH CONFIGURATION IF EXISTS public.unaccent_english;
CREATE TEXT SEARCH CONFIGURATION public.unaccent_english (COPY = pg_catalog.english);
ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ALTER MAPPING FOR hword, hword_part, word
    WITH unaccent, english_stem;
ALTER TEXT SEARCH CONFIGURATION public.unaccent_english
    ALTER MAPPING FOR numword, numhword, hword_numpart
    WITH unaccent, simple;

-- The pinned search_path keeps the trigger correct under any caller session:
-- unlike the pg_catalog-resolved 'english', both unaccent_english and the
-- unaccent function it chains live in public, so resolution must not depend
-- on the writing session's search_path.
CREATE OR REPLACE FUNCTION public.works_search_vector_update() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path = pg_catalog, public
    AS $$
BEGIN
    NEW.search_vector := to_tsvector('public.unaccent_english', COALESCE(NEW.title, '') || ' ' || COALESCE(NEW.description, ''));
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
