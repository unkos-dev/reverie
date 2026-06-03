-- Irreversible data backfill. Normalisation discards the original hyphen,
-- spacing, and prefix formatting of each ISBN, so the prior formatting cannot
-- be reconstructed. Down is a deliberate no-op: rolling back the schema does
-- not restore lost formatting. The backfill only reshapes formatting and does
-- not validate checksums, so it preserves the validity of any row that was
-- already valid and does not make an already-invalid row valid.
SELECT 1;
