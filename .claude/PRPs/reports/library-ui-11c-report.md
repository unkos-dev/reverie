# Implementation Report

**Plan**: `.claude/PRPs/plans/library-ui.plan.md` (Phase 11c)
**Source Issue**: UNK-80
**Branch**: `feat/unk-80-library-ui-11c`
**PR**: <https://github.com/unkos-dev/reverie/pull/316>
**Date**: 2026-05-24
**Status**: COMPLETE

---

## Summary

Sub-phase 11c — manual operator metadata edit + accept/reject/revert UI on book detail page. Adds `PATCH /api/books/{id}/metadata` (RFC 7396 JSON Merge Patch), embeds pending `metadata_versions` rows in `BookDetail`, ships `VersionsTab` + `EditMetadataDialog` on the frontend.

---

## Assessment vs Reality

| Metric     | Predicted     | Actual             | Reasoning                                                                                   |
| ---------- | ------------- | ------------------ | ------------------------------------------------------------------------------------------- |
| Complexity | HIGH (parent) | MEDIUM (sub-phase) | Foundation (`apply_version`/`clear_field`) already in place — manual edit reuses them       |
| Confidence | HIGH          | HIGH               | Plan was precise; one scope-creep self-correction (204→200 on legacy endpoints) per advisor |

**Deviations:**

- Plan §11c-task-5 calls for "react-query mutations with optimistic update + invalidation" — shipped invalidation only. Deferred to follow-up; pessimistic default is correct enough for first cut.
- Plan task 6 mentions `<AlertDialog>` "when the change would clear a previously-set field" — implemented as confirm-only on clear of currently-populated fields.
- `apiFetch` taught to tolerate empty 2xx bodies — pre-emptive fix so accept/reject/revert legacy 200+empty contract flows through the central wrapper without `SyntaxError`.

---

## Tasks Completed

| #   | Task                                              | File                                             | Status |
| --- | ------------------------------------------------- | ------------------------------------------------ | ------ |
| 1   | Add `serde_with` dep                              | `backend/Cargo.toml`                             | ✅     |
| 2   | Extend `BookDetail.metadata_versions`             | `backend/src/models/library.rs`                  | ✅     |
| 3   | Wire `load_pending_versions` in detail            | `backend/src/routes/library/mod.rs`              | ✅     |
| 4   | `PATCH /api/books/{id}/metadata` handler          | `backend/src/routes/metadata.rs`                 | ✅     |
| 5   | `insert_manual_version` with `resolved_by`        | `backend/src/routes/metadata.rs`                 | ✅     |
| 6   | 7 PATCH tests (RED → GREEN)                       | `backend/src/routes/metadata.rs`                 | ✅     |
| 7   | `updateBookMetadata` + Zod schema                 | `frontend/src/api/books.ts`                      | ✅     |
| 8   | `acceptVersion` / `rejectVersion` / `revertField` | `frontend/src/api/metadata.ts`                   | ✅     |
| 9   | `VersionsTab.tsx`                                 | `frontend/src/pages/book/VersionsTab.tsx`        | ✅     |
| 10  | `EditMetadataDialog.tsx`                          | `frontend/src/pages/book/EditMetadataDialog.tsx` | ✅     |
| 11  | Empty-2xx-body tolerance in `apiFetch`            | `frontend/src/api/fetch.ts`                      | ✅     |
| 12  | Frontend API client tests                         | `frontend/src/api/{books,metadata}.test.ts`      | ✅     |
| 13  | `BookPage` Versions-tab test rewrite              | `frontend/src/pages/book/BookPage.test.tsx`      | ✅     |

---

## Validation Results

| Check                                    | Result | Details                                |
| ---------------------------------------- | ------ | -------------------------------------- |
| `cargo fmt --check`                      | ✅     | clean                                  |
| `cargo clippy --all-targets -D warnings` | ✅     | 0 errors                               |
| `cargo sqlx prepare --check -- --tests`  | ✅     | cache fresh                            |
| `cargo test`                             | ✅     | 548 passed, 0 failed, 1 ignored        |
| `npm run lint`                           | ✅     | 0 warnings                             |
| `npm test`                               | ✅     | 217 passed                             |
| `npm run build`                          | ✅     | bundles OK                             |
| `npm run detect` (impeccable)            | ✅     | no findings                            |
| Manual browser QA                        | ⏭️     | Deferred to staging per 11b carry-over |

