-- UNK-368: indexing review — btree indexes for hot ORDER BY / JOIN sort keys
-- that the schema did not yet cover (ADR-7 / CLAUDE.md indexing discipline:
-- index keyset sort keys + frequent ORDER BY/JOIN columns; do not over-index
-- write-heavy columns). FK coverage was already complete (38/38) before this
-- migration; every index below backs a sort or correlated-lookup hot path,
-- eliminating an in-memory Sort node that the existing indexes left in place.

-- HIGH — default library list + OPDS /new feed. GET /api/v1/books with no sort
-- (SortMode::Recent) and OPDS author/series/new feeds all emit
-- ORDER BY m.created_at DESC, m.id DESC (routes/library/mod.rs, routes/opds).
-- No existing index covers this; the scan was Seq Scan + Sort. The composite
-- also backs the Recent keyset continuation WHERE (created_at, id) < (cursor).
CREATE INDEX idx_manifestations_recent_keyset
    ON public.manifestations USING btree (created_at DESC, id DESC);

-- MEDIUM — per-work author ordering. The SortMode::Author correlated subquery
-- (ORDER BY wa.position ASC LIMIT 1, run per outer row) and the batch
-- load_authors_for_works (WHERE work_id = ANY($1) ORDER BY wa.position) both
-- sort on position, which the PK (work_id, author_id, role) does not order.
CREATE INDEX idx_work_authors_work_position
    ON public.work_authors USING btree (work_id, position ASC);

-- MEDIUM — pending metadata versions for the book-detail review panel.
-- load_pending_versions: WHERE manifestation_id = $1 AND status = 'pending'
-- ORDER BY last_seen_at DESC (the query also post-filters new_value != 'null'
-- and NOT id = ANY($canonical) and applies a LIMIT — neither affects index
-- suitability; they are applied on the index-scan result). The existing
-- idx_mv_manifestation_field (manifestation_id, field_name) covers the equality
-- but not the status filter or the sort. Partial on status = 'pending' keeps the
-- index to actionable rows only, so the high-frequency status transitions on the
-- rest of the table do not pay for it.
CREATE INDEX idx_mv_pending_last_seen
    ON public.metadata_versions USING btree (manifestation_id, last_seen_at DESC)
    WHERE status = 'pending'::public.metadata_review_status;

-- LOW — OPDS series-books feed: WHERE sw.series_id = $1
-- ORDER BY sw.position ASC NULLS LAST, m.created_at DESC, m.id DESC. PK
-- (series_id, work_id) covers the equality but not the position sort. The index
-- supplies (series_id, position ASC NULLS LAST); the trailing m.created_at/m.id
-- keys are on the joined manifestations table and cannot live in a series_works
-- index, so Postgres incremental-sorts those within each position tie-group.
CREATE INDEX idx_series_works_series_position
    ON public.series_works USING btree (series_id, position ASC NULLS LAST);

-- LOW — active device tokens per user, read on every Basic-auth request:
-- WHERE user_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC.
-- Partial on revoked_at IS NULL covers only live tokens (most are revoked in a
-- mature install) and supplies the created_at order without a Sort.
CREATE INDEX idx_device_tokens_user_active
    ON public.device_tokens USING btree (user_id, created_at DESC)
    WHERE revoked_at IS NULL;
