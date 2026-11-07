ALTER TABLE public.metadata_versions ADD COLUMN submitted_by uuid
REFERENCES public.users (id) ON DELETE SET NULL;
UPDATE public.metadata_versions SET submitted_by = resolved_by
WHERE source = 'manual' AND status = 'pending';

CREATE FUNCTION public.metadata_observation_key(value jsonb, method text, submitter uuid)
RETURNS bytea LANGUAGE sql IMMUTABLE PARALLEL SAFE
SET search_path = pg_catalog AS $$
    SELECT sha256(convert_to(jsonb_build_array(value, method, submitter)::text, 'UTF8'));
$$;

ALTER TABLE public.metadata_versions ADD COLUMN observation_key bytea;
UPDATE public.metadata_versions
SET observation_key = public.metadata_observation_key(new_value, match_type, submitted_by);
ALTER TABLE public.metadata_versions ALTER COLUMN observation_key SET NOT NULL;

CREATE FUNCTION public.assign_metadata_observation_key() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, public, pg_temp AS $$
BEGIN
    NEW.observation_key := public.metadata_observation_key(NEW.new_value, NEW.match_type, NEW.submitted_by);
    RETURN NEW;
END;
$$;
CREATE TRIGGER metadata_observation_identity BEFORE INSERT ON public.metadata_versions
FOR EACH ROW EXECUTE FUNCTION public.assign_metadata_observation_key();
REVOKE ALL ON FUNCTION public.assign_metadata_observation_key() FROM public,
reverie_app, reverie_ingestion, reverie_readonly;
ALTER TABLE public.metadata_versions DROP CONSTRAINT metadata_versions_mfs_hash_unique;
ALTER TABLE public.metadata_versions ADD CONSTRAINT metadata_versions_observation_unique
UNIQUE (manifestation_id, source, field_name, observation_key);

REVOKE UPDATE ON public.metadata_versions FROM reverie_app, reverie_ingestion;
GRANT UPDATE (status, confidence_score, resolved_by, resolved_at, last_seen_at, observation_count)
ON public.metadata_versions TO reverie_app, reverie_ingestion;

CREATE OR REPLACE FUNCTION public.metadata_provenance_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    changed_ids uuid[];
    affected jsonb;
BEGIN
    SELECT array_agg(o.id ORDER BY o.id) INTO changed_ids
    FROM old_rows o JOIN new_rows n USING (id)
    WHERE (o.manifestation_id, o.source, o.field_name, o.new_value, o.confidence_score, o.match_type, o.submitted_by)
        IS DISTINCT FROM
          (n.manifestation_id, n.source, n.field_name, n.new_value, n.confidence_score, n.match_type, n.submitted_by);
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

CREATE TABLE public.metadata_undo_operations (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES public.users (id) ON DELETE CASCADE,
    session_id text NOT NULL REFERENCES tower_sessions.session (id) ON DELETE CASCADE,
    manifestation_id uuid NOT NULL REFERENCES public.manifestations (id) ON DELETE CASCADE,
    work_id uuid NOT NULL REFERENCES public.works (id) ON DELETE CASCADE,
    request_id uuid NOT NULL,
    field_name text NOT NULL,
    before_state jsonb NOT NULL,
    after_state jsonb NOT NULL,
    after_revision bigint NOT NULL CHECK (after_revision >= 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL DEFAULT (clock_timestamp() + interval '1 hour'),
    consumed_at timestamptz,
    superseded_at timestamptz,
    UNIQUE (user_id, session_id, manifestation_id, request_id, field_name)
);
CREATE UNIQUE INDEX metadata_undo_latest ON public.metadata_undo_operations
(user_id, session_id, manifestation_id, field_name)
WHERE consumed_at IS NULL AND superseded_at IS NULL;
CREATE INDEX metadata_undo_expiry ON public.metadata_undo_operations (expires_at);
ALTER TABLE public.metadata_undo_operations ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.metadata_undo_operations FORCE ROW LEVEL SECURITY;
CREATE POLICY metadata_undo_owner ON public.metadata_undo_operations
USING (user_id = nullif(current_setting('app.current_user_id', TRUE), '')::uuid)
WITH CHECK (user_id = nullif(current_setting('app.current_user_id', TRUE), '')::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE ON public.metadata_undo_operations TO reverie_app;
CREATE POLICY metadata_undo_expired_maintenance ON public.metadata_undo_operations TO reverie_migrator
USING (expires_at <= clock_timestamp());
GRANT SELECT, DELETE ON public.metadata_undo_operations TO reverie_migrator;


CREATE FUNCTION public.reclaim_metadata_undo() RETURNS bigint
LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
    WITH expired AS (
        SELECT id FROM public.metadata_undo_operations
        WHERE expires_at <= clock_timestamp() ORDER BY expires_at, id LIMIT 1000
    ), removed AS (
        DELETE FROM public.metadata_undo_operations
        WHERE expires_at <= clock_timestamp() AND id IN (SELECT id FROM expired) RETURNING id
    ) SELECT count(*) FROM removed;
$$;
REVOKE ALL ON FUNCTION public.reclaim_metadata_undo() FROM public, reverie_readonly, reverie_ingestion;
GRANT EXECUTE ON FUNCTION public.reclaim_metadata_undo() TO reverie_app;

ALTER FUNCTION public.reclaim_metadata_undo() OWNER TO reverie_migrator;
