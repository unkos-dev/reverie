ALTER TABLE manifestations
ALTER COLUMN pub_date TYPE text USING to_char(pub_date, 'YYYY-MM-DD');

ALTER TABLE writeback_jobs ADD COLUMN field_name text;
