DROP INDEX public.idx_shelf_items_shelf_keyset;

ALTER TABLE public.shelf_items DROP COLUMN "position";

CREATE INDEX idx_shelf_items_shelf_keyset ON public.shelf_items USING btree (
    shelf_id,
    added_at,
    manifestation_id
);
