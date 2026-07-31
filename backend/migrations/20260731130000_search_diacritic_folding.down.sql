DROP INDEX IF EXISTS public.idx_works_subtitle_trgm;
CREATE INDEX idx_works_subtitle_trgm
    ON public.works USING gin (subtitle public.gin_trgm_ops);

DROP INDEX IF EXISTS public.idx_manifestations_publisher_trgm;
CREATE INDEX idx_manifestations_publisher_trgm
    ON public.manifestations USING gin (publisher public.gin_trgm_ops)
    WHERE (publisher IS NOT NULL);

DROP INDEX IF EXISTS public.idx_tags_name_trgm;
CREATE INDEX idx_tags_name_trgm ON public.tags USING gin (name public.gin_trgm_ops);

DROP INDEX IF EXISTS public.idx_moods_name_trgm;
CREATE INDEX idx_moods_name_trgm ON public.moods USING gin (name public.gin_trgm_ops);

DROP INDEX IF EXISTS public.idx_genres_name_trgm;
CREATE INDEX idx_genres_name_trgm ON public.genres USING gin (name public.gin_trgm_ops);

DROP INDEX IF EXISTS public.idx_works_title_trgm;
CREATE INDEX idx_works_title_trgm ON public.works USING gist (title public.gist_trgm_ops);

DROP INDEX IF EXISTS public.idx_series_name_trgm;
CREATE INDEX idx_series_name_trgm ON public.series USING gist (name public.gist_trgm_ops);

DROP INDEX IF EXISTS public.idx_authors_name_trgm;
CREATE INDEX idx_authors_name_trgm ON public.authors USING gist (name public.gist_trgm_ops);

CREATE OR REPLACE FUNCTION public.works_search_vector_update() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.search_vector := to_tsvector('english', COALESCE(NEW.title, '') || ' ' || COALESCE(NEW.description, ''));
    RETURN NEW;
END;
$$;

UPDATE public.works
   SET search_vector = to_tsvector('english', COALESCE(title, '') || ' ' || COALESCE(description, ''));

REINDEX INDEX public.idx_works_search_vector;

DROP TEXT SEARCH CONFIGURATION public.unaccent_english;

DROP FUNCTION public.immutable_unaccent(text);

DROP EXTENSION IF EXISTS unaccent;
