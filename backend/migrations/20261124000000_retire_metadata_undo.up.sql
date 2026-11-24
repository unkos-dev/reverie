DROP FUNCTION public.reclaim_metadata_undo();
DROP TABLE public.metadata_undo_operations;

CREATE OR REPLACE FUNCTION public.metadata_scalar_revision() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, public, pg_temp AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.metadata_revision := 0;
        RETURN NEW;
    END IF;

    IF EXISTS (
        SELECT 1 FROM unnest(TG_ARGV) AS specification
        CROSS JOIN LATERAL jsonb_each(specification::jsonb) AS field
        WHERE EXISTS (
            SELECT 1 FROM jsonb_array_elements_text(field.value) AS column_name
            WHERE to_jsonb(OLD) -> column_name IS DISTINCT FROM to_jsonb(NEW) -> column_name
        )
    ) THEN
        NEW.metadata_revision := OLD.metadata_revision + 1;
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
        FROM jsonb_to_recordset(affected) AS owner(owner_kind text, owner_id uuid)
        WHERE owner_kind = 'manifestation'
    )
    ORDER BY m.id FOR UPDATE OF m;
    PERFORM w.id FROM public.works w
    WHERE w.id IN (
        SELECT owner_id
        FROM jsonb_to_recordset(affected) AS owner(owner_kind text, owner_id uuid)
        WHERE owner_kind = 'work'
    )
    ORDER BY w.id FOR UPDATE OF w;

    UPDATE public.manifestations AS m
    SET metadata_revision = m.metadata_revision + 1
    WHERE m.id IN (
        SELECT owner_id
        FROM jsonb_to_recordset(affected) AS owner(owner_kind text, owner_id uuid)
        WHERE owner_kind = 'manifestation'
    );

    UPDATE public.works AS w
    SET metadata_revision = w.metadata_revision + 1
    WHERE w.id IN (
        SELECT owner_id
        FROM jsonb_to_recordset(affected) AS owner(owner_kind text, owner_id uuid)
        WHERE owner_kind = 'work'
    );
END;
$$;

ALTER TABLE public.works DROP COLUMN metadata_field_revisions;
ALTER TABLE public.manifestations DROP COLUMN metadata_field_revisions;
