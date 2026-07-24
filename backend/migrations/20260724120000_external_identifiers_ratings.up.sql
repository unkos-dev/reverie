-- External-source identifiers registry + per-source ratings cache + provider
-- visibility. Two deliberately different domains:
--
--  * Identifiers are stable, hand-editable, provenance-tracked facts. They flow
--    through the existing metadata journal (source_version_id pointer) and mirror
--    the genres/moods junction template, but are hardened with their own RLS
--    because a direct read must not bypass row authorization (CodeGuard IDOR).
--    One value per (entity, scheme): PK is (entity_id, scheme), external_id is
--    replaced in place on set (single-slot AutoFill policy).
--  * Ratings are a refreshable multi-source cache, one row per (manifestation,
--    source). Each provider is independently authoritative for its own scale, so
--    there is no single canonical value to reconcile; ratings live OUTSIDE the
--    journal/policy/lock engine and are never written back. RLS-protected,
--    read-only to users, written only by enrichment.
--
-- Two distinct vocabularies, not one: identifier `scheme` FKs a new
-- `identifier_schemes` namespace table; rating `source` FKs a constrained
-- `rating_sources` table (itself FK'd to the `metadata_sources` provenance
-- vocabulary). These are different axes (a manually entered googlebooks id has
-- observer `manual` but scheme `googlebooks`).

-- ---- 1) identifier scheme vocabulary ----

-- Closed namespace of external-identifier schemes. ISBN is intentionally absent:
-- it stays in the manifestations.isbn_10/13 columns. Excludes the
-- provenance-only sources (manual/opf/ai) which are not identifier namespaces.
CREATE TABLE public.identifier_schemes (
    id text NOT NULL,
    display_name text NOT NULL,
    CONSTRAINT identifier_schemes_pkey PRIMARY KEY (id)
);

INSERT INTO public.identifier_schemes (id, display_name)
VALUES
    ('asin',         'Amazon ASIN'),
    ('oclc',         'OCLC / WorldCat'),
    ('lccn',         'Library of Congress'),
    ('googlebooks',  'Google Books'),
    ('openlibrary',  'Open Library'),
    ('hardcover',    'Hardcover'),
    ('goodreads',    'Goodreads'),
    ('librarything', 'LibraryThing'),
    ('wikidata',     'Wikidata'),
    ('calibre',      'Calibre');

-- ---- 2) metadata_sources extension (manual-only observers) ----

-- Manual-only providers seeded so a manual observation can record `source` here
-- where applicable, and so rating_sources' FK to metadata_sources is satisfiable
-- (amazon in particular: rating_sources.amazon FKs this table). These are not
-- auto-fetch adapters, so they take a low base_priority and never enter
-- build_sources. kind 'external' distinguishes them from the fetching adapters.
--
-- ON CONFLICT (id) DO NOTHING is load-bearing, not decorative: the down step
-- deliberately leaves these rows in place (they are harmless additive vocabulary
-- and a DELETE would fail once a real observation references one), so a
-- revert-then-reapply would otherwise re-INSERT existing PKs and abort with
-- 23505. Together with the leave-on-down choice this keeps up/down idempotent.
INSERT INTO public.metadata_sources (id, display_name, kind, enabled, base_priority, config)
VALUES
    ('goodreads',    'Goodreads',    'external', true, 10, '{}'),
    ('librarything', 'LibraryThing', 'external', true, 10, '{}'),
    ('asin',         'Amazon ASIN',  'external', true, 10, '{}'),
    ('wikidata',     'Wikidata',     'external', true, 10, '{}'),
    ('calibre',      'Calibre',      'external', true, 10, '{}'),
    ('amazon',       'Amazon',       'external', true, 10, '{}')
ON CONFLICT (id) DO NOTHING;

-- ---- 3) rating source vocabulary (rating-capable subset) ----

