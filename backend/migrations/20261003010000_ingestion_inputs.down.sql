ALTER TABLE public.settings
    DROP COLUMN accepted_formats,
    DROP COLUMN cleanup_imported,
    DROP COLUMN cleanup_duplicates,
    ADD COLUMN format_priority text [] NOT NULL DEFAULT '{epub,pdf,mobi,azw3,cbz,cbr}',
    ADD COLUMN cleanup_mode text NOT NULL DEFAULT 'all' CHECK (cleanup_mode IN ('all', 'ingested', 'none'));

ALTER TABLE public.ingestion_jobs
    DROP COLUMN input_id,
    DROP COLUMN input_generation,
    DROP COLUMN outcome;

DROP TABLE public.ingestion_inputs;
DROP TYPE public.ingestion_attempt_outcome;
DROP TYPE public.ingestion_input_status;
