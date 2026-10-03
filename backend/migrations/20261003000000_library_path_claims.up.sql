ALTER TABLE public.manifestations
    ADD CONSTRAINT manifestations_id_library_key UNIQUE (id, library_id);

CREATE TABLE public.library_path_claims (
    library_id uuid NOT NULL,
    path text NOT NULL CHECK (
        path <> ''
        AND path !~ '(^/|/$|//|\\|^[A-Za-z]:|(^|/)[.]{1,2}(/|$))'
    ),
    manifestation_id uuid NOT NULL,
    PRIMARY KEY (library_id, path),
    UNIQUE (manifestation_id, library_id, path)
);

CREATE INDEX idx_library_path_claims_owner ON public.library_path_claims (manifestation_id);

INSERT INTO public.library_path_claims (library_id, path, manifestation_id)
SELECT
    library_id,
    file_path,
    id
FROM public.manifestations
UNION
SELECT
    library_id,
    relocation_source_path,
    id
FROM public.manifestations
WHERE relocation_source_path IS NOT NULL
UNION
SELECT
    library_id,
    relocation_destination_path,
    id
FROM public.manifestations
WHERE relocation_destination_path IS NOT NULL;

ALTER TABLE public.library_path_claims
    ADD CONSTRAINT library_path_claims_owner_fk
        FOREIGN KEY (manifestation_id, library_id)
        REFERENCES public.manifestations(id, library_id) ON DELETE CASCADE
        DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE public.manifestations
    ADD CONSTRAINT manifestations_recorded_path_claim_fk
        FOREIGN KEY (library_id, file_path)
        REFERENCES public.library_path_claims(library_id, path)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT manifestations_recorded_owner_claim_fk
        FOREIGN KEY (id, library_id, file_path)
        REFERENCES public.library_path_claims(manifestation_id, library_id, path)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT manifestations_source_owner_claim_fk
        FOREIGN KEY (id, library_id, relocation_source_path)
        REFERENCES public.library_path_claims(manifestation_id, library_id, path)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT manifestations_destination_owner_claim_fk
        FOREIGN KEY (id, library_id, relocation_destination_path)
        REFERENCES public.library_path_claims(manifestation_id, library_id, path)
        DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE public.library_path_claims ENABLE ROW LEVEL SECURITY;

CREATE POLICY library_path_claims_ingestion ON public.library_path_claims
    TO reverie_ingestion USING (TRUE) WITH CHECK (TRUE);
CREATE POLICY library_path_claims_writeback ON public.library_path_claims
    TO reverie_app
    USING (current_setting('app.system_context', TRUE) = 'writeback') -- noqa: CV11
    WITH CHECK (current_setting('app.system_context', TRUE) = 'writeback'); -- noqa: CV11

GRANT SELECT, INSERT, UPDATE, DELETE ON public.library_path_claims TO reverie_app, reverie_ingestion;
