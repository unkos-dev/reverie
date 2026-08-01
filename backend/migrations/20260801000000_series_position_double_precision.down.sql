ALTER TABLE public.series_works
    ALTER COLUMN position TYPE numeric USING position::numeric;
