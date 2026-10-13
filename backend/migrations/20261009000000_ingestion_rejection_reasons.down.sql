DROP VIEW public.ingestion_input_classes;
ALTER TABLE public.ingestion_inputs DROP COLUMN retries_exhausted_at;
ALTER TABLE public.ingestion_inputs DROP COLUMN rejection_reasons;
