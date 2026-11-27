CREATE UNIQUE INDEX idx_shelves_user_name_folded
ON public.shelves (user_id, lower(public.immutable_unaccent(name)));
