ALTER TABLE public.manifestations
    ADD COLUMN relocation_source_path text,
    ADD COLUMN relocation_destination_path text,
    ADD CONSTRAINT manifestations_relocation_pair_check CHECK (
        (relocation_source_path IS NULL) = (relocation_destination_path IS NULL)
    ),
    ADD CONSTRAINT manifestations_relocation_source_check CHECK (
        relocation_source_path <> ''
        AND relocation_source_path !~ '(^/|/$|//|\\|^[A-Za-z]:|(^|/)[.]{1,2}(/|$))'
    ),
    ADD CONSTRAINT manifestations_relocation_destination_check CHECK (
        relocation_destination_path <> ''
        AND relocation_destination_path !~ '(^/|/$|//|\\|^[A-Za-z]:|(^|/)[.]{1,2}(/|$))'
    );

CREATE INDEX idx_manifestations_relocation_intent ON public.manifestations (id)
WHERE relocation_source_path IS NOT NULL;

ALTER TABLE public.writeback_jobs
    DROP CONSTRAINT writeback_jobs_reason_chk,
    ADD CONSTRAINT writeback_jobs_reason_chk CHECK (reason IN ('metadata', 'cover', 'relocation'));
