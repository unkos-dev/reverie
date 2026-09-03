-- These policy blocks mirror the shipped manifestation_external_identifiers policies byte for
-- byte; the correlated references in a policy predicate are what RF01/RF03 flag.
-- noqa: disable=RF01,RF03,CV11,CP03,CP04
ALTER TABLE public.manifestation_genres ENABLE ROW LEVEL SECURITY;

CREATE POLICY manifestation_genres_select ON public.manifestation_genres FOR SELECT
TO reverie_app,
reverie_readonly USING ((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_genres.manifestation_id)
)));

CREATE POLICY manifestation_genres_insert ON public.manifestation_genres FOR INSERT
TO reverie_app WITH CHECK (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_genres.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_genres_update ON public.manifestation_genres FOR UPDATE
TO reverie_app USING (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_genres.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
)))) WITH CHECK (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_genres.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_genres_delete ON public.manifestation_genres FOR DELETE
TO reverie_app USING (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_genres.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_genres_ingestion_full_access ON public.manifestation_genres
TO reverie_ingestion USING (true) WITH CHECK (true);

ALTER TABLE public.manifestation_moods ENABLE ROW LEVEL SECURITY;

CREATE POLICY manifestation_moods_select ON public.manifestation_moods FOR SELECT
TO reverie_app,
reverie_readonly USING ((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_moods.manifestation_id)
)));

CREATE POLICY manifestation_moods_insert ON public.manifestation_moods FOR INSERT
TO reverie_app WITH CHECK (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_moods.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_moods_update ON public.manifestation_moods FOR UPDATE
TO reverie_app USING (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_moods.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
)))) WITH CHECK (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_moods.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_moods_delete ON public.manifestation_moods FOR DELETE
TO reverie_app USING (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_moods.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_moods_ingestion_full_access ON public.manifestation_moods
TO reverie_ingestion USING (true) WITH CHECK (true);

ALTER TABLE public.manifestation_tags ENABLE ROW LEVEL SECURITY;

CREATE POLICY manifestation_tags_select ON public.manifestation_tags FOR SELECT
TO reverie_app,
reverie_readonly USING ((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_tags.manifestation_id)
)));

CREATE POLICY manifestation_tags_insert ON public.manifestation_tags FOR INSERT
TO reverie_app WITH CHECK (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_tags.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_tags_update ON public.manifestation_tags FOR UPDATE
TO reverie_app USING (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_tags.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
)))) WITH CHECK (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_tags.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_tags_delete ON public.manifestation_tags FOR DELETE
TO reverie_app USING (((EXISTS (
    SELECT 1
    FROM public.manifestations AS m
    WHERE (m.id = manifestation_tags.manifestation_id))) AND (EXISTS (
    SELECT 1
    FROM public.users
    WHERE ((users.id = ((
        SELECT current_setting(
            'app.current_user_id'::text,
            true
        ) AS current_setting
    ))::uuid) AND (users.role = ANY(ARRAY[
        'admin'::public.user_role,
        'adult'::public.user_role
    ])))
))));

CREATE POLICY manifestation_tags_ingestion_full_access ON public.manifestation_tags
TO reverie_ingestion USING (true) WITH CHECK (true);
