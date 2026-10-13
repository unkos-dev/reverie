-- Every restored row sits at position 0; the dropped ordering is not recoverable.
DROP INDEX public.idx_shelf_items_shelf_keyset;

ALTER TABLE public.shelf_items ADD COLUMN "position" integer DEFAULT 0 NOT NULL;

CREATE INDEX idx_shelf_items_shelf_keyset ON public.shelf_items USING btree (
    shelf_id,
    "position",
    added_at,
    manifestation_id
);
