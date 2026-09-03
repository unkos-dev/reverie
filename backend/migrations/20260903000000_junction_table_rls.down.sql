DROP POLICY IF EXISTS manifestation_genres_select ON public.manifestation_genres;
DROP POLICY IF EXISTS manifestation_genres_insert ON public.manifestation_genres;
DROP POLICY IF EXISTS manifestation_genres_update ON public.manifestation_genres;
DROP POLICY IF EXISTS manifestation_genres_delete ON public.manifestation_genres;
DROP POLICY IF EXISTS manifestation_genres_ingestion_full_access ON public.manifestation_genres;
ALTER TABLE public.manifestation_genres DISABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS manifestation_moods_select ON public.manifestation_moods;
DROP POLICY IF EXISTS manifestation_moods_insert ON public.manifestation_moods;
DROP POLICY IF EXISTS manifestation_moods_update ON public.manifestation_moods;
DROP POLICY IF EXISTS manifestation_moods_delete ON public.manifestation_moods;
DROP POLICY IF EXISTS manifestation_moods_ingestion_full_access ON public.manifestation_moods;
ALTER TABLE public.manifestation_moods DISABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS manifestation_tags_select ON public.manifestation_tags;
DROP POLICY IF EXISTS manifestation_tags_insert ON public.manifestation_tags;
DROP POLICY IF EXISTS manifestation_tags_update ON public.manifestation_tags;
DROP POLICY IF EXISTS manifestation_tags_delete ON public.manifestation_tags;
DROP POLICY IF EXISTS manifestation_tags_ingestion_full_access ON public.manifestation_tags;
ALTER TABLE public.manifestation_tags DISABLE ROW LEVEL SECURITY;
