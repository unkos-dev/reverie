-- manifestations.cover_path is the enrichment preview sidecar (Step 7), a
-- distinct artefact from an EPUB's own embedded cover. The dashboard's
-- "Cover" coverage metric counted only cover_path, undercounting books whose
-- embedded cover is fine but whose sidecar was never generated.
--
-- has_embedded_cover is set at ingestion time from the EPUB validator's
-- Layer 5 cover check (declared, present, decodable/parsable). NULL means
-- "unknown" -- pre-existing rows ingested before this column, non-EPUB
-- formats (no structural validator, so no embedded-cover check runs), or a
-- validator crash -- and must not be read as "no cover"; that mirrors the
-- pending/clean distinction validation_status already draws. Rows ingested
-- before this migration stay NULL until re-validated; there is currently no
-- backfill pass that re-parses existing files to populate it retroactively.
ALTER TABLE public.manifestations
    ADD COLUMN has_embedded_cover boolean;
