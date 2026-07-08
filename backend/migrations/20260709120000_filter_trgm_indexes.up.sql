-- Trigram index backing the subtitle contains-filter.
--
-- A free-text column gains a case-insensitive `_contains` filter only together
-- with its trigram index, the same "filterable iff index-backed" discipline the
-- sort whitelist applies to orderable columns. `title` contains already rides
-- the GiST trigram index from the initial schema; this GIN index covers the
-- subtitle column.
--
-- `isbn_13` deliberately gets no trigram index: `isbn_13_eq` (wildcard-free
-- ILIKE) rides the existing plain b-tree index, and `isbn_13_contains`
-- (substring on a structured 13-digit code) is a rare pattern a bounded
-- `LIMIT`-capped filter scan handles without paying the write/storage cost of a
-- GIN index on a high-churn column.
--
-- GIN (not GiST) matches the vocabulary-index convention established for the
-- genre/mood/tag/publisher trigram indexes.

CREATE INDEX idx_works_subtitle_trgm
    ON public.works USING gin (subtitle public.gin_trgm_ops);
