-- Genres + moods vocabularies, content_rating enum, tags de-typing, trgm indexes.
-- 1) content_rating enum + lockable manifestations column with journal pointer.
-- 2) genres/moods vocabulary tables + junction tables mirroring manifestation_tags
--    (composite PK, CASCADE FKs, per-row source_version_id journal pointer).
-- 3) tags de-typing: with genres and moods as dedicated vocabularies, typed tags
--    are dead weight; tags collapse to a flat vocabulary unique on lower(name).
-- 4) GIN trgm indexes for typeahead suggest (prefix ILIKE + word_similarity) on
--    genres/moods/tags names and manifestations.publisher.
-- RLS: none on vocabulary/junction tables (matches manifestation_tags); row
-- visibility is mediated by joining through RLS-governed manifestations.

-- ---- 1) content_rating ----

CREATE TYPE public.content_rating AS ENUM (
    'everyone',
    'teen',
    'mature',
    'adult',
    'explicit'
);

ALTER TABLE public.manifestations
    ADD COLUMN content_rating public.content_rating,
    ADD COLUMN content_rating_version_id uuid,
    ADD CONSTRAINT manifestations_content_rating_version_id_fkey
        FOREIGN KEY (content_rating_version_id)
        REFERENCES public.metadata_versions(id) ON DELETE SET NULL;

CREATE INDEX idx_manifestations_content_rating_version_id
    ON public.manifestations USING btree (content_rating_version_id)
    WHERE (content_rating_version_id IS NOT NULL);

-- ---- 2) genres / moods vocabularies + junctions ----

CREATE TABLE public.genres (
    id uuid DEFAULT uuidv7() NOT NULL,
    name text NOT NULL,
    CONSTRAINT genres_pkey PRIMARY KEY (id)
);

CREATE TABLE public.moods (
    id uuid DEFAULT uuidv7() NOT NULL,
    name text NOT NULL,
    CONSTRAINT moods_pkey PRIMARY KEY (id)
);

-- Case-insensitive uniqueness: case-variant duplicates are the exact dedup
-- failure the suggest endpoints exist to prevent. Find-or-create targets this
-- index via ON CONFLICT ((lower(name))).
CREATE UNIQUE INDEX idx_genres_name_lower ON public.genres ((lower(name)));
CREATE UNIQUE INDEX idx_moods_name_lower ON public.moods ((lower(name)));

CREATE TABLE public.manifestation_genres (
    manifestation_id uuid NOT NULL,
    genre_id uuid NOT NULL,
    source_version_id uuid,
    CONSTRAINT manifestation_genres_pkey PRIMARY KEY (manifestation_id, genre_id),
    CONSTRAINT manifestation_genres_manifestation_id_fkey
        FOREIGN KEY (manifestation_id)
        REFERENCES public.manifestations(id) ON DELETE CASCADE,
    CONSTRAINT manifestation_genres_genre_id_fkey
        FOREIGN KEY (genre_id)
        REFERENCES public.genres(id) ON DELETE CASCADE,
    CONSTRAINT manifestation_genres_source_version_id_fkey
        FOREIGN KEY (source_version_id)
        REFERENCES public.metadata_versions(id) ON DELETE SET NULL
);

CREATE TABLE public.manifestation_moods (
    manifestation_id uuid NOT NULL,
    mood_id uuid NOT NULL,
    source_version_id uuid,
    CONSTRAINT manifestation_moods_pkey PRIMARY KEY (manifestation_id, mood_id),
    CONSTRAINT manifestation_moods_manifestation_id_fkey
        FOREIGN KEY (manifestation_id)
        REFERENCES public.manifestations(id) ON DELETE CASCADE,
    CONSTRAINT manifestation_moods_mood_id_fkey
        FOREIGN KEY (mood_id)
        REFERENCES public.moods(id) ON DELETE CASCADE,
    CONSTRAINT manifestation_moods_source_version_id_fkey
        FOREIGN KEY (source_version_id)
        REFERENCES public.metadata_versions(id) ON DELETE SET NULL
);

CREATE INDEX idx_manifestation_genres_genre_id
    ON public.manifestation_genres USING btree (genre_id);
CREATE INDEX idx_manifestation_moods_mood_id
    ON public.manifestation_moods USING btree (mood_id);

-- Partial pointer indexes mirror idx_manifestation_tags_source_version_id.
CREATE INDEX idx_manifestation_genres_source_version_id
    ON public.manifestation_genres USING btree (source_version_id)
    WHERE (source_version_id IS NOT NULL);
CREATE INDEX idx_manifestation_moods_source_version_id
    ON public.manifestation_moods USING btree (source_version_id)
    WHERE (source_version_id IS NOT NULL);

-- ---- 3) tags de-typing ----

-- Collapse rows that become duplicates once tag_type stops discriminating
-- (same lower(name) across different types). Survivor = smallest id (uuidv7,
-- so oldest). Re-pointing is insert-with-conflict-skip then blanket delete:
-- a single UPDATE would collide on the composite PK when a manifestation
-- carries several rows that collapse to one survivor. When that happens the
-- surviving row's source_version_id is an arbitrary pick among the collapsed
-- rows; acceptable for a journal pointer.
CREATE TEMP TABLE tag_dupes ON COMMIT DROP AS
SELECT id, survivor_id
FROM (
    SELECT id,
           first_value(id) OVER (PARTITION BY lower(name) ORDER BY id) AS survivor_id
    FROM public.tags
) ranked
WHERE id <> survivor_id;

INSERT INTO public.manifestation_tags (manifestation_id, tag_id, source_version_id)
SELECT mt.manifestation_id, d.survivor_id, mt.source_version_id
FROM public.manifestation_tags mt
JOIN tag_dupes d ON mt.tag_id = d.id
ON CONFLICT (manifestation_id, tag_id) DO NOTHING;

DELETE FROM public.manifestation_tags
WHERE tag_id IN (SELECT id FROM tag_dupes);

DELETE FROM public.tags
WHERE id IN (SELECT id FROM tag_dupes);

ALTER TABLE public.tags
    DROP CONSTRAINT tags_name_tag_type_key,
    DROP COLUMN tag_type;

DROP TYPE public.tag_type;

CREATE UNIQUE INDEX idx_tags_name_lower ON public.tags ((lower(name)));

-- ---- 4) trgm indexes for suggest ----

-- GIN over GiST: read-heavy lookup vocabulary, non-lossy, faster searches;
-- GIN trgm also accelerates the ILIKE prefix leg. Existing GiST trgm indexes
-- (authors/series/works) are untouched.
CREATE INDEX idx_genres_name_trgm
    ON public.genres USING gin (name public.gin_trgm_ops);
CREATE INDEX idx_moods_name_trgm
    ON public.moods USING gin (name public.gin_trgm_ops);
CREATE INDEX idx_tags_name_trgm
    ON public.tags USING gin (name public.gin_trgm_ops);
CREATE INDEX idx_manifestations_publisher_trgm
    ON public.manifestations USING gin (publisher public.gin_trgm_ops)
    WHERE (publisher IS NOT NULL);

-- ---- grants (mirror tags / manifestation_tags) ----

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.genres TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.genres TO reverie_ingestion;
GRANT SELECT ON TABLE public.genres TO reverie_readonly;

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.moods TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.moods TO reverie_ingestion;
GRANT SELECT ON TABLE public.moods TO reverie_readonly;

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_genres TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_genres TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_genres TO reverie_readonly;

GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_moods TO reverie_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.manifestation_moods TO reverie_ingestion;
GRANT SELECT ON TABLE public.manifestation_moods TO reverie_readonly;
