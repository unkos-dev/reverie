ALTER TABLE public.manifestations
    ADD COLUMN enrichment_failures jsonb NOT NULL DEFAULT '[]'
    CHECK (jsonb_typeof(enrichment_failures) = 'array'); -- noqa: CV11
