DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM manifestations
        WHERE pub_date IS NOT NULL AND pub_date !~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'
    ) THEN
        RAISE EXCEPTION 'publication dates must be complete dates before rollback';
    END IF;
    IF EXISTS (
        SELECT 1 FROM writeback_jobs
        WHERE field_name = 'pub_date' AND status IN ('pending', 'in_progress')
    ) THEN
        RAISE EXCEPTION 'publication-date jobs must drain before rollback';
    END IF;
END;
$$;

ALTER TABLE manifestations ALTER COLUMN pub_date TYPE date USING pub_date::date;
ALTER TABLE writeback_jobs DROP COLUMN field_name;
