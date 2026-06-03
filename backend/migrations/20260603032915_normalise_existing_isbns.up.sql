-- Backfill existing ISBN values to the digits-only shape the ingestion
-- pipeline and the metadata PATCH handler now produce (hyphens/spaces
-- stripped, ISBN-10 'X' check digit uppercased).
--
-- Before ISBN normalisation on the manual-edit PATCH path, operator edits
-- persisted raw strings (e.g. "978-0-306-40615-7") while ingestion stored
-- "9780306406157". Rematch uses an exact-equality join, so divergent
-- formatting silently hid duplicate manifestations. This one-shot backfill
-- aligns historical rows; the WHERE guards touch only rows that actually
-- change. There is no UNIQUE constraint on isbn_10/isbn_13 (manifestations
-- may legitimately share an ISBN), so collapsing formats cannot violate a
-- constraint.

UPDATE manifestations
SET isbn_10 = upper(regexp_replace(isbn_10, '[- ]', '', 'g'))
WHERE isbn_10 IS NOT NULL
  AND isbn_10 <> upper(regexp_replace(isbn_10, '[- ]', '', 'g'));

UPDATE manifestations
SET isbn_13 = regexp_replace(isbn_13, '[- ]', '', 'g')
WHERE isbn_13 IS NOT NULL
  AND isbn_13 <> regexp_replace(isbn_13, '[- ]', '', 'g');
