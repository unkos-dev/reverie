REVOKE ALL ON webhook_deliveries FROM tome_readonly;
REVOKE ALL ON webhooks FROM tome_readonly;
REVOKE ALL ON ingestion_jobs FROM tome_readonly;
REVOKE ALL ON api_cache FROM tome_readonly;

REVOKE ALL ON ingestion_jobs FROM tome_ingestion;
REVOKE ALL ON api_cache FROM tome_ingestion;

REVOKE ALL ON webhook_deliveries FROM tome_app;
REVOKE ALL ON webhooks FROM tome_app;
REVOKE ALL ON ingestion_jobs FROM tome_app;
REVOKE ALL ON api_cache FROM tome_app;

DROP TABLE IF EXISTS webhook_deliveries;
DROP TABLE IF EXISTS webhooks;
DROP TABLE IF EXISTS ingestion_jobs;
DROP TABLE IF EXISTS api_cache;

DROP TYPE IF EXISTS job_status;
