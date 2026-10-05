DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM public.ingestion_jobs WHERE publication_library_id IS NOT NULL) THEN
        RAISE EXCEPTION 'Unresolved ingestion publications must be reconciled before downgrade';
    END IF;
END
$$;

DROP INDEX public.idx_ingestion_jobs_unresolved_publication;
ALTER TABLE public.ingestion_jobs
    DROP CONSTRAINT ingestion_publication_failure_pair,
    DROP CONSTRAINT ingestion_publication_linked,
    DROP CONSTRAINT ingestion_publication_evidence_pair,
    DROP COLUMN publication_failure_reason,
    DROP COLUMN publication_failure_class,
    DROP COLUMN publication_size,
    DROP COLUMN publication_hash,
    DROP COLUMN publication_identity,
    DROP COLUMN publication_path,
    DROP COLUMN publication_library_id;
