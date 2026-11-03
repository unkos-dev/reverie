CREATE INDEX idx_manifestations_isbn_10
ON public.manifestations (isbn_10)
WHERE isbn_10 IS NOT NULL;
