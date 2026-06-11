-- Backfill for the UNK-313 behaviour change: non-EPUB rows ingested before
-- this release were stored as validation_status='clean' despite never being
-- validated (structural validation only exists for EPUB). Reset them to the
-- 'pending' default so 'clean' once again means "validator ran, found no
-- issues" across the whole table, not just rows ingested after the change.
UPDATE manifestations
SET validation_status = 'pending'
WHERE validation_status = 'clean'
  AND format <> 'epub';
