-- Reverse of external identifiers + ratings + provider visibility.
--
-- Asymmetry (intentional, keeps up/down idempotent): the additive
-- metadata_sources seed rows (amazon, goodreads, librarything, asin, wikidata,
-- calibre) are deliberately NOT deleted here. A DELETE would fail once a real
-- manual observation has referenced one via metadata_versions.source, and a
-- re-applied up would re-INSERT them; they are harmless additive vocabulary.
-- Leaving them is what makes the up seed's ON CONFLICT (id) DO NOTHING
-- load-bearing: the two together keep revert-then-reapply clean.

-- Drop child tables (they FK identifier_schemes / rating_sources) before the
-- reference tables. RLS policies drop with their tables.
DROP TABLE public.manifestation_external_ratings;
DROP TABLE public.manifestation_external_identifiers;
DROP TABLE public.work_external_identifiers;

-- Reference vocabularies. rating_sources FKs metadata_sources (which stays).
DROP TABLE public.rating_sources;
DROP TABLE public.identifier_schemes;

ALTER TABLE public.settings
    DROP COLUMN revision,
    DROP COLUMN provider_visibility;
