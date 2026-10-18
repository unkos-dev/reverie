CREATE INDEX idx_ingestion_inputs_matched_manifestation
ON public.ingestion_inputs (matched_manifestation_id)
WHERE matched_manifestation_id IS NOT NULL;

CREATE INDEX idx_ingestion_inputs_work
ON public.ingestion_inputs (work_id)
WHERE work_id IS NOT NULL;