-- A rating source must be rating-capable, not merely any observer. This closed
-- set is what provider_visibility validation and the ratings FK both assume.
-- FK to metadata_sources DB-enforces that every rating source is also a known
-- provenance source. manual/opf/ai/identifier-only providers cannot carry a
-- rating.
CREATE TABLE public.rating_sources (
    id text NOT NULL,
    display_name text NOT NULL,
    CONSTRAINT rating_sources_pkey PRIMARY KEY (id),
    CONSTRAINT rating_sources_id_fkey
        FOREIGN KEY (id) REFERENCES public.metadata_sources(id)
);

INSERT INTO public.rating_sources (id, display_name)
VALUES
    ('googlebooks', 'Google Books'),
    ('openlibrary', 'Open Library'),
    ('hardcover',   'Hardcover'),
    ('goodreads',   'Goodreads'),
    ('amazon',      'Amazon');

-- ---- 4) identifier registry tables ----

-- external_id CHECK is a structural backstop (the typed per-(scheme, level)
-- parser at the PATCH boundary is the precise gate): safe charset only, 1..255.
-- The anchored class rejects path/query separators, whitespace, control chars,
-- and non-ASCII, so a value can never smuggle a path/query segment into an
-- outbound REST call.

CREATE TABLE public.work_external_identifiers (
    work_id uuid NOT NULL,
    scheme text NOT NULL,
    external_id text NOT NULL,
    source_version_id uuid,
    CONSTRAINT work_external_identifiers_pkey PRIMARY KEY (work_id, scheme),
    CONSTRAINT work_external_identifiers_work_id_fkey
        FOREIGN KEY (work_id) REFERENCES public.works(id) ON DELETE CASCADE,
    CONSTRAINT work_external_identifiers_scheme_fkey
        FOREIGN KEY (scheme) REFERENCES public.identifier_schemes(id),
    CONSTRAINT work_external_identifiers_source_version_id_fkey
        FOREIGN KEY (source_version_id)
        REFERENCES public.metadata_versions(id) ON DELETE SET NULL,
    CONSTRAINT work_external_identifiers_external_id_check
        CHECK (external_id ~ '^[A-Za-z0-9._-]{1,255}$')
);

CREATE TABLE public.manifestation_external_identifiers (
    manifestation_id uuid NOT NULL,
    scheme text NOT NULL,
    external_id text NOT NULL,
    source_version_id uuid,
    CONSTRAINT manifestation_external_identifiers_pkey PRIMARY KEY (manifestation_id, scheme),
    CONSTRAINT manifestation_external_identifiers_manifestation_id_fkey
        FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE,
    CONSTRAINT manifestation_external_identifiers_scheme_fkey
        FOREIGN KEY (scheme) REFERENCES public.identifier_schemes(id),
    CONSTRAINT manifestation_external_identifiers_source_version_id_fkey
        FOREIGN KEY (source_version_id)
        REFERENCES public.metadata_versions(id) ON DELETE SET NULL,
    CONSTRAINT manifestation_external_identifiers_external_id_check
        CHECK (external_id ~ '^[A-Za-z0-9._-]{1,255}$')
);

-- Partial pointer indexes mirror idx_manifestation_genres_source_version_id.
CREATE INDEX idx_work_external_identifiers_source_version_id
    ON public.work_external_identifiers USING btree (source_version_id)
    WHERE (source_version_id IS NOT NULL);
CREATE INDEX idx_manifestation_external_identifiers_source_version_id
    ON public.manifestation_external_identifiers USING btree (source_version_id)
    WHERE (source_version_id IS NOT NULL);

-- ---- 5) ratings cache ----

