-- Revoke the grant added in .up.sql.
REVOKE SELECT ON field_locks FROM tome_ingestion;
