-- Extensions
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Enum types
CREATE TYPE user_role AS ENUM ('admin', 'adult', 'child');
CREATE TYPE author_role AS ENUM ('author', 'editor', 'translator', 'narrator');
CREATE TYPE manifestation_format AS ENUM ('epub', 'pdf', 'mobi', 'azw3', 'cbz', 'cbr');
CREATE TYPE validation_status AS ENUM ('pending', 'valid', 'invalid', 'repaired');
CREATE TYPE ingestion_status AS ENUM ('pending', 'processing', 'complete', 'failed', 'skipped');
CREATE TYPE metadata_source AS ENUM ('opf', 'openlibrary', 'googlebooks', 'manual', 'ai');
CREATE TYPE metadata_review_status AS ENUM ('draft', 'accepted', 'rejected');
CREATE TYPE tag_type AS ENUM ('genre', 'sub_genre', 'trope', 'theme');

-- Grant schema usage to all application roles.
-- Table-level grants are issued in subsequent migrations after tables exist.
GRANT USAGE ON SCHEMA public TO tome_app;
GRANT USAGE ON SCHEMA public TO tome_ingestion;
GRANT USAGE ON SCHEMA public TO tome_readonly;

-- NOTE on enum design:
-- manifestations.ingestion_status tracks the file's lifecycle within the ingestion
-- pipeline (pending -> processing -> complete/failed/skipped). It lives on the
-- manifestation row itself.
--
-- ingestion_jobs.status (using job_status, defined in the system_tables migration)
-- tracks the batch job orchestration layer — a single job may process many files.
-- These are intentionally separate concerns: a job can fail while individual files
-- within it succeeded, and vice versa.
