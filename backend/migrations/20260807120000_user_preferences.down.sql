-- Reverse of per-user library display preferences.
--
-- The policy and the grants drop with the table; the enum types are only
-- reachable from its columns, so they drop after it.

DROP TABLE public.user_preferences;

DROP TYPE public.library_view;
DROP TYPE public.library_density;
