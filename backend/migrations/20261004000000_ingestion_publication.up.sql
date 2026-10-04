ALTER TABLE public.ingestion_jobs
    ADD COLUMN publication_library_id uuid REFERENCES public.libraries(id),
    ADD COLUMN publication_path text CHECK (
        publication_path <> ''
        AND publication_path !~ '(^/|/$|//|\\|^[A-Za-z]:|(^|/)[.]{1,2}(/|$))'
    ),
    ADD COLUMN publication_identity jsonb,
    ADD COLUMN publication_hash text CHECK (publication_hash ~ '^[0-9a-f]{64}$'),
    ADD COLUMN publication_size bigint CHECK (publication_size >= 0),
    ADD COLUMN publication_failure_class public.ingestion_attempt_outcome CHECK (
        publication_failure_class IN ('shared_dependency', 'transient_input', 'needs_change')
    ),
    ADD COLUMN publication_failure_reason text,
    ADD CONSTRAINT ingestion_publication_evidence_pair CHECK (
        (publication_library_id IS NULL) = (publication_path IS NULL)
        AND (publication_library_id IS NULL) = (publication_identity IS NULL)
        AND (publication_library_id IS NULL) = (publication_hash IS NULL)
        AND (publication_library_id IS NULL) = (publication_size IS NULL)
    ),
    ADD CONSTRAINT ingestion_publication_linked CHECK (publication_library_id IS NULL OR input_id IS NOT NULL),
    ADD CONSTRAINT ingestion_publication_failure_pair CHECK (
        (publication_failure_class IS NULL) = (publication_failure_reason IS NULL)
        AND (publication_failure_class IS NULL OR publication_library_id IS NOT NULL)
    );

CREATE INDEX idx_ingestion_jobs_unresolved_publication ON public.ingestion_jobs (id)
WHERE publication_library_id IS NOT NULL;
