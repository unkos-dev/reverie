-- sqlx:disable-transaction
-- ALTER TYPE ... ADD VALUE cannot run inside a PostgreSQL transaction.
-- sqlx wraps every migration in a transaction by default — this pragma disables that.

-- Add 'degraded' to validation_status enum.
-- NOTE: PostgreSQL cannot remove enum values once added if any rows use them.
-- Roll back before any EPUBs are ingested to keep the rollback clean.
ALTER TYPE validation_status ADD VALUE IF NOT EXISTS 'degraded';

-- Add accessibility metadata JSONB column (read-only, sourced from OPF)
ALTER TABLE manifestations ADD COLUMN accessibility_metadata JSONB;
