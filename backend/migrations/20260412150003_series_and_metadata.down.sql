REVOKE ALL ON manifestation_tags FROM tome_readonly;
REVOKE ALL ON tags FROM tome_readonly;
REVOKE ALL ON metadata_versions FROM tome_readonly;
REVOKE ALL ON omnibus_contents FROM tome_readonly;
REVOKE ALL ON series_works FROM tome_readonly;
REVOKE ALL ON series FROM tome_readonly;

REVOKE ALL ON manifestation_tags FROM tome_ingestion;
REVOKE ALL ON tags FROM tome_ingestion;
REVOKE ALL ON metadata_versions FROM tome_ingestion;
REVOKE ALL ON omnibus_contents FROM tome_ingestion;
REVOKE ALL ON series_works FROM tome_ingestion;
REVOKE ALL ON series FROM tome_ingestion;

REVOKE ALL ON manifestation_tags FROM tome_app;
REVOKE ALL ON tags FROM tome_app;
REVOKE ALL ON metadata_versions FROM tome_app;
REVOKE ALL ON omnibus_contents FROM tome_app;
REVOKE ALL ON series_works FROM tome_app;
REVOKE ALL ON series FROM tome_app;

DROP TABLE IF EXISTS manifestation_tags;
DROP TABLE IF EXISTS tags;
DROP TABLE IF EXISTS metadata_versions;
DROP TABLE IF EXISTS omnibus_contents;
DROP TABLE IF EXISTS series_works;
DROP TABLE IF EXISTS series;
