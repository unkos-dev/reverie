-- Contributors generalization + subtitle + pages.
-- 1) New nullable columns (catalog-only, no rewrite) + author-sort index.
-- 2) Backfill works.first_author_sort_name from role='author' rows.
-- 3) Rewrite legacy "creators" journal rows into per-role contributors.* rows,
--    preserving work_authors.source_version_id continuity per role.
-- RLS policies, grants, and updated_at triggers on works/manifestations cover
-- all columns and are untouched (new columns inherit them).

ALTER TABLE public.works
    ADD COLUMN subtitle text,
    ADD COLUMN subtitle_version_id uuid,
    ADD COLUMN first_author_sort_name text,
    ADD CONSTRAINT works_subtitle_version_id_fkey
        FOREIGN KEY (subtitle_version_id)
        REFERENCES public.metadata_versions(id) ON DELETE SET NULL;

ALTER TABLE public.manifestations
    ADD COLUMN pages integer,
    ADD COLUMN pages_version_id uuid,
    ADD CONSTRAINT manifestations_pages_positive
        CHECK (pages IS NULL OR pages > 0),
    ADD CONSTRAINT manifestations_pages_version_id_fkey
        FOREIGN KEY (pages_version_id)
        REFERENCES public.metadata_versions(id) ON DELETE SET NULL;

-- Every canonical scalar pairs with a *_version_id pointer (accept/revert
-- machinery + Versions tab depend on it); partial indexes mirror
-- idx_works_description_version_id and siblings.
CREATE INDEX idx_works_subtitle_version_id
    ON public.works USING btree (subtitle_version_id)
    WHERE (subtitle_version_id IS NOT NULL);
CREATE INDEX idx_manifestations_pages_version_id
    ON public.manifestations USING btree (pages_version_id)
    WHERE (pages_version_id IS NOT NULL);

-- Mirror of idx_works_sort_title_id; NULLS sort last under ASC by default.
CREATE INDEX idx_works_first_author_sort_id
    ON public.works USING btree (first_author_sort_name, id);

UPDATE public.works w
SET first_author_sort_name = (
    SELECT a.sort_name
    FROM public.work_authors wa
    JOIN public.authors a ON a.id = wa.author_id
    WHERE wa.work_id = w.id AND wa.role = 'author'
    ORDER BY wa.position ASC
    LIMIT 1
);

-- ---- journal rewrite: "creators" -> per-role "contributors.<role>" ----
-- Legacy shapes under field_name='creators':
--   source='opf'                       : [{name, sort_name, role}, ...]
--   source in (enrichment source ids)  : ["Name", ...]  (author role only)
-- New shape: array of name strings, array order = position order.

CREATE TEMP TABLE creators_split ON COMMIT DROP AS
WITH exploded AS (
    SELECT mv.id AS old_id, mv.manifestation_id, mv.source, mv.status,
           mv.confidence_score, mv.created_at, mv.resolved_at, mv.resolved_by,
           mv.match_type, mv.first_seen_at, mv.last_seen_at, mv.observation_count,
           CASE WHEN jsonb_typeof(e.elem) = 'object'
                THEN COALESCE(e.elem->>'role', 'author') ELSE 'author' END AS role,
           CASE WHEN jsonb_typeof(e.elem) = 'object'
                THEN e.elem->>'name' ELSE e.elem #>> '{}' END AS name,
           e.ord
    FROM public.metadata_versions mv,
         LATERAL jsonb_array_elements(mv.new_value) WITH ORDINALITY AS e(elem, ord)
    WHERE mv.field_name = 'creators'
)
SELECT uuidv7() AS new_id,
       old_id, manifestation_id, source, status, confidence_score, created_at,
       resolved_at, resolved_by, match_type, first_seen_at, last_seen_at,
       observation_count,
       'contributors.' || role AS field_name,
       jsonb_agg(to_jsonb(name) ORDER BY ord) AS new_value
FROM exploded
WHERE name IS NOT NULL AND btrim(name) <> ''
GROUP BY old_id, manifestation_id, source, status, confidence_score, created_at,
         resolved_at, resolved_by, match_type, first_seen_at, last_seen_at,
         observation_count, role;

-- value_hash replicates the Rust hasher for string arrays: items trimmed,
-- sorted bytewise by their JSON encoding, joined compact. COLLATE "C" = byte order.
ALTER TABLE creators_split ADD COLUMN value_hash bytea;
UPDATE creators_split cs
SET value_hash = sha256(convert_to(
    '[' || (
        SELECT string_agg(t.j, ',' ORDER BY t.j COLLATE "C")
        FROM (
            SELECT to_jsonb(btrim(el #>> '{}'))::text AS j
            FROM jsonb_array_elements(cs.new_value) el
        ) t
    ) || ']', 'UTF8'));

INSERT INTO public.metadata_versions
    (id, manifestation_id, source, field_name, new_value, status, confidence_score,
     created_at, resolved_at, resolved_by, value_hash, match_type,
     first_seen_at, last_seen_at, observation_count)
SELECT new_id, manifestation_id, source, field_name, new_value, status,
       confidence_score, created_at, resolved_at, resolved_by, value_hash,
       match_type, first_seen_at, last_seen_at, observation_count
FROM creators_split
ON CONFLICT (manifestation_id, source, field_name, value_hash) DO NOTHING;

-- Re-point work_authors at the per-role row (join through the conflict key so
-- rows collapsed by ON CONFLICT above still resolve to the surviving row).
UPDATE public.work_authors wa
SET source_version_id = mv.id
FROM creators_split cs
JOIN public.metadata_versions mv
  ON mv.manifestation_id = cs.manifestation_id
 AND mv.source = cs.source
 AND mv.field_name = cs.field_name
 AND mv.value_hash = cs.value_hash
WHERE wa.source_version_id = cs.old_id
  AND cs.field_name = 'contributors.' || wa.role::text;

-- Any pointer not resolvable per role (e.g. its name was filtered as empty).
UPDATE public.work_authors
SET source_version_id = NULL
WHERE source_version_id IN
      (SELECT id FROM public.metadata_versions WHERE field_name = 'creators');

DELETE FROM public.metadata_versions WHERE field_name = 'creators';
