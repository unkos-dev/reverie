ALTER TABLE public.works
ADD COLUMN metadata_revision bigint NOT NULL DEFAULT 0 CHECK (metadata_revision >= 0),
ADD COLUMN metadata_field_revisions jsonb NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE public.manifestations
ADD COLUMN metadata_revision bigint NOT NULL DEFAULT 0 CHECK (metadata_revision >= 0),
ADD COLUMN metadata_field_revisions jsonb NOT NULL DEFAULT '{}'::jsonb;

CREATE FUNCTION public.metadata_scalar_revision() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    changed_fields text[];
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.metadata_revision := 0;
        NEW.metadata_field_revisions := '{}'::jsonb;
        RETURN NEW;
    END IF;

    SELECT array_agg(field.key ORDER BY field.key) INTO changed_fields
    FROM unnest(TG_ARGV) AS specification
    CROSS JOIN LATERAL jsonb_each(specification::jsonb) AS field
    WHERE EXISTS (
        SELECT 1 FROM jsonb_array_elements_text(field.value) AS column_name
        WHERE to_jsonb(OLD) -> column_name IS DISTINCT FROM to_jsonb(NEW) -> column_name
    );
    IF changed_fields IS NOT NULL THEN
        NEW.metadata_revision := OLD.metadata_revision + 1;
        NEW.metadata_field_revisions := OLD.metadata_field_revisions || (
            SELECT jsonb_object_agg(field_name, NEW.metadata_revision)
            FROM unnest(changed_fields) AS field_name
        );
    END IF;
    RETURN NEW;
END;
$$;

-- THREAT: Only installed triggers may advance identities across policy-hidden sibling editions.
CREATE FUNCTION public.advance_metadata_revisions(affected jsonb) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    parent record;
BEGIN
    FOR parent IN
        SELECT owner_id, array_agg(DISTINCT field_name ORDER BY field_name) AS fields
        FROM jsonb_to_recordset(affected) AS field(owner_kind text, owner_id uuid, field_name text)
        WHERE owner_kind = 'manifestation'
        GROUP BY owner_id ORDER BY owner_id
    LOOP
        UPDATE public.manifestations AS m
        SET metadata_revision = m.metadata_revision + 1,
            metadata_field_revisions = m.metadata_field_revisions || (
                SELECT jsonb_object_agg(field_name, m.metadata_revision + 1)
                FROM unnest(parent.fields) AS field_name
            )
        WHERE m.id = parent.owner_id;
    END LOOP;
    FOR parent IN
        SELECT owner_id, array_agg(DISTINCT field_name ORDER BY field_name) AS fields
        FROM jsonb_to_recordset(affected) AS field(owner_kind text, owner_id uuid, field_name text)
        WHERE owner_kind = 'work'
        GROUP BY owner_id ORDER BY owner_id
    LOOP
        UPDATE public.works AS w
        SET metadata_revision = w.metadata_revision + 1,
            metadata_field_revisions = w.metadata_field_revisions || (
                SELECT jsonb_object_agg(field_name, w.metadata_revision + 1)
                FROM unnest(parent.fields) AS field_name
            )
        WHERE w.id = parent.owner_id;
    END LOOP;
END;
$$;

CREATE FUNCTION public.metadata_relation_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    changed_rows jsonb;
    affected jsonb;
BEGIN
    IF TG_OP = 'INSERT' THEN
        SELECT jsonb_agg(to_jsonb(row_value)) INTO changed_rows FROM new_rows AS row_value;
    ELSIF TG_OP = 'DELETE' THEN
        SELECT jsonb_agg(to_jsonb(row_value)) INTO changed_rows FROM old_rows AS row_value;
    ELSE
        SELECT jsonb_agg(to_jsonb(row_value)) INTO changed_rows
        FROM (
            (SELECT * FROM old_rows EXCEPT SELECT * FROM new_rows)
            UNION ALL
            (SELECT * FROM new_rows EXCEPT SELECT * FROM old_rows)
        ) AS row_value;
    END IF;
    SELECT jsonb_agg(jsonb_build_object(
        'owner_kind', TG_ARGV[0], 'owner_id', row_value ->> TG_ARGV[1],
        'field_name', CASE TG_ARGV[2]
            WHEN 'contributors' THEN 'contributors.' || (row_value ->> 'role')
            WHEN 'identifiers' THEN 'identifiers.' || TG_ARGV[0] || '.' || (row_value ->> 'scheme')
            ELSE TG_ARGV[2]
        END
    )) INTO affected
    FROM jsonb_array_elements(changed_rows) AS row_value;
    PERFORM public.advance_metadata_revisions(affected);
    RETURN NULL;
