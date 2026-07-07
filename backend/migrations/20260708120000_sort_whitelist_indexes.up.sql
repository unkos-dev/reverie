-- Sort-whitelist indexes for the /api/v1/books multi-level sort stack
-- (indexing discipline: a column enters the sort whitelist only with its
-- ordering indexes in the same migration; index keyset sort keys exactly,
-- including the unique final tiebreaker, so keyset WHERE + ORDER BY + LIMIT
-- resolves as an index range scan instead of a sort).
--
-- title and created_at are NOT NULL and already covered: forward/backward
-- scans of idx_works_sort_title_id and idx_manifestations_recent_keyset
-- serve both directions. The two nullable whitelisted columns (pages,
-- first_author_sort_name) sort NULLS LAST in both directions, and
-- Postgres's DESC default is NULLS FIRST, so DESC NULLS LAST cannot ride
-- a backward scan of an ascending index - each needs an explicit
-- DESC NULLS LAST composite.

-- pages ASC: btree ASC default is NULLS LAST, so the plain composite
-- serves ORDER BY m.pages ASC NULLS LAST, m.id ASC directly.
CREATE INDEX idx_manifestations_pages_keyset
    ON public.manifestations USING btree (pages, id);

-- pages DESC: ORDER BY m.pages DESC NULLS LAST, m.id DESC.
CREATE INDEX idx_manifestations_pages_keyset_desc
    ON public.manifestations USING btree (pages DESC NULLS LAST, id DESC);

-- author DESC: ORDER BY w.first_author_sort_name DESC NULLS LAST, id DESC.
-- The ascending direction is served by the existing
-- idx_works_first_author_sort_id (first_author_sort_name, id).
CREATE INDEX idx_works_first_author_sort_desc
    ON public.works USING btree (first_author_sort_name DESC NULLS LAST, id DESC);