---

## Files Changed

23 files: +1602 / -62 lines

### Backend (new logic)

- `backend/Cargo.toml` — `serde_with` dep
- `backend/src/models/library.rs` — `MetadataVersionRow` struct + field on `BookDetail`
- `backend/src/routes/library/mod.rs` — `load_pending_versions`
- `backend/src/routes/metadata.rs` — `PATCH /api/books/{id}/metadata` + 7 tests

### Frontend (new logic)

- `frontend/src/api/books.ts` — `MetadataVersionRow`, `updateBookMetadata`, `UpdateBookMetadataFields`
- `frontend/src/api/metadata.ts` — accept/reject/revert wrappers
- `frontend/src/api/fetch.ts` — empty-2xx-body tolerance
- `frontend/src/api/index.ts` — barrel exports
- `frontend/src/pages/book/VersionsTab.tsx` — per-field rows with mutators
- `frontend/src/pages/book/EditMetadataDialog.tsx` — manual edit Sheet + AlertDialog confirm

### Tests

- `frontend/src/api/books.test.ts` — `updateBookMetadata` + 404 ApiError
- `frontend/src/api/metadata.test.ts` — accept/reject/revert URL+body shape
- `frontend/src/pages/book/BookPage.test.tsx` — Versions tab assertion rewritten
- `frontend/src/routes/book.test.ts` — fixture extended

### Cache

- `backend/.sqlx/*` — 6 new + 21 reformatted query caches (sqlx-cli prepare)

---

## Deviations from Plan

| Deviation                             | Why                                                                    |
| ------------------------------------- | ---------------------------------------------------------------------- |
| Optimistic update deferred            | Pessimistic + invalidation correct enough for first cut; UX follow-up  |
| No browser QA on `/b/:id`             | Per 11b carry-over — Coder workspace has no OIDC stub                  |
| `apiFetch` empty-body tolerance added | Needed so accept/reject/revert legacy 200+empty contract flows through |

---

## Issues Encountered

1. **Scope creep self-correction** — initially flipped accept/reject/revert handlers from 200→204 to work around `apiFetch` failing on empty 200. Advisor flagged this as an unauthorized contract change to legacy endpoints. Reverted those handlers to 200, kept PATCH at 204 (greenfield), and added empty-body tolerance to `apiFetch` instead.
2. **`Option<Option<T>>` clippy lint** — RFC 7396 sparse-update encoding requires the type. Annotated with `#[allow(clippy::option_option, reason = "…")]`.
3. **prettier on `.sqlx/*.json`** — sqlx-cli's `prepare` output formats differently from prettier defaults; ran `prettier --write` over the regenerated files to satisfy `lint-staged`.

---

## Tests Written

| Test File                                   | Test Cases                                                                                                                                                                                                                                                                           |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `backend/src/routes/metadata.rs::tests`     | `patch_book_metadata_requires_auth`, `patch_sets_title_and_writes_canonical`, `patch_clears_description_with_null`, `patch_with_empty_body_returns_422`, `patch_child_account_forbidden`, `patch_leaves_sibling_pending_drafts_alone`, `patch_returns_404_for_missing_manifestation` |
| `frontend/src/api/books.test.ts`            | `updateBookMetadata > PATCHes /api/books/{id}/metadata with the fields envelope`, `updateBookMetadata > surfaces 422 validation as ApiError`, `getBook > returns 404-bearing ApiError when the manifestation is hidden`                                                              |
| `frontend/src/api/metadata.test.ts`         | `acceptVersion`, `rejectVersion`, `revertField > clear`, `revertField > re-promote`                                                                                                                                                                                                  |
| `frontend/src/pages/book/BookPage.test.tsx` | `Versions tab badges the pending count and exposes the editor`                                                                                                                                                                                                                       |

---

## Next Steps

- [ ] User review + merge of PR #316
- [ ] Sub-phase 11d (series + shelves CRUD) per plan §"Sub-phase 11d"
- [ ] Optimistic update follow-up on metadata mutations (operator-feedback-driven)
