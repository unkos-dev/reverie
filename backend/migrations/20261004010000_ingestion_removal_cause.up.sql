ALTER TABLE public.ingestion_inputs
    DROP CONSTRAINT ingestion_inputs_removal_cause_check,
    ADD CONSTRAINT ingestion_inputs_removal_cause_check CHECK (
        removal_cause IN ('automatic_cleanup', 'admin_deletion', 'external_disappearance', 'unattributed_disappearance')
    );
