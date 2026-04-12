REVOKE ALL ON manifestations FROM tome_readonly;
REVOKE ALL ON work_authors FROM tome_readonly;
REVOKE ALL ON authors FROM tome_readonly;
REVOKE ALL ON works FROM tome_readonly;
REVOKE ALL ON users FROM tome_readonly;

REVOKE ALL ON manifestations FROM tome_ingestion;
REVOKE ALL ON work_authors FROM tome_ingestion;
REVOKE ALL ON authors FROM tome_ingestion;
REVOKE ALL ON works FROM tome_ingestion;

REVOKE ALL ON manifestations FROM tome_app;
REVOKE ALL ON work_authors FROM tome_app;
REVOKE ALL ON authors FROM tome_app;
REVOKE ALL ON works FROM tome_app;
REVOKE ALL ON users FROM tome_app;

DROP TABLE IF EXISTS manifestations;
DROP TABLE IF EXISTS work_authors;
DROP TABLE IF EXISTS authors;
DROP TABLE IF EXISTS works;
DROP TABLE IF EXISTS users;
