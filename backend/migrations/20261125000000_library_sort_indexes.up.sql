-- Allow digits in sort_stack wire names (e.g. isbn_13) and add composite
-- ordering indexes for the newly sortable single-table Library columns.

ALTER TABLE public.user_preferences
DROP CONSTRAINT user_preferences_sort_stack_shape;

ALTER TABLE public.user_preferences
ADD CONSTRAINT user_preferences_sort_stack_shape
CHECK ((sort_stack ~ '^-?[a-z0-9_]+(,-?[a-z0-9_]+){0,2}$'::text));

CREATE INDEX idx_works_subtitle_keyset
ON public.works USING btree (subtitle, id);

CREATE INDEX idx_works_subtitle_keyset_desc
ON public.works USING btree (subtitle DESC NULLS LAST, id DESC);

CREATE INDEX idx_manifestations_isbn_13_keyset
ON public.manifestations USING btree (isbn_13, id);

CREATE INDEX idx_manifestations_isbn_13_keyset_desc
ON public.manifestations USING btree (isbn_13 DESC NULLS LAST, id DESC);