END;
$$;

CREATE FUNCTION public.metadata_reference_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    changed_ids uuid[];
    changed_schemes text[];
    affected jsonb;
BEGIN
    IF TG_TABLE_NAME = 'identifier_schemes' THEN
        SELECT array_agg(o.id ORDER BY o.id) INTO changed_schemes
        FROM old_rows o JOIN new_rows n USING (id)
        WHERE o.display_name IS DISTINCT FROM n.display_name;
        SELECT jsonb_agg(to_jsonb(field)) INTO affected FROM (
            SELECT 'manifestation' AS owner_kind, manifestation_id AS owner_id,
                   'identifiers.manifestation.' || scheme AS field_name
            FROM public.manifestation_external_identifiers WHERE scheme = ANY(changed_schemes)
            UNION ALL
            SELECT 'work', work_id, 'identifiers.work.' || scheme
            FROM public.work_external_identifiers WHERE scheme = ANY(changed_schemes)
        ) AS field;
    ELSE
        SELECT array_agg(o.id ORDER BY o.id) INTO changed_ids
        FROM old_rows o JOIN new_rows n USING (id)
        WHERE o.name IS DISTINCT FROM n.name
           OR (TG_TABLE_NAME = 'authors' AND to_jsonb(o) -> 'sort_name' IS DISTINCT FROM to_jsonb(n) -> 'sort_name');
        CASE TG_TABLE_NAME
            WHEN 'authors' THEN
                SELECT jsonb_agg(jsonb_build_object(
                    'owner_kind', 'work', 'owner_id', work_id, 'field_name', 'contributors.' || role::text
                )) INTO affected FROM public.work_authors WHERE author_id = ANY(changed_ids);
            WHEN 'genres' THEN
                SELECT jsonb_agg(jsonb_build_object(
                    'owner_kind', 'manifestation', 'owner_id', manifestation_id, 'field_name', 'genres'
                )) INTO affected FROM public.manifestation_genres WHERE genre_id = ANY(changed_ids);
            WHEN 'moods' THEN
                SELECT jsonb_agg(jsonb_build_object(
                    'owner_kind', 'manifestation', 'owner_id', manifestation_id, 'field_name', 'moods'
                )) INTO affected FROM public.manifestation_moods WHERE mood_id = ANY(changed_ids);
            WHEN 'tags' THEN
                SELECT jsonb_agg(jsonb_build_object(
                    'owner_kind', 'manifestation', 'owner_id', manifestation_id, 'field_name', 'tags'
                )) INTO affected FROM public.manifestation_tags WHERE tag_id = ANY(changed_ids);
        END CASE;
    END IF;
    PERFORM public.advance_metadata_revisions(affected);
    RETURN NULL;
END;
$$;

