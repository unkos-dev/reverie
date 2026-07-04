-- Reverse of the contributors/subtitle/pages migration. Local-dev
-- reversibility only, not a production rollback; these migrations roll up
-- into the base schema before the first release.
-- The string-array journal shape hashes identically under the old "creators"
-- normaliser, so re-tagged rows keep valid value_hash values.

UPDATE public.metadata_versions
SET field_name = 'creators'
WHERE field_name = 'contributors.author';

DELETE FROM public.metadata_versions WHERE field_name LIKE 'contributors.%';

DROP INDEX public.idx_works_first_author_sort_id;
DROP INDEX public.idx_works_subtitle_version_id;
DROP INDEX public.idx_manifestations_pages_version_id;

ALTER TABLE public.manifestations
    DROP CONSTRAINT manifestations_pages_positive,
    DROP CONSTRAINT manifestations_pages_version_id_fkey,
    DROP COLUMN pages_version_id,
    DROP COLUMN pages;

ALTER TABLE public.works
    DROP CONSTRAINT works_subtitle_version_id_fkey,
    DROP COLUMN first_author_sort_name,
    DROP COLUMN subtitle_version_id,
    DROP COLUMN subtitle;
