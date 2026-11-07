DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM public.metadata_versions
        GROUP BY manifestation_id, source, field_name, value_hash HAVING count(*) > 1
    ) THEN
        RAISE EXCEPTION 'Cannot downgrade metadata observations: distinct observations share a normalised key';
    END IF;
END;
$$;
DROP FUNCTION public.reclaim_metadata_undo();
DROP TABLE public.metadata_undo_operations;
CREATE OR REPLACE FUNCTION public.metadata_provenance_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    changed_ids uuid[];
    affected jsonb;
BEGIN
    SELECT array_agg(o.id ORDER BY o.id) INTO changed_ids
    FROM old_rows o JOIN new_rows n USING (id)
    WHERE (o.manifestation_id, o.source, o.field_name, o.new_value, o.confidence_score, o.match_type, o.resolved_by)
        IS DISTINCT FROM
          (n.manifestation_id, n.source, n.field_name, n.new_value, n.confidence_score, n.match_type, n.resolved_by);
    IF changed_ids IS NULL THEN
        RETURN NULL;
    END IF;
    SELECT jsonb_agg(to_jsonb(field)) INTO affected FROM (
        SELECT 'manifestation' AS owner_kind, id AS owner_id, 'publisher' AS field_name
        FROM public.manifestations WHERE publisher_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation' AS owner_kind, id AS owner_id, 'pub_date' AS field_name
        FROM public.manifestations WHERE pub_date_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation' AS owner_kind, id AS owner_id, 'isbn_10' AS field_name
        FROM public.manifestations WHERE isbn_10_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation' AS owner_kind, id AS owner_id, 'isbn_13' AS field_name
        FROM public.manifestations WHERE isbn_13_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation' AS owner_kind, id AS owner_id, 'pages' AS field_name
        FROM public.manifestations WHERE pages_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation' AS owner_kind, id AS owner_id, 'content_rating' AS field_name
        FROM public.manifestations WHERE content_rating_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation' AS owner_kind, id AS owner_id, 'cover' AS field_name
        FROM public.manifestations WHERE cover_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'work' AS owner_kind, id AS owner_id, 'title' AS field_name
        FROM public.works WHERE title_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'work' AS owner_kind, id AS owner_id, 'subtitle' AS field_name
        FROM public.works WHERE subtitle_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'work' AS owner_kind, id AS owner_id, 'description' AS field_name
        FROM public.works WHERE description_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'work' AS owner_kind, id AS owner_id, 'language' AS field_name
        FROM public.works WHERE language_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'work', work_id, 'contributors.' || role::text
        FROM public.work_authors WHERE source_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'work', work_id, 'identifiers.work.' || scheme
        FROM public.work_external_identifiers WHERE source_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation', manifestation_id, 'identifiers.manifestation.' || scheme
        FROM public.manifestation_external_identifiers WHERE source_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation', manifestation_id, 'genres'
        FROM public.manifestation_genres WHERE source_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation', manifestation_id, 'moods'
        FROM public.manifestation_moods WHERE source_version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'manifestation', manifestation_id, 'tags'
        FROM public.manifestation_tags WHERE source_version_id = ANY(changed_ids)
    ) AS field;
    PERFORM public.advance_metadata_revisions(affected);
    RETURN NULL;
END;
$$;
ALTER TABLE public.metadata_versions DROP CONSTRAINT metadata_versions_observation_unique;
DROP TRIGGER metadata_observation_identity ON public.metadata_versions;
DROP FUNCTION public.assign_metadata_observation_key();
ALTER TABLE public.metadata_versions DROP COLUMN observation_key;
ALTER TABLE public.metadata_versions DROP COLUMN submitted_by;
DROP FUNCTION public.metadata_observation_key(jsonb, text, uuid);
ALTER TABLE public.metadata_versions ADD CONSTRAINT metadata_versions_mfs_hash_unique
UNIQUE (manifestation_id, source, field_name, value_hash);
GRANT UPDATE ON public.metadata_versions TO reverie_app, reverie_ingestion;
