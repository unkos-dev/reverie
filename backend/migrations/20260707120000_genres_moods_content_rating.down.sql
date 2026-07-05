-- Reverse of genres/moods/content_rating.
-- Collapsed case-duplicate tag rows and their junction re-pointing are not
-- reconstructable; the down path restores schema shape only.

DROP INDEX public.idx_manifestations_publisher_trgm;
DROP INDEX public.idx_tags_name_trgm;
DROP INDEX public.idx_moods_name_trgm;
DROP INDEX public.idx_genres_name_trgm;

DROP INDEX public.idx_tags_name_lower;

CREATE TYPE public.tag_type AS ENUM (
    'genre',
    'sub_genre',
    'trope',
    'theme'
);

ALTER TABLE public.tags ADD COLUMN tag_type public.tag_type;
UPDATE public.tags SET tag_type = 'theme';
ALTER TABLE public.tags ALTER COLUMN tag_type SET NOT NULL;
ALTER TABLE public.tags
    ADD CONSTRAINT tags_name_tag_type_key UNIQUE (name, tag_type);

DROP TABLE public.manifestation_moods;
DROP TABLE public.manifestation_genres;
DROP TABLE public.moods;
DROP TABLE public.genres;

ALTER TABLE public.manifestations
    DROP CONSTRAINT manifestations_content_rating_version_id_fkey,
    DROP COLUMN content_rating_version_id,
    DROP COLUMN content_rating;

DROP TYPE public.content_rating;
