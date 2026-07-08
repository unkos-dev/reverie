-- Trigram indexes backing the typed contains-filters.
--
-- A column gains a case-insensitive `_contains` filter only together with its
-- trigram index, the same "filterable iff index-backed" discipline the sort
-- whitelist applies to orderable columns. `title` contains already rides the
-- GiST trigram index from the initial schema; these two GIN indexes cover the
-- remaining contains-filterable text columns. `isbn_13_eq` (wildcard-free
-- ILIKE) rides the same GIN index as `isbn_13_contains`.
--
-- GIN (not GiST) matches the vocabulary-index convention established for the
-- genre/mood/tag/publisher trigram indexes.

CREATE INDEX idx_works_subtitle_trgm
    ON public.works USING gin (subtitle public.gin_trgm_ops);

CREATE INDEX idx_manifestations_isbn13_trgm
    ON public.manifestations USING gin (isbn_13 public.gin_trgm_ops);