CREATE FUNCTION public.metadata_provenance_revision() RETURNS trigger
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
    SELECT jsonb_agg(to_jsonb(field)) INTO affected FROM (
        SELECT 'manifestation' AS owner_kind, m.id AS owner_id, scalar.field_name
        FROM public.manifestations m
        CROSS JOIN LATERAL (VALUES
            ('publisher', m.publisher_version_id), ('pub_date', m.pub_date_version_id),
            ('isbn_10', m.isbn_10_version_id), ('isbn_13', m.isbn_13_version_id),
            ('pages', m.pages_version_id), ('content_rating', m.content_rating_version_id),
            ('cover', m.cover_version_id)
        ) AS scalar(field_name, version_id)
        WHERE scalar.version_id = ANY(changed_ids)
        UNION ALL
        SELECT 'work', w.id, scalar.field_name
        FROM public.works w
        CROSS JOIN LATERAL (VALUES
            ('title', w.title_version_id), ('subtitle', w.subtitle_version_id),
            ('description', w.description_version_id), ('language', w.language_version_id)
        ) AS scalar(field_name, version_id)
        WHERE scalar.version_id = ANY(changed_ids)
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

CREATE TRIGGER works_metadata_scalar
BEFORE INSERT OR UPDATE ON public.works
FOR EACH ROW EXECUTE FUNCTION public.metadata_scalar_revision(
    '{"title":["title","title_version_id","sort_title"]}',
    '{"subtitle":["subtitle","subtitle_version_id"]}',
    '{"description":["description","description_version_id"]}',
    '{"language":["language","language_version_id"]}',
    '{"contributors.author":["first_author_sort_name"]}'
);

CREATE TRIGGER manifestations_metadata_scalar
BEFORE INSERT OR UPDATE ON public.manifestations
FOR EACH ROW EXECUTE FUNCTION public.metadata_scalar_revision(
    '{"publisher":["publisher","publisher_version_id"]}',
    '{"pub_date":["pub_date","pub_date_version_id"]}',
    '{"isbn_10":["isbn_10","isbn_10_version_id"]}',
    '{"isbn_13":["isbn_13","isbn_13_version_id"]}',
    '{"pages":["pages","pages_version_id"]}',
    '{"content_rating":["content_rating","content_rating_version_id"]}',
    '{"cover":["cover_path","cover_sha256","cover_size_bytes","cover_source","cover_version_id"]}',
    '{"work":["work_id"]}'
);

CREATE TRIGGER work_authors_metadata_insert
AFTER INSERT ON public.work_authors
REFERENCING NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('work', 'work_id', 'contributors');

CREATE TRIGGER work_authors_metadata_update
AFTER UPDATE ON public.work_authors
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('work', 'work_id', 'contributors');

CREATE TRIGGER work_authors_metadata_delete
AFTER DELETE ON public.work_authors
REFERENCING OLD TABLE AS old_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('work', 'work_id', 'contributors');

CREATE TRIGGER work_external_identifiers_metadata_insert
AFTER INSERT ON public.work_external_identifiers
REFERENCING NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('work', 'work_id', 'identifiers');

CREATE TRIGGER work_external_identifiers_metadata_update
AFTER UPDATE ON public.work_external_identifiers
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('work', 'work_id', 'identifiers');

CREATE TRIGGER work_external_identifiers_metadata_delete
AFTER DELETE ON public.work_external_identifiers
REFERENCING OLD TABLE AS old_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('work', 'work_id', 'identifiers');

CREATE TRIGGER manifestation_external_identifiers_metadata_insert
AFTER INSERT ON public.manifestation_external_identifiers
REFERENCING NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision(
    'manifestation', 'manifestation_id', 'identifiers'
);

CREATE TRIGGER manifestation_external_identifiers_metadata_update
AFTER UPDATE ON public.manifestation_external_identifiers
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision(
    'manifestation', 'manifestation_id', 'identifiers'
);

CREATE TRIGGER manifestation_external_identifiers_metadata_delete
AFTER DELETE ON public.manifestation_external_identifiers
REFERENCING OLD TABLE AS old_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision(
    'manifestation', 'manifestation_id', 'identifiers'
);

CREATE TRIGGER manifestation_genres_metadata_insert
AFTER INSERT ON public.manifestation_genres
REFERENCING NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'genres');

CREATE TRIGGER manifestation_genres_metadata_update
AFTER UPDATE ON public.manifestation_genres
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'genres');

CREATE TRIGGER manifestation_genres_metadata_delete
AFTER DELETE ON public.manifestation_genres
REFERENCING OLD TABLE AS old_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'genres');

CREATE TRIGGER manifestation_moods_metadata_insert
AFTER INSERT ON public.manifestation_moods
REFERENCING NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'moods');

CREATE TRIGGER manifestation_moods_metadata_update
AFTER UPDATE ON public.manifestation_moods
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'moods');

CREATE TRIGGER manifestation_moods_metadata_delete
AFTER DELETE ON public.manifestation_moods
REFERENCING OLD TABLE AS old_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'moods');

CREATE TRIGGER manifestation_tags_metadata_insert
AFTER INSERT ON public.manifestation_tags
REFERENCING NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'tags');

CREATE TRIGGER manifestation_tags_metadata_update
AFTER UPDATE ON public.manifestation_tags
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'tags');

CREATE TRIGGER manifestation_tags_metadata_delete
AFTER DELETE ON public.manifestation_tags
REFERENCING OLD TABLE AS old_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_relation_revision('manifestation', 'manifestation_id', 'tags');

CREATE TRIGGER authors_metadata_reference
AFTER UPDATE ON public.authors
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_reference_revision();

CREATE TRIGGER genres_metadata_reference
AFTER UPDATE ON public.genres
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_reference_revision();

