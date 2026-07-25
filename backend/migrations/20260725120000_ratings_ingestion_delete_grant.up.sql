-- Enrichment owns the full lifecycle of the ratings cache, including the
-- removal case: a rating-capable provider record that no longer reports a
-- rating must be able to clear the cached row, or the projection serves the
-- obsolete score indefinitely. The original grant covered only the upsert
-- (SELECT, INSERT, UPDATE); RLS already permits the delete through the
-- ingestion role's ALL policy, so only the table grant was missing.
-- reverie_app stays SELECT-only: users still cannot write ratings.

GRANT DELETE ON TABLE public.manifestation_external_ratings TO reverie_ingestion;
