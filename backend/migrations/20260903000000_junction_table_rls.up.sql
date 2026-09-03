-- sqruff reports current_setting() as a casting-style violation regardless of cast syntax, so CV11 is
-- suppressed file-wide.
-- noqa: disable=CV11
ALTER TABLE public.manifestation_genres ENABLE ROW LEVEL SECURITY;

CREATE POLICY manifestation_genres_select ON public.manifestation_genres FOR SELECT
TO reverie_app,
reverie_readonly USING (manifestation_id IN (SELECT id FROM public.manifestations));

CREATE POLICY manifestation_genres_insert ON public.manifestation_genres FOR INSERT
TO reverie_app WITH CHECK (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_genres_update ON public.manifestation_genres FOR UPDATE
TO reverie_app USING (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_genres_delete ON public.manifestation_genres FOR DELETE
TO reverie_app USING (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_genres_ingestion_full_access ON public.manifestation_genres
TO reverie_ingestion USING (TRUE) WITH CHECK (TRUE);

ALTER TABLE public.manifestation_moods ENABLE ROW LEVEL SECURITY;

CREATE POLICY manifestation_moods_select ON public.manifestation_moods FOR SELECT
TO reverie_app,
reverie_readonly USING (manifestation_id IN (SELECT id FROM public.manifestations));

CREATE POLICY manifestation_moods_insert ON public.manifestation_moods FOR INSERT
TO reverie_app WITH CHECK (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_moods_update ON public.manifestation_moods FOR UPDATE
TO reverie_app USING (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_moods_delete ON public.manifestation_moods FOR DELETE
TO reverie_app USING (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_moods_ingestion_full_access ON public.manifestation_moods
TO reverie_ingestion USING (TRUE) WITH CHECK (TRUE);

ALTER TABLE public.manifestation_tags ENABLE ROW LEVEL SECURITY;

CREATE POLICY manifestation_tags_select ON public.manifestation_tags FOR SELECT
TO reverie_app,
reverie_readonly USING (manifestation_id IN (SELECT id FROM public.manifestations));

CREATE POLICY manifestation_tags_insert ON public.manifestation_tags FOR INSERT
TO reverie_app WITH CHECK (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_tags_update ON public.manifestation_tags FOR UPDATE
TO reverie_app USING (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_tags_delete ON public.manifestation_tags FOR DELETE
TO reverie_app USING (
    manifestation_id IN (SELECT id FROM public.manifestations)
    AND EXISTS (
        SELECT 1
        FROM public.users
        WHERE id = (SELECT current_setting('app.current_user_id', TRUE)::uuid) AND role IN ('admin', 'adult')
    )
);

CREATE POLICY manifestation_tags_ingestion_full_access ON public.manifestation_tags
TO reverie_ingestion USING (TRUE) WITH CHECK (TRUE);
