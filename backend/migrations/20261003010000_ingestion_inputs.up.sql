CREATE TYPE public.ingestion_input_status AS ENUM (
    'pending', 'processing', 'imported', 'duplicate', 'rejected', 'not_accepted', 'operational_failure', 'removed'
);

CREATE TYPE public.ingestion_attempt_outcome AS ENUM (
    'imported', 'duplicate', 'rejected', 'changed', 'shared_dependency',
    'transient_input', 'needs_change', 'interrupted'
);

CREATE TABLE public.ingestion_inputs (
    id uuid PRIMARY KEY DEFAULT uuidv7(), -- noqa: CV11
    source_path bytea NOT NULL CHECK (octet_length(source_path) > 0), -- noqa: CV11
    fingerprint jsonb NOT NULL,
    generation bigint NOT NULL DEFAULT 1 CHECK (generation > 0),
    status public.ingestion_input_status NOT NULL DEFAULT 'pending',
    reason text,
    work_id uuid REFERENCES public.works(id) ON DELETE SET NULL,
    retry_reset_at timestamptz NOT NULL DEFAULT now(), -- noqa: CV11
    observed_at timestamptz NOT NULL DEFAULT now(), -- noqa: CV11
    completed_at timestamptz,
    removed_at timestamptz,
    removal_cause text CHECK (removal_cause IN ('automatic_cleanup', 'admin_deletion', 'external_disappearance')),
    CHECK ((status = 'removed') = (removed_at IS NOT NULL AND removal_cause IS NOT NULL)),
    CHECK (status = 'removed' OR (removed_at IS NULL AND removal_cause IS NULL)),
    CHECK (retry_reset_at >= '0001-01-01' AND retry_reset_at < '10000-01-01'),
    CHECK (observed_at >= '0001-01-01' AND observed_at < '10000-01-01'),
    CHECK (completed_at >= '0001-01-01' AND completed_at < '10000-01-01'),
    CHECK (removed_at >= '0001-01-01' AND removed_at < '10000-01-01')
);

CREATE UNIQUE INDEX idx_ingestion_inputs_present_path ON public.ingestion_inputs (source_path)
WHERE status <> 'removed';
CREATE INDEX idx_ingestion_inputs_current ON public.ingestion_inputs (id) WHERE status <> 'removed';

ALTER TABLE public.ingestion_jobs
    ADD COLUMN input_id uuid REFERENCES public.ingestion_inputs(id),
    ADD COLUMN input_generation bigint,
    ADD COLUMN outcome public.ingestion_attempt_outcome,
    ADD CONSTRAINT ingestion_jobs_input_pair CHECK ((input_id IS NULL) = (input_generation IS NULL)),
    ADD CONSTRAINT ingestion_jobs_generation_positive CHECK (input_generation > 0);

CREATE INDEX idx_ingestion_jobs_input_history ON public.ingestion_jobs (input_id, input_generation, created_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.ingestion_inputs TO reverie_app, reverie_ingestion;
GRANT SELECT ON public.ingestion_inputs TO reverie_readonly;

ALTER TABLE public.settings
    DROP COLUMN format_priority,
    DROP COLUMN cleanup_mode,
    ADD COLUMN accepted_formats text [] NOT NULL DEFAULT '{epub}' CHECK (accepted_formats <@ ARRAY['epub']::text []),
    ADD COLUMN cleanup_imported boolean NOT NULL DEFAULT TRUE,
    ADD COLUMN cleanup_duplicates boolean NOT NULL DEFAULT FALSE;
