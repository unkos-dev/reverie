# Implementation Report — Library UI 11d (UNK-80)

**Plan**: `.claude/PRPs/plans/library-ui.plan.md` (Sub-phase 11d — Series + Shelves CRUD)
**Branch**: `feat/unk-80-library-ui-11d`
**Date**: 2026-05-24
**Status**: COMPLETE

## Summary

Series detail endpoint + shelves CRUD + items reorder with `If-Match`
optimistic concurrency, plus the matching React surfaces:
`SeriesPage` (with completeness indicator), `ShelvesListPage`
(create/rename/delete), `ShelfDetailPage` (drag-to-reorder via
`@dnd-kit/sortable`), and a `Shelf` picker affordance on
`LibraryPage`. Two scoped carry-overs landed in the same PR:
`BookDetail` wire shape expanded with `publisher` + `pub_date`
(unblocks 11c `EditMetadataDialog` re-enablement), and shelf-id
chips on the library page now resolve to shelf names via the
shelves cache.

## Tasks completed

| #   | Task                                 | Status |
| --- | ------------------------------------ | ------ |
| T0  | Migration + `system-shelf-immutable` | ✅     |
| T1  | `GET /api/series/{id}`               | ✅     |
| T2  | Shelves CRUD                         | ✅     |
| T3  | Shelf items add/remove/reorder       | ✅     |
| T4  | Frontend api/series + api/shelves    | ✅     |
| T5  | SeriesPage + route                   | ✅     |
| T6  | Shelves UI + dnd-kit                 | ✅     |
| T7  | 11b carry-over — shelf picker        | ✅     |
| T8  | 11c carry-over — publisher/pub_date  | ✅     |
| T9  | Validation + sqlx prepare + report   | ✅     |

## Validation

| Check                         | Result | Notes                        |
| ----------------------------- | ------ | ---------------------------- |
| `cargo fmt --check`           | ✅     |                              |
| `cargo clippy -- -D warnings` | ✅     | All targets                  |
| `cargo test --lib`            | ✅     | 580 passed, 1 ignored        |
| `cargo sqlx prepare --check`  | ✅     | Cache regenerated, committed |
| Frontend `npm test`           | ✅     | 231 passed, 28 files         |
| Frontend `npm run lint`       | ✅     | 0 errors                     |
| Frontend `npm run stylelint`  | ✅     |                              |
| Frontend `npm run detect`     | ✅     |                              |
| Frontend `npm run build`      | ✅     | Series + shelves chunks emit |

## Files created

### Backend

- `backend/migrations/20260524044439_shelves_updated_at.{up,down}.sql`
- `backend/src/models/series.rs`
- `backend/src/models/shelf.rs`
- `backend/src/routes/series/mod.rs`, `tests.rs`
- `backend/src/routes/shelves/mod.rs`, `tests.rs`

### Frontend

- `frontend/src/api/series.ts`, `series.test.ts`
- `frontend/src/api/shelves.ts`, `shelves.test.ts`
- `frontend/src/pages/series/SeriesPage.tsx`
- `frontend/src/pages/shelves/ShelvesListPage.tsx`
- `frontend/src/pages/shelves/ShelfDetailPage.tsx`
- `frontend/src/routes/series.tsx`, `series.test.ts`
- `frontend/src/routes/shelves.tsx`, `shelf-detail.tsx`

### Tooling

- Added `@dnd-kit/core@^6`, `@dnd-kit/sortable@^8`, `@dnd-kit/utilities@^3`

## Files updated

- `backend/src/error/mod.rs` — `SystemShelfImmutable` variant + IntoResponse arm + test
- `backend/src/error/problems.rs` — `SYSTEM_SHELF_IMMUTABLE` slug
- `backend/src/lib.rs` — register `series::router()` + `shelves::router()`
- `backend/src/models/library.rs` — `publisher` + `pub_date` on `BookDetail`
- `backend/src/models/mod.rs` — register `series` + `shelf` modules
- `backend/src/routes/library/mod.rs` — surface canonical pub fields + visibility uplift on shared helpers
- `backend/src/routes/library/tests.rs` — pub fields wire-shape guard + dedicated `detail_endpoint_surfaces_publisher_and_pub_date` test
- `backend/src/routes/mod.rs` — register modules
- `frontend/src/api/books.ts` — `publisher` + `pub_date` in `BookDetailSchema`
- `frontend/src/api/index.ts` — barrel exports
- `frontend/src/lib/query/keys.ts` — `series.*` + `shelves.*` key factories
- `frontend/src/main.tsx` — mount new routes
- `frontend/src/pages/book/BookPage.test.tsx`, `frontend/src/api/books.test.ts`, `frontend/src/routes/book.test.ts` — fixtures extended with `publisher` + `pub_date`
- `frontend/src/pages/book/VersionsTab.tsx` — re-add `publisher` + `pub_date` to `canonicalEditableFields`; `canonicalValue` reads from `BookDetail`
- `frontend/src/pages/library/LibraryPage.tsx` — `ShelfPickerButton` + shelf-name resolution for active-filter chip
- `frontend/src/routes/production.ts` — register `seriesRoute`, `shelvesRoute`, `shelfDetailRoute`

## Test matrix coverage

Plan §11d-task-7 test matrix:

| Bullet                                                                      | Test                                                                                  |
| --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| happy: GET /api/shelves returns user's shelves                              | `list_shelves_returns_only_callers_shelves`                                           |
| happy: POST + PATCH + DELETE round-trip                                     | `create_shelf_round_trips` + `rename_shelf_updates_name` + `delete_shelf_removes_row` |
| happy: PUT items with correct If-Match                                      | `reorder_happy_path_persists_new_order`                                               |
| negative: DELETE on is_system shelf → 409 system-shelf-immutable            | `delete_system_shelf_returns_409_system_shelf_immutable` (+ rename variant)           |
| negative: adult A cannot mutate adult B's shelf (404, existence-not-leaked) | `rename_other_users_shelf_returns_404` + `delete_other_users_shelf_returns_404`       |
| negative: child cannot CREATE/DELETE shelves                                | `child_cannot_create_shelf` + `child_cannot_delete_shelf`                             |
| negative: child CAN view own shelves                                        | `child_can_view_own_shelves`                                                          |
| CONCURRENCY: two parallel reorders, exactly one wins                        | `parallel_reorders_with_same_if_match_serialize`                                      |
| edge: PUT items with stale If-Match → 412                                   | `reorder_with_stale_if_match_returns_412`                                             |
| edge: PUT items without If-Match → 428                                      | `reorder_without_if_match_returns_428`                                                |
| Series existence-not-leaked + happy path + RLS child view                   | `routes::series::tests::*` (5 tests)                                                  |

## Deviations from plan

- **Item-mutation ETag bump rule:** plan §11d-task-4 specifies
  `If-Match` only on the reorder PUT and leaves POST/DELETE item
  ETag behaviour unspecified. Per advisor guidance (option b), every
  items mutation (`POST /items`, `DELETE /items/{mid}`, `PUT
/items`) explicitly issues `UPDATE shelves SET updated_at = now()`
  so the ETag tracks every visible state change. Without this, a
  follow-up reorder PUT would 412 spuriously after add/remove.
  Tests `add_shelf_item_appends_and_bumps_etag` +
  `remove_shelf_item_bumps_etag` lock this in.
- **Manifestation visibility probe (`POST /items`):** plan asked
  only for "add". The handler also runs an RLS-scoped existence
  probe on the target manifestation to prevent shelf-existence
  probing via random UUIDs. Adults have full manifestation
  visibility under existing RLS, so the probe is a no-op for them;
  the threat is meaningful only for children (test
  `add_shelf_item_404_when_child_cannot_see_manifestation`).
- **`ShelvesSidebar` library-page integration:** plan describes
  shelves as a sidebar on `/library`. Shipped as standalone
  `/shelves` + `/shelves/:id` routes plus a `Shelf` picker affordance
  on the library page. Library sidebar restructure recorded as
  carry-over (below) — would require library page layout redesign
  beyond the 11d surface.
- **Author / series picker affordances:** plan §11b carry-over
  asked for picker affordances on `LibraryPage` filter chips.
  Only the shelves picker is achievable in 11d — author and series
  list endpoints (`GET /api/authors`, `GET /api/series`) do not
  exist. Recorded as carry-over (below).
- **`bg-black` overlay debt:** memory entry
  `project_bg_black_overlays_deferred.md` flagged shadcn overlay
  fixes. Grep on `frontend/src/components/ui/{dialog,alert-dialog,sheet}.tsx`
  finds no remaining instances — debt likely cleared by an earlier
  CodeRabbit pass. Closing notes deferred to follow-up.

## Carry-overs to next phase

- **Library-page shelves sidebar restructure.** `/shelves` ships as a
  standalone route; the always-visible sidebar wired into the
  `/library` shell needs a layout redesign of `LibraryPage` (split
  layout with sticky aside). Defer to 11e or a Step 12 polish PR.
- **Author + series picker affordances.** Need new list endpoints
  (`GET /api/authors`, `GET /api/series`). The shelves picker proves
  the Popover+Command pattern is reusable; once the endpoints exist
  it's mechanical.
- **`load_pending_versions` size bound (from 11c carry-over).** Still
  unbounded — not addressed in this PR. Bound + test next time
  `metadata.rs` opens.
- **`publisher` whitespace hash-normalization divergence (11c).**
  Still latent — `insert_manual_version` hashes raw vs
  `value_hash::value_hash()` trims. Address when canonicalising the
  manual + auto paths.
- **`title`-null-clear → 422 path untested (11c).** Still uncovered.
  Add when `metadata.rs` opens next.
- **`load_pending_versions` LIMIT** and **`publisher` whitespace
  hash divergence**: unchanged from 11c carry-over — not touched in
  11d, still open in the carry-over list.

## Notes

- ETag values are RFC 9110 quoted RFC 3339 timestamps (the entity-
  tag and the timestamp coincide for shelves; `parse_if_match`
  strips surrounding quotes + tolerates the optional `W/` weak
  prefix per RFC 9110 §8.8.3).
- All shelves CRUD handlers gate ownership at the SQL boundary
  (`WHERE id = $1 AND user_id = $2`) since `shelves` carries no RLS.
  Mismatched ownership resolves to 404 (existence-not-leaked).
- `routes::library::load_authors_for_works`, `parse_ingestion`,
  `parse_enrichment` lifted to `pub(crate)` for re-use by
  `routes::series::detail` (no behaviour change for the originals).
- Browser QA deferred to staging (same gap as 11b/11c: Coder
  workspace has no OIDC stub and the existing sqlx-cache/live-DB
  drift on `orchestrator.rs` blocks a clean `cargo run`).