CREATE TABLE public.manifestation_external_ratings (
    manifestation_id uuid NOT NULL,
    source text NOT NULL,
    rating real NOT NULL,
    rating_scale real NOT NULL,
    review_count integer DEFAULT 0 NOT NULL,
    fetched_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT manifestation_external_ratings_pkey PRIMARY KEY (manifestation_id, source),
    CONSTRAINT manifestation_external_ratings_manifestation_id_fkey
        FOREIGN KEY (manifestation_id) REFERENCES public.manifestations(id) ON DELETE CASCADE,
    CONSTRAINT manifestation_external_ratings_source_fkey
        FOREIGN KEY (source) REFERENCES public.rating_sources(id),
    CONSTRAINT manifestation_external_ratings_rating_scale_positive
        CHECK (rating_scale > 0),
    CONSTRAINT manifestation_external_ratings_rating_range
        CHECK (rating >= 0 AND rating <= rating_scale),
    CONSTRAINT manifestation_external_ratings_review_count_nonneg
        CHECK (review_count >= 0)
);

-- ---- 6) row-level security ----

-- CREATE POLICY is inert without ENABLE (proven against the initial schema): a
-- table with policies but no ENABLE stays fully readable/writable through its
-- grants. Match the schema's ENABLE-only convention (never FORCE): the app
-- connects as non-owner roles for which ENABLE already applies.
--
-- Predicates are lifted verbatim from the proven manifestations policies. Two
-- details are load-bearing and must not be paraphrased away:
--  1. current_setting('app.current_user_id', true) passes missing_ok=true, so an
--     unset GUC resolves to NULL and the predicate fails CLOSED (deny), rather
--     than raising a hard query error as the naive cast would.
--  2. The outer column in every correlated EXISTS is qualified with the policy's
--     own table. manifestations has a work_id column, so an unqualified
--     `work_id` in vis_work would bind to m.work_id and defeat work-scope RLS
--     entirely; work_external_identifiers.work_id keeps the outer binding.

ALTER TABLE public.work_external_identifiers ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.manifestation_external_identifiers ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.manifestation_external_ratings ENABLE ROW LEVEL SECURITY;

-- work_external_identifiers: manual writes by reverie_app (admin/adult),
-- enrichment by reverie_ingestion; readable per work visibility.
CREATE POLICY work_external_identifiers_select ON public.work_external_identifiers
    FOR SELECT TO reverie_app, reverie_readonly
    USING (EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.work_id = work_external_identifiers.work_id)));

CREATE POLICY work_external_identifiers_insert ON public.work_external_identifiers
    FOR INSERT TO reverie_app
    WITH CHECK ((EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.work_id = work_external_identifiers.work_id))) AND (EXISTS ( SELECT 1
       FROM public.users
      WHERE ((users.id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));

CREATE POLICY work_external_identifiers_update ON public.work_external_identifiers
    FOR UPDATE TO reverie_app
    USING ((EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.work_id = work_external_identifiers.work_id))) AND (EXISTS ( SELECT 1
       FROM public.users
      WHERE ((users.id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))))
    WITH CHECK ((EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.work_id = work_external_identifiers.work_id))) AND (EXISTS ( SELECT 1
       FROM public.users
      WHERE ((users.id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));

CREATE POLICY work_external_identifiers_delete ON public.work_external_identifiers
    FOR DELETE TO reverie_app
    USING ((EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.work_id = work_external_identifiers.work_id))) AND (EXISTS ( SELECT 1
       FROM public.users
      WHERE ((users.id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));

CREATE POLICY work_external_identifiers_ingestion_full_access ON public.work_external_identifiers
    TO reverie_ingestion USING (true) WITH CHECK (true);

-- manifestation_external_identifiers: same shape with vis_manif substituted.
CREATE POLICY manifestation_external_identifiers_select ON public.manifestation_external_identifiers
    FOR SELECT TO reverie_app, reverie_readonly
    USING (EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.id = manifestation_external_identifiers.manifestation_id)));

CREATE POLICY manifestation_external_identifiers_insert ON public.manifestation_external_identifiers
    FOR INSERT TO reverie_app
    WITH CHECK ((EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.id = manifestation_external_identifiers.manifestation_id))) AND (EXISTS ( SELECT 1
       FROM public.users
      WHERE ((users.id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));

CREATE POLICY manifestation_external_identifiers_update ON public.manifestation_external_identifiers
    FOR UPDATE TO reverie_app
    USING ((EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.id = manifestation_external_identifiers.manifestation_id))) AND (EXISTS ( SELECT 1
       FROM public.users
      WHERE ((users.id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))))
    WITH CHECK ((EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.id = manifestation_external_identifiers.manifestation_id))) AND (EXISTS ( SELECT 1
       FROM public.users
      WHERE ((users.id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));

CREATE POLICY manifestation_external_identifiers_delete ON public.manifestation_external_identifiers
    FOR DELETE TO reverie_app
    USING ((EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.id = manifestation_external_identifiers.manifestation_id))) AND (EXISTS ( SELECT 1
       FROM public.users
      WHERE ((users.id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid) AND (users.role = ANY (ARRAY['admin'::public.user_role, 'adult'::public.user_role]))))));

