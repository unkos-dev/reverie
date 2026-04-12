-- Job status enum (separate from ingestion_status — see note in migration 1).
-- ingestion_status tracks per-file lifecycle on manifestations.
-- job_status tracks batch orchestration on ingestion_jobs.
-- A job can fail while individual files within it succeeded, and vice versa.
CREATE TYPE job_status AS ENUM ('queued', 'running', 'complete', 'failed');

-- API response cache
CREATE TABLE api_cache (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source TEXT NOT NULL,
    lookup_key TEXT NOT NULL,
    response JSONB NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    UNIQUE (source, lookup_key)
);

-- Ingestion job tracking
CREATE TABLE ingestion_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id UUID NOT NULL,
    source_path TEXT NOT NULL,
    status job_status NOT NULL DEFAULT 'queued',
    error_message TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Webhooks
CREATE TABLE webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    events JSONB NOT NULL DEFAULT '[]',
    payload_template TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Webhook delivery log
CREATE TABLE webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    response_status INTEGER,
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Grants: tome_app (full DML on all system tables)
GRANT SELECT, INSERT, UPDATE, DELETE ON api_cache TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ingestion_jobs TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON webhooks TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON webhook_deliveries TO tome_app;

-- Grants: tome_ingestion (pipeline tables only — no webhook access)
GRANT SELECT, INSERT, UPDATE, DELETE ON api_cache TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON ingestion_jobs TO tome_ingestion;

-- Grants: tome_readonly (SELECT on all)
GRANT SELECT ON api_cache TO tome_readonly;
GRANT SELECT ON ingestion_jobs TO tome_readonly;
GRANT SELECT ON webhooks TO tome_readonly;
GRANT SELECT ON webhook_deliveries TO tome_readonly;
