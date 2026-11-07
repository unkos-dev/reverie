DROP TRIGGER metadata_versions_metadata_reference ON public.metadata_versions;
DROP TRIGGER identifier_schemes_metadata_reference ON public.identifier_schemes;
DROP TRIGGER tags_metadata_reference ON public.tags;
DROP TRIGGER moods_metadata_reference ON public.moods;
DROP TRIGGER genres_metadata_reference ON public.genres;
DROP TRIGGER authors_metadata_reference ON public.authors;
DROP TRIGGER manifestation_tags_metadata_delete ON public.manifestation_tags;
DROP TRIGGER manifestation_tags_metadata_update ON public.manifestation_tags;
DROP TRIGGER manifestation_tags_metadata_insert ON public.manifestation_tags;
DROP TRIGGER manifestation_moods_metadata_delete ON public.manifestation_moods;
DROP TRIGGER manifestation_moods_metadata_update ON public.manifestation_moods;
DROP TRIGGER manifestation_moods_metadata_insert ON public.manifestation_moods;
DROP TRIGGER manifestation_genres_metadata_delete ON public.manifestation_genres;
DROP TRIGGER manifestation_genres_metadata_update ON public.manifestation_genres;
DROP TRIGGER manifestation_genres_metadata_insert ON public.manifestation_genres;
DROP TRIGGER manifestation_external_identifiers_metadata_delete ON public.manifestation_external_identifiers;
DROP TRIGGER manifestation_external_identifiers_metadata_update ON public.manifestation_external_identifiers;
DROP TRIGGER manifestation_external_identifiers_metadata_insert ON public.manifestation_external_identifiers;
DROP TRIGGER work_external_identifiers_metadata_delete ON public.work_external_identifiers;
DROP TRIGGER work_external_identifiers_metadata_update ON public.work_external_identifiers;
DROP TRIGGER work_external_identifiers_metadata_insert ON public.work_external_identifiers;
DROP TRIGGER work_authors_metadata_delete ON public.work_authors;
DROP TRIGGER work_authors_metadata_update ON public.work_authors;
DROP TRIGGER work_authors_metadata_insert ON public.work_authors;
DROP TRIGGER manifestations_metadata_scalar ON public.manifestations;
DROP TRIGGER works_metadata_scalar ON public.works;
DROP FUNCTION public.metadata_provenance_revision();
DROP FUNCTION public.metadata_reference_revision();
DROP FUNCTION public.metadata_relation_revision();
DROP FUNCTION public.advance_metadata_revisions(jsonb);
DROP FUNCTION public.metadata_scalar_revision();

ALTER TABLE public.works
DROP COLUMN metadata_field_revisions,
DROP COLUMN metadata_revision;
GRANT INSERT, UPDATE ON public.works TO reverie_app, reverie_ingestion;

ALTER TABLE public.manifestations
DROP COLUMN metadata_field_revisions,
DROP COLUMN metadata_revision;
GRANT INSERT, UPDATE ON public.manifestations TO reverie_app, reverie_ingestion;
