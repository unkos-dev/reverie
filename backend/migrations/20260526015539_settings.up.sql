-- Persisted operator-tunable settings (ADR: adr/2026-05-26-persisted-settings.md).
-- Single-row table with typed columns; singleton constraint prevents multi-row.

CREATE TABLE settings (
    id bool PRIMARY KEY DEFAULT true CONSTRAINT singleton CHECK (id = true),

    -- Enrichment subsystem
    enrichment_enabled bool NOT NULL DEFAULT true,
    enrichment_concurrency int NOT NULL DEFAULT 2
        CHECK (enrichment_concurrency BETWEEN 1 AND 10),
    enrichment_poll_idle_secs int NOT NULL DEFAULT 30,
    enrichment_fetch_budget_secs int NOT NULL DEFAULT 15,

    -- Cover acquisition
    cover_max_bytes bigint NOT NULL DEFAULT 10485760,
    cover_download_timeout_secs int NOT NULL DEFAULT 30,
    cover_min_long_edge_px int NOT NULL DEFAULT 1000,
    cover_redirect_limit int NOT NULL DEFAULT 3,

    -- Writeback worker
    writeback_enabled bool NOT NULL DEFAULT true,
    writeback_concurrency int NOT NULL DEFAULT 2
        CHECK (writeback_concurrency BETWEEN 1 AND 10),
    writeback_poll_idle_secs int NOT NULL DEFAULT 5,
    writeback_max_attempts int NOT NULL DEFAULT 3,

    -- OPDS catalogue
    opds_enabled bool NOT NULL DEFAULT true,
    opds_page_size int NOT NULL DEFAULT 50
        CHECK (opds_page_size BETWEEN 1 AND 500),

    -- Ingestion
    format_priority text[] NOT NULL DEFAULT '{epub,pdf,mobi,azw3,cbz,cbr}',
    cleanup_mode text NOT NULL DEFAULT 'all'
        CHECK (cleanup_mode IN ('all', 'ingested', 'none')),

    -- Enrichment source API base URLs
    openlibrary_base_url text NOT NULL DEFAULT 'https://openlibrary.org',
    googlebooks_base_url text NOT NULL DEFAULT 'https://www.googleapis.com/books/v1',
    hardcover_base_url text NOT NULL DEFAULT 'https://api.hardcover.app/v1/graphql',

    updated_at timestamptz NOT NULL DEFAULT now()
);

-- Singleton row — always exactly one row.
INSERT INTO settings DEFAULT VALUES;

-- Notify function: fires on any UPDATE to propagate changes to listeners.
CREATE OR REPLACE FUNCTION notify_settings_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('settings_changed', '');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER settings_changed_trigger
    AFTER UPDATE ON settings
    FOR EACH ROW EXECUTE FUNCTION notify_settings_changed();

-- Role grants
GRANT SELECT, UPDATE ON settings TO reverie_app;
GRANT SELECT ON settings TO reverie_readonly;
