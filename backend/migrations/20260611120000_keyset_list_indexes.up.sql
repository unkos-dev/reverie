-- UNK-374: btree indexes backing the keyset-paginated list scans added in
-- the same change (ADR-7 / CLAUDE.md indexing discipline: index keyset sort
-- keys). Each index matches its endpoint's ORDER BY exactly, including the
-- unique final tiebreaker, so the keyset WHERE + ORDER BY + LIMIT resolves
-- as an index range scan instead of a sort.

-- GET /api/v1/shelves: ORDER BY is_system DESC, name ASC, id ASC per user.
CREATE INDEX idx_shelves_user_keyset
    ON public.shelves USING btree (user_id, is_system DESC, name ASC, id ASC);

-- GET /api/v1/shelves/{id} items: ORDER BY position ASC, added_at ASC,
-- manifestation_id ASC per shelf.
CREATE INDEX idx_shelf_items_shelf_keyset
    ON public.shelf_items USING btree (shelf_id, position ASC, added_at ASC, manifestation_id ASC);

-- OPDS authors navigation feed: ORDER BY sort_name ASC, id ASC.
CREATE INDEX idx_authors_sort_name_id
    ON public.authors USING btree (sort_name ASC, id ASC);

-- OPDS series navigation feed: ORDER BY sort_name ASC, id ASC.
CREATE INDEX idx_series_sort_name_id
    ON public.series USING btree (sort_name ASC, id ASC);