CREATE TRIGGER moods_metadata_reference
AFTER UPDATE ON public.moods
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_reference_revision();

CREATE TRIGGER tags_metadata_reference
AFTER UPDATE ON public.tags
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_reference_revision();

CREATE TRIGGER identifier_schemes_metadata_reference
AFTER UPDATE ON public.identifier_schemes
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_reference_revision();

CREATE TRIGGER metadata_versions_metadata_reference
AFTER UPDATE ON public.metadata_versions
REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
FOR EACH STATEMENT EXECUTE FUNCTION public.metadata_provenance_revision();

REVOKE ALL ON FUNCTION public.metadata_scalar_revision() FROM public, reverie_app, reverie_ingestion, reverie_readonly;
REVOKE ALL ON FUNCTION public.advance_metadata_revisions(jsonb) FROM public,
reverie_app,
reverie_ingestion,
reverie_readonly;
REVOKE ALL ON FUNCTION public.metadata_relation_revision() FROM public,
reverie_app,
reverie_ingestion,
reverie_readonly;
REVOKE ALL ON FUNCTION public.metadata_reference_revision() FROM public,
reverie_app,
reverie_ingestion,
reverie_readonly;
REVOKE ALL ON FUNCTION public.metadata_provenance_revision() FROM public,
reverie_app,
reverie_ingestion,
reverie_readonly;

REVOKE INSERT, UPDATE ON public.works FROM reverie_app, reverie_ingestion;
GRANT INSERT (
    id,
    title,
    sort_title,
    description,
    language,
    created_at,
    updated_at,
    search_vector,
    title_version_id,
    description_version_id,
    language_version_id,
    subtitle,
    subtitle_version_id,
    first_author_sort_name
) ON public.works TO reverie_app,
reverie_ingestion;
GRANT UPDATE (
    id,
    title,
    sort_title,
    description,
    language,
    created_at,
    updated_at,
    search_vector,
    title_version_id,
    description_version_id,
    language_version_id,
    subtitle,
    subtitle_version_id,
    first_author_sort_name
) ON public.works TO reverie_app,
reverie_ingestion;

REVOKE INSERT, UPDATE ON public.manifestations FROM reverie_app, reverie_ingestion;
GRANT INSERT (
    id,
    work_id,
    isbn_10,
    isbn_13,
    publisher,
    pub_date,
    format,
    file_path,
    ingestion_file_hash,
    file_size_bytes,
    validation_status,
    ingestion_status,
    created_at,
    updated_at,
    accessibility_metadata,
    publisher_version_id,
    pub_date_version_id,
    isbn_10_version_id,
    isbn_13_version_id,
    cover_path,
    cover_sha256,
    cover_size_bytes,
    cover_source,
    cover_version_id,
    suspected_duplicate_work_id,
    enrichment_status,
    enrichment_attempted_at,
    enrichment_attempt_count,
    enrichment_error,
    current_file_hash,
    pages,
    pages_version_id,
    content_rating,
    content_rating_version_id,
    enrichment_rerun_requested,
    has_embedded_cover,
    library_id,
    relocation_source_path,
    relocation_destination_path,
    enrichment_failures,
    enrichment_retry_not_before,
    enrichment_claim_seq
) ON public.manifestations TO reverie_app,
reverie_ingestion;
GRANT UPDATE (
    id,
    work_id,
    isbn_10,
    isbn_13,
    publisher,
    pub_date,
    format,
    file_path,
    ingestion_file_hash,
    file_size_bytes,
    validation_status,
    ingestion_status,
    created_at,
    updated_at,
    accessibility_metadata,
    publisher_version_id,
    pub_date_version_id,
    isbn_10_version_id,
    isbn_13_version_id,
    cover_path,
    cover_sha256,
    cover_size_bytes,
    cover_source,
    cover_version_id,
    suspected_duplicate_work_id,
    enrichment_status,
    enrichment_attempted_at,
    enrichment_attempt_count,
    enrichment_error,
    current_file_hash,
    pages,
    pages_version_id,
    content_rating,
    content_rating_version_id,
    enrichment_rerun_requested,
    has_embedded_cover,
    library_id,
    relocation_source_path,
    relocation_destination_path,
    enrichment_failures,
    enrichment_retry_not_before,
    enrichment_claim_seq
) ON public.manifestations TO reverie_app,
reverie_ingestion;
