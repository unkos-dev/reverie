DROP INDEX public.idx_manifestations_isbn_13_keyset_desc;
DROP INDEX public.idx_manifestations_isbn_13_keyset;
DROP INDEX public.idx_works_subtitle_keyset_desc;
DROP INDEX public.idx_works_subtitle_keyset;

ALTER TABLE public.user_preferences
DROP CONSTRAINT user_preferences_sort_stack_shape;

ALTER TABLE public.user_preferences
ADD CONSTRAINT user_preferences_sort_stack_shape
CHECK ((sort_stack ~ '^-?[a-z_]+(,-?[a-z_]+){0,2}$'::text));
