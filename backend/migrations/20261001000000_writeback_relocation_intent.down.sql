ALTER TABLE public.writeback_jobs
    DROP CONSTRAINT writeback_jobs_reason_chk,
    ADD CONSTRAINT writeback_jobs_reason_chk CHECK (reason IN ('metadata', 'cover'));

DROP INDEX public.idx_manifestations_relocation_intent;

ALTER TABLE public.manifestations
    DROP CONSTRAINT manifestations_relocation_pair_check,
    DROP CONSTRAINT manifestations_relocation_source_check,
    DROP CONSTRAINT manifestations_relocation_destination_check,
    DROP COLUMN relocation_source_path,
    DROP COLUMN relocation_destination_path;
