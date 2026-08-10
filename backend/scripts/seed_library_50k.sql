-- Dev-DB seed: 50,000 synthetic works/manifestations for at-scale UI and
-- query-plan verification of the library table view.
--
-- Usage:
--   reverie-dev psql-rw -f backend/scripts/seed_library_50k.sql
--
-- Teardown:
--   reverie-dev reset-library
--
-- Runs as reverie_migrator, which owns every app table and therefore
-- bypasses RLS on `manifestations` -- no session GUCs are required.
--
-- WARNING: seeded rows are catalog stubs only. `manifestations.file_path`
-- points at a `/seed/` path that does not exist on disk and no cover is
-- generated, so grid-view cover thumbnails 404 on every seeded book. Use
-- table/list view, or the `/design/grid-spike` harness, to inspect this
-- data; do not expect covers to render.
--
-- Vocabulary ("Seeded Tome ...", "Seeded Scrivener ...") is deliberately
-- distinct from every fixture title/author in the test suite so pg_trgm
-- similarity checks in find_or_create paths never cross the 0.6 threshold
-- against this corpus.
--
-- Idempotence guard: aborts only if `works` already holds more than 1,000
-- rows. That stops double-seeding, but up to 1,000 pre-existing works WILL
-- pass the guard, get 50k synthetic rows mixed in beside them, and be
-- destroyed together with the seeds by the `reset-library` teardown. Do
-- not run this against a dev library whose contents you want to keep.

\set ON_ERROR_STOP on

BEGIN;

DO $$
BEGIN
    IF (SELECT count(*) FROM works) > 1000 THEN
        RAISE EXCEPTION
            'seed_library_50k: works already has more than 1000 rows; refusing to seed on top of existing data. Run reverie-dev reset-library first.';
    END IF;
END $$;

-- 500 distinct synthetic authors, spread across the 50k works below so
-- sort=author has realistic cardinality instead of one giant tie group.
INSERT INTO authors (name, sort_name)
SELECT
    'Seeded Scrivener ' || lpad(a.n::text, 3, '0') AS name,
    'Seeded Scrivener ' || lpad(a.n::text, 3, '0') AS sort_name
FROM generate_series(1, 500) AS a (n);

-- 50,000 works. `first_author_sort_name` is app-maintained (no trigger --
-- see backend/src/models/work.rs::refresh_first_author_sort and the
-- backfill in migrations/20260706120000_contributors_subtitle_pages.up.sql),
-- so it is set directly here from the same deterministic author mapping
-- used for the work_authors insert below, rather than refreshed after the
-- fact.
INSERT INTO works (title, sort_title, first_author_sort_name)
SELECT
    'Seeded Tome ' || lpad(g.n::text, 5, '0') AS title,
    'Seeded Tome ' || lpad(g.n::text, 5, '0') AS sort_title,
    'Seeded Scrivener ' || lpad((((g.n - 1) % 500) + 1)::text, 3, '0')
    AS first_author_sort_name
FROM generate_series(1, 50000) AS g (n);

-- One author per work. Joined on `sort_name` (unique among the 500
-- seeded authors) rather than a row_number()/uuidv7 ordering trick --
-- uuidv7 only sorts in insertion order down to clock resolution, so a
-- position-based join could silently mismatch a work's linked author
-- against the `first_author_sort_name` already set above. Matching on
-- the value we deterministically derived keeps both self-consistent.
INSERT INTO work_authors (work_id, author_id, role, position)
SELECT
    w.id AS work_id,
    a.id AS author_id,
    'author'::author_role AS role,
    0 AS position
FROM works w
JOIN authors a
    ON
        a.sort_name = w.first_author_sort_name
        AND a.name LIKE 'Seeded Scrivener %'
WHERE w.title LIKE 'Seeded Tome %';

-- 50,000 manifestations, one per seeded work. `file_path` and both hash
-- columns key off the (already-known) work id -- the manifestation's own
-- id isn't available until after the row exists, and one manifestation
-- per work makes the work id equally unique per row.
INSERT INTO manifestations
(
    work_id, format, file_path, ingestion_file_hash, current_file_hash,
    file_size_bytes, ingestion_status, validation_status
)
SELECT
    w.id AS work_id,
    'epub'::manifestation_format AS format,
    '/seed/seed-' || w.id || '.epub' AS file_path,
    'seed-hash-' || w.id AS ingestion_file_hash,
    'seed-hash-' || w.id AS current_file_hash,
    1000 AS file_size_bytes,
    'complete'::ingestion_status AS ingestion_status,
    'clean'::validation_status AS validation_status
FROM works w
WHERE w.title LIKE 'Seeded Tome %';

ANALYZE works, manifestations, work_authors;

COMMIT;