CREATE POLICY manifestation_external_identifiers_ingestion_full_access ON public.manifestation_external_identifiers
    TO reverie_ingestion USING (true) WITH CHECK (true);

-- manifestation_external_ratings: written only by enrichment; users read-only.
-- No reverie_app write policy exists, so default-deny blocks every user write.
CREATE POLICY manifestation_external_ratings_select ON public.manifestation_external_ratings
    FOR SELECT TO reverie_app, reverie_readonly
    USING (EXISTS ( SELECT 1
       FROM public.manifestations m
      WHERE (m.id = manifestation_external_ratings.manifestation_id)));

CREATE POLICY manifestation_external_ratings_ingestion_full_access ON public.manifestation_external_ratings
    TO reverie_ingestion USING (true) WITH CHECK (true);

-- ---- 7) settings: provider visibility + revision counter ----

-- provider_visibility: per-provider display show/hide, keys validated at the app
-- layer against the identifier_schemes + rating_sources union. revision: a
-- DB-generated monotonic version counter (incremented in the row update itself)
-- so the shared settings cache is a monotonic replica independent of both
-- lock-acquisition order and transaction-start timestamps (now() is
-- transaction-start time and can go backwards across concurrent writers).
ALTER TABLE public.settings
    ADD COLUMN provider_visibility jsonb DEFAULT '{}'::jsonb NOT NULL,
    ADD COLUMN revision bigint DEFAULT 0 NOT NULL;

-- ---- 8) grants ----

-- Reference vocabularies: SELECT to all roles (mirrors metadata_sources), which
-- provider_visibility validation reads.
GRANT SELECT ON TABLE public.identifier_schemes TO reverie_app;
GRANT SELECT ON TABLE public.identifier_schemes TO reverie_ingestion;
GRANT SELECT ON TABLE public.identifier_schemes TO reverie_readonly;

GRANT SELECT ON TABLE public.rating_sources TO reverie_app;
GRANT SELECT ON TABLE public.rating_sources TO reverie_ingestion;
GRANT SELECT ON TABLE public.rating_sources TO reverie_readonly;

-- Identifier tables follow the junction template (app + ingestion full CRUD,
-- readonly SELECT). RLS is the gate; grants match intent.
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.work_external_identifiers TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.work_external_identifiers TO reverie_ingestion;
GRANT SELECT ON TABLE public.work_external_identifiers TO reverie_readonly;

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_external_identifiers TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_external_identifiers TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_external_identifiers TO reverie_readonly;

-- Ratings: reverie_app SELECT only (read-only intent; ratings are
-- enrichment-written); reverie_ingestion SELECT,INSERT,UPDATE (the upsert);
-- readonly SELECT.
GRANT SELECT ON TABLE public.manifestation_external_ratings TO reverie_app;
GRANT SELECT,INSERT,UPDATE ON TABLE public.manifestation_external_ratings TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_external_ratings TO reverie_readonly;
