REVOKE USAGE ON SCHEMA public FROM tome_readonly;
REVOKE USAGE ON SCHEMA public FROM tome_ingestion;
REVOKE USAGE ON SCHEMA public FROM tome_app;

DROP TYPE IF EXISTS tag_type;
DROP TYPE IF EXISTS metadata_review_status;
DROP TYPE IF EXISTS metadata_source;
DROP TYPE IF EXISTS ingestion_status;
DROP TYPE IF EXISTS validation_status;
DROP TYPE IF EXISTS manifestation_format;
DROP TYPE IF EXISTS author_role;
DROP TYPE IF EXISTS user_role;

DROP EXTENSION IF EXISTS "pg_trgm";
