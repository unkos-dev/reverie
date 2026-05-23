# Implementation Report — 11a-A.4

**Plan**: `/home/coder/reverie/.claude/PRPs/plans/library-ui.plan.md` (Tasks 5, 6, 7, 8)
**Source issue**: UNK-80 (umbrella) — 11a-A.4 slice
**Branch**: `feat/unk-80-library-ui-11a-A.4`
**Date**: 2026-05-23
**Status**: COMPLETE

---

## Summary

Lands `GET /api/books/{id}` (book detail w/ work prose, tags, metadata-version
summary) and `GET /api/works/{id}` (work prose + all RLS-visible
manifestations of the work). Consumes the `BookDetail`, `MetadataVersionSummary`,
`WorkDetail`, `WorkManifestation` DTOs introduced (with `#[allow(dead_code)]`)
in 11a-A.3. `[allow(dead_code, reason = "consumed by 11a-A.4")]` removed from
each.

Existence-not-leaked invariant honoured at two seams: detail returns 404
when RLS hides the manifestation; work returns 404 when no manifestation
of the work is RLS-visible (existence probe under the RLS transaction,
collapsing to 404 before fetching the work row).

---

## Assessment vs Reality

| Metric     | Predicted (plan)      | Actual | Reasoning                                                                                                                                                                                                |
| ---------- | --------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Complexity | MEDIUM (3 plan tasks) | MEDIUM | Detail query needed broader column set than plan sketch (canonical pointers in same row → no second round-trip for accepted-count); refactor for `clippy::too_many_lines` was straightforward extraction |
| Confidence | HIGH                  | HIGH   | Plan patterns mirrored cleanly; the only surprise was that the `metadata_review_status` enum is now `pending\|rejected` (canonical pointers carry "accepted") — folded into the summary semantics        |

---

## Tasks Completed

| #   | Task                                                              | File                                        | Status |
| --- | ----------------------------------------------------------------- | ------------------------------------------- | ------ |
| 5   | RED tests: `GET /api/books/{id}` (happy/hidden/malformed-UUID)    | `backend/src/routes/library/tests.rs`       | ✅     |
| 6   | GREEN handler: `GET /api/books/{id}` w/ metadata-version summary  | `backend/src/routes/library/mod.rs`         | ✅     |
| 7   | RED+GREEN: `GET /api/works/{id}` w/ RLS-existence gate            | `backend/src/routes/library/{mod,tests}.rs` | ✅     |
| 8   | fmt + clippy `-D warnings` + full lib test + sqlx prepare --check | (CI parity locally)                         | ✅     |

---

## Validation Results

| Check                                               | Result | Details                                                                                                |
| --------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------ |
| `cargo fmt --check`                                 | ✅     | clean                                                                                                  |
| `cargo clippy --all-targets -- -D warnings`         | ✅     | 0 errors (refactored `detail` to <100 lines to satisfy `too_many_lines`)                               |
| `cargo test --lib`                                  | ✅     | 525 passed; 0 failed (19 in `routes::library::tests`)                                                  |
| `cargo sqlx prepare --workspace --check -- --tests` | ✅     | clean after regen; 4 unrelated `.sqlx/*.json` formatting deltas reverted to keep the slice diff scoped |

No `backend/tests/` integration target exists; all tests live in `#[cfg(test)]` modules — `cargo test --lib` covers the full suite.

---

## Files Changed

| File                                  | Action | Notes                                                                                                                                                                                       |
| ------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `backend/src/routes/library/mod.rs`   | UPDATE | New routes `/api/books/{id}` + `/api/works/{id}`; `detail`, `fetch_detail_row`, `load_manifestation_tags`, `count_pending_versions`, `accepted_pointer_count`, `work_detail`                |
| `backend/src/routes/library/tests.rs` | UPDATE | +6 tests for the new endpoints                                                                                                                                                              |
| `backend/src/models/library.rs`       | UPDATE | Dropped `#[allow(dead_code, reason="…11a-A.4 slice")]` from `BookDetail`, `MetadataVersionSummary`, `WorkDetail`, `WorkManifestation`; tightened `MetadataVersionSummary` doc to match impl |
| `backend/.sqlx/query-*.json`          | CREATE | 11 new prepared-query cache files for the new `sqlx::query!` / `query_scalar!` sites                                                                                                        |

---

## Deviations from Plan

1. **Plan Task 6 says `sqlx::query_as!`; implementation uses `sqlx::query!` + manual map into a `DetailRow` struct.** Functionally equivalent (compile-time-checked SQL + types), and the manual map gave a clean place to bundle the canonical-pointer columns the `accepted` count reads. No behaviour difference; struct `DetailRow` is private to the module.
2. **`metadata_version_summary.accepted` counts canonical-pointer SLOTS filled (max 8) rather than distinct accepted-version IDs.** In the current schema, each `*_version_id` slot binds one `metadata_versions` row whose `(manifestation_id, field_name)` is unique to that slot, so the count of filled slots equals the count of distinct accepted versions in play. The `MetadataVersionSummary` docstring is rewritten to match the impl ("Number of fields whose canonical pointer is currently set").

---

## Issues Encountered

- **DB unreachable on `localhost:5433`** — workspace network attached to the docker compose network on a prior session; used `postgres://reverie:reverie@reverie-postgres:5432/reverie_dev` for the sqlx-online compile path. Pre-existing constraint (project memory `project_docker_network_workaround.md`).
- **`metadata_versions.match_type NOT NULL`** — first iteration of the detail-happy test omitted the column, fixture insert failed. Added explicit `'title'` value; matches what enrichment writes.
- **`clippy::too_many_lines`** — initial `detail` handler was 123 LOC. Extracted `fetch_detail_row`, `load_manifestation_tags`, `count_pending_versions`, `accepted_pointer_count` helpers; main handler now under the cap.

---

## Tests Written

| Test File                             | Test Cases                                                                                                                                                                                                                                                                                             |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `backend/src/routes/library/tests.rs` | `detail_endpoint_returns_book_with_version_summary`, `detail_endpoint_hidden_id_returns_404`, `detail_endpoint_malformed_uuid_returns_400`, `work_endpoint_returns_work_with_manifestations`, `work_endpoint_hidden_work_returns_404`, `work_endpoint_child_without_shelf_returns_404_not_empty_array` |

Security regression guards:

- `detail_endpoint_hidden_id_returns_404` — child user, manifestation not on their shelves → 404 (NOT 403).
- `work_endpoint_child_without_shelf_returns_404_not_empty_array` — work row exists but its manifestation is RLS-hidden → 404 (NOT 200 with `manifestations: []`).

---

## Next Steps

- [x] Local validation (fmt / clippy / test / sqlx prepare --check) all green.
- [ ] Commit + push branch.
- [ ] Open PR titled `feat(api): GET /api/books/{id} + /api/works/{id} (UNK-80, 11a-A.4)`.
- [ ] Hand off — user reviews and merges.

Plan file `library-ui.plan.md` deliberately NOT archived — 11a → 11f remains in flight; slice A.4 is one of multiple 11a sub-PRs.
