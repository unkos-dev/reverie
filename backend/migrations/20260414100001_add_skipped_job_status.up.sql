-- Add 'skipped' to job_status enum for duplicate detection during ingestion.
-- PG 12+ supports ALTER TYPE ADD VALUE inside a transaction.
ALTER TYPE job_status ADD VALUE 'skipped';
