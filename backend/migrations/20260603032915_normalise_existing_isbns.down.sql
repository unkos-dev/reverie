-- Irreversible data backfill. Normalisation discards the original hyphen and
-- spacing of each ISBN, so the prior formatting cannot be reconstructed.
-- Down is a deliberate no-op: rolling back the schema does not restore lost
-- formatting, and the normalised values remain valid ISBNs.
SELECT 1;
