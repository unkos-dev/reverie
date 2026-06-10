-- Best-effort reverse: pre-UNK-313 every non-EPUB row was written as 'clean'
-- at ingest time, so restoring 'clean' for all non-EPUB 'pending' rows
-- reproduces the old behaviour exactly.
UPDATE manifestations
SET validation_status = 'clean'
WHERE validation_status = 'pending'
  AND format <> 'epub';
