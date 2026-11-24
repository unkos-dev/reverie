ALTER TABLE public.works ADD COLUMN metadata_field_revisions jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE public.manifestations ADD COLUMN metadata_field_revisions jsonb NOT NULL DEFAULT '{}'::jsonb;

CREATE OR REPLACE FUNCTION public.metadata_scalar_revision() RETURNS trigger
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
CREATE OR REPLACE FUNCTION public.advance_metadata_revisions(affected jsonb) RETURNS void
LANGUAGE plpgsql STRICT SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
BEGIN
    PERFORM m.id FROM public.manifestations m
    WHERE m.id IN (
        SELECT owner_id
        FROM jsonb_to_recordset(affected) AS field(owner_kind text, owner_id uuid, field_name text)
        WHERE owner_kind = 'manifestation'
    )
    ORDER BY m.id FOR UPDATE OF m;
    PERFORM w.id FROM public.works w
    WHERE w.id IN (
        SELECT owner_id
        FROM jsonb_to_recordset(affected) AS field(owner_kind text, owner_id uuid, field_name text)
        WHERE owner_kind = 'work'
    )
    ORDER BY w.id FOR UPDATE OF w;

    WITH changed AS (
        SELECT owner_id, array_agg(DISTINCT field_name ORDER BY field_name) AS fields
        FROM jsonb_to_recordset(affected) AS field(owner_kind text, owner_id uuid, field_name text)
        WHERE owner_kind = 'manifestation'
        GROUP BY owner_id
    )
    UPDATE public.manifestations AS m
    SET metadata_revision = m.metadata_revision + 1,
        metadata_field_revisions = m.metadata_field_revisions || (
            SELECT jsonb_object_agg(field_name, m.metadata_revision + 1)
            FROM unnest(changed.fields) AS field_name
        )
    FROM changed WHERE m.id = changed.owner_id;

    WITH changed AS (
        SELECT owner_id, array_agg(DISTINCT field_name ORDER BY field_name) AS fields
        FROM jsonb_to_recordset(affected) AS field(owner_kind text, owner_id uuid, field_name text)
        WHERE owner_kind = 'work'
        GROUP BY owner_id
    )
    UPDATE public.works AS w
    SET metadata_revision = w.metadata_revision + 1,
        metadata_field_revisions = w.metadata_field_revisions || (
            SELECT jsonb_object_agg(field_name, w.metadata_revision + 1)
            FROM unnest(changed.fields) AS field_name
        )
    FROM changed WHERE w.id = changed.owner_id;
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
