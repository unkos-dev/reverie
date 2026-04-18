-- Reverse of 20260417000001_add_enrichment_pipeline.up.sql.
--
-- Ordering: newest additions first, enum-dependent columns before enum drops,
-- schema-changing ALTERs before enum rebuilds.

---------------------------------------------------------------------------
-- 9. api_cache columns + kind enum
---------------------------------------------------------------------------

ALTER TABLE api_cache
    DROP COLUMN IF EXISTS http_status,
    DROP COLUMN IF EXISTS response_kind;

DROP TYPE IF EXISTS api_cache_kind;

---------------------------------------------------------------------------
-- 8. Enrichment queue columns + enum
---------------------------------------------------------------------------

DROP INDEX IF EXISTS idx_manifestations_enrichment_queue;

ALTER TABLE manifestations
    DROP COLUMN IF EXISTS enrichment_error,
    DROP COLUMN IF EXISTS enrichment_attempt_count,
    DROP COLUMN IF EXISTS enrichment_attempted_at,
    DROP COLUMN IF EXISTS enrichment_status;

DROP TYPE IF EXISTS enrichment_status;

---------------------------------------------------------------------------
-- 7. Field locks
---------------------------------------------------------------------------

DROP TABLE IF EXISTS field_locks;

---------------------------------------------------------------------------
-- 6. Rematch pointer
---------------------------------------------------------------------------

ALTER TABLE manifestations
    DROP COLUMN IF EXISTS suspected_duplicate_work_id;

---------------------------------------------------------------------------
-- 5. Canonical pointer columns
---------------------------------------------------------------------------

ALTER TABLE manifestation_tags
    DROP COLUMN IF EXISTS source_version_id;

ALTER TABLE work_authors
    DROP COLUMN IF EXISTS source_version_id;

ALTER TABLE manifestations
    DROP COLUMN IF EXISTS cover_version_id,
    DROP COLUMN IF EXISTS cover_source,
    DROP COLUMN IF EXISTS cover_size_bytes,
    DROP COLUMN IF EXISTS cover_sha256,
    DROP COLUMN IF EXISTS cover_path,
    DROP COLUMN IF EXISTS isbn_13_version_id,
    DROP COLUMN IF EXISTS isbn_10_version_id,
    DROP COLUMN IF EXISTS pub_date_version_id,
    DROP COLUMN IF EXISTS publisher_version_id;

ALTER TABLE works
    DROP COLUMN IF EXISTS language_version_id,
    DROP COLUMN IF EXISTS description_version_id,
    DROP COLUMN IF EXISTS title_version_id;

---------------------------------------------------------------------------
-- 4. Revert metadata_versions journal changes.
---------------------------------------------------------------------------

DROP INDEX IF EXISTS idx_mv_last_seen;
DROP INDEX IF EXISTS idx_mv_manifestation_field;

ALTER TABLE metadata_versions
    DROP CONSTRAINT IF EXISTS metadata_versions_mfs_hash_unique;

-- Recreate metadata_source enum (must exist before converting source column back).
CREATE TYPE metadata_source AS ENUM ('opf', 'openlibrary', 'googlebooks', 'manual', 'ai');

-- Drop any rows whose source can't map back to the original enum. 'hardcover'
-- was added in this migration's registry; those rows can't survive revert.
DELETE FROM metadata_versions
 WHERE source NOT IN ('opf', 'openlibrary', 'googlebooks', 'manual', 'ai');

ALTER TABLE metadata_versions
    DROP CONSTRAINT IF EXISTS metadata_versions_source_fk;

ALTER TABLE metadata_versions
    ALTER COLUMN source TYPE metadata_source USING source::metadata_source;

ALTER TABLE metadata_versions
    DROP COLUMN IF EXISTS observation_count,
    DROP COLUMN IF EXISTS last_seen_at,
    DROP COLUMN IF EXISTS first_seen_at,
    DROP COLUMN IF EXISTS match_type,
    DROP COLUMN IF EXISTS value_hash;

-- Re-add the pre-migration unique constraint. If the journal has accumulated
-- multiple rows per (manifestation, source, field) (expected under the new
-- model), this will fail — which is the correct signal that data can't be
-- losslessly reverted.
ALTER TABLE metadata_versions
    ADD CONSTRAINT metadata_versions_manifestation_source_field_unique
    UNIQUE (manifestation_id, source, field_name);

---------------------------------------------------------------------------
-- 3. Restore original metadata_review_status enum.
---------------------------------------------------------------------------

ALTER TYPE metadata_review_status RENAME TO metadata_review_status_old;

CREATE TYPE metadata_review_status AS ENUM ('draft', 'accepted', 'rejected');

ALTER TABLE metadata_versions ALTER COLUMN status DROP DEFAULT;

ALTER TABLE metadata_versions
    ALTER COLUMN status TYPE metadata_review_status
    USING CASE status::text
        WHEN 'pending'  THEN 'draft'::metadata_review_status
        WHEN 'rejected' THEN 'rejected'::metadata_review_status
    END;

ALTER TABLE metadata_versions ALTER COLUMN status SET DEFAULT 'draft';

DROP TYPE metadata_review_status_old;

---------------------------------------------------------------------------
-- 2. Queue / cache enums already dropped above.
---------------------------------------------------------------------------

---------------------------------------------------------------------------
-- 1. Source registry.  (DROP TABLE cascades its grants — no REVOKE needed.)
---------------------------------------------------------------------------

DROP TABLE IF EXISTS metadata_sources;

-- pgcrypto is left enabled; dropping it could break unrelated future consumers.
