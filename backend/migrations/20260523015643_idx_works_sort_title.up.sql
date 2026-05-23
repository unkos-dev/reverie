-- Composite index supporting `ORDER BY w.sort_title ASC, w.id ASC`
-- with `(w.sort_title, w.id) > ($1, $2)` cursor predicate on the JSON
-- `/api/books?sort=title` list endpoint. See plan
-- `.claude/PRPs/plans/library-ui.plan.md` (11a Task 4) and ADR
-- `adr/2026-05-22-json-api-conventions.md`.
CREATE INDEX IF NOT EXISTS idx_works_sort_title_id
    ON works (sort_title, id);
