# Implementation Report

**Plan**: `.claude/PRPs/plans/unk-276-validation-status.plan.md`
**Source Issue**: UNK-276
**Branch**: `feat/unk-276-validation-status`
**Date**: 2026-05-28
**Status**: COMPLETE

---

## Summary

Renamed the Postgres `validation_status` enum value `valid` → `clean`
(canonical set `pending | clean | repaired | degraded`), introduced a
typed `ValidationStatus` `sqlx::Type` enum mirroring the
`IngestionStatus`/`EnrichmentStatus` pattern, and retired the last
raw-`String` DB-enum field across the API DTOs, the library/series
routes, and the frontend Zod client. Closes the `sqlx::Type` enum
migration series.

---

## Assessment vs Reality

| Metric     | Predicted | Actual | Reasoning                                                                                                             |
| ---------- | --------- | ------ | --------------------------------------------------------------------------------------------------------------------- |
| Complexity | MEDIUM    | MEDIUM | Mechanical refactor across many sites; no surprises in the type machinery.                                            |
| Confidence | HIGH      | HIGH   | Established internal pattern; the only real risk (runtime QueryBuilder decode) was covered by a value-assertion test. |

### Deviations from plan

1. **Frontend test fixtures (8 sites) updated beyond plan scope.** Plan
   Task 10 only listed `frontend/src/api/books.ts`. Tightening the Zod
   schema from `z.string()` to `z.enum([...])` makes the literal
   `validation_status: "valid"` in 5 test files (`books.test.ts` ×4,
   `series.test.ts`, `LibraryPage.test.tsx`, `book.test.ts`,
   `BookPage.test.tsx`) fail `ZodError`. Changed all to `"clean"`.
   Required for the suite to pass — not optional scope creep.

2. **Module registration position.** Plan said "after series/theme_preference,
   before user". That is not alphabetical. Placed `validation_status`
   correctly between `user` and `work` (alpha order, matching the file's
   existing convention).

3. **Orchestrator test query converted to typed-enum decode.** Plan Task
   7 kept the test's `validation_status::text` cast and only changed the
   asserted string. Acceptance criterion #2 (`no validation_status::text
in backend/src`) conflicted with that. Converted the test
   `query_scalar!` to decode `AS "...: ValidationStatus"` and assert
   `== ValidationStatus::Clean` — satisfies the gate and exercises the
   typed decode.

4. **INSERT-bind comment corrected.** The orchestrator INSERT still binds
   `validation_status` as `($N::text)::validation_status` (a computed
   `&'static str`, per plan). Its comment claimed the field "has no Rust
   counterpart" — now false. Updated the comment; left the bind as-is.

---

## Tasks Completed

| #   | Task                                                                   | Status |
| --- | ---------------------------------------------------------------------- | ------ |
| 1   | CREATE migration pair (`valid`→`clean`)                                | ✅     |
| 2   | CREATE `ValidationStatus` enum + tests + drift probe + allowlist entry | ✅     |
| 3   | Register module in `models/mod.rs`                                     | ✅     |
| 4   | Retype 3 DTO fields in `library.rs`                                    | ✅     |
| 5   | `routes/library/mod.rs` decode sites (incl. runtime QueryBuilder)      | ✅     |
| 6   | `routes/series/mod.rs` column override                                 | ✅     |
| 7   | Orchestrator map (`Clean`→`"clean"`) + assert                          | ✅     |
| 8   | 17 SQL seed literals + 2 comments + tightened list-path tests          | ✅     |
| 9   | Regenerate `.sqlx` cache                                               | ✅     |
| 10  | Frontend `books.ts` z.enum ×3 + type + 8 test fixtures                 | ✅     |
| 11  | `docs/schema.md`, `RELEASE_DOCS_BACKLOG.md`, debt lift, README index   | ✅     |
| 12  | Full validation                                                        | ✅     |

---

## Validation Results

| Check                                                         | Result | Details                                                                            |
| ------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------- |
| `cargo fmt --check`                                           | ✅     | clean                                                                              |
| `cargo clippy --workspace --all-targets --locked -D warnings` | ✅     | 0 warnings                                                                         |
| Backend tests                                                 | ✅     | 659 passed, 0 failed, 1 ignored (+2 integration)                                   |
| `cargo sqlx prepare --check -- --tests`                       | ✅     | cache consistent                                                                   |
| Doc-lint (broken intra-doc links)                             | ✅     | 0 broken (2 pre-existing private-link warnings in `enrichment/http.rs`, unrelated) |
| Frontend lint                                                 | ✅     | 0 warnings                                                                         |
| Frontend `tsc -b`                                             | ✅     | clean                                                                              |
| Frontend tests                                                | ✅     | 254 passed                                                                         |
| DB enum (`\dT+`)                                              | ✅     | `{pending, clean, repaired, degraded}`                                             |

### Acceptance gates

- `rg "validation_status: String" backend/src` → none ✅
- `rg "validation_status::text" backend/src` → none ✅
- `rg "'valid'::validation_status" backend` → none ✅
- Drift probe present + passing ✅
- List-path runtime decode value-asserted (`== "clean"`) ✅

---

## Security review

Touches a DB-decode boundary. Change strictly tightens it: an unknown DB
variant now fails `sqlx` decode loudly instead of flowing to the wire as
an opaque string; the frontend union narrows from `z.string()` to a
closed `z.enum`, so an unaccounted-for backend value surfaces as a
`ZodError` at the boundary. No new user-input surface, no auth/secret/IO
change. Stands up to security review.

---

## Files Changed

29 source/doc files + `.sqlx` cache (14 query files rehashed, net-zero
count). See `git diff --stat`. New files: migration pair,
`models/validation_status.rs`, `docs/RELEASE_DOCS_BACKLOG.md`, ADR
(`adr/2026-05-28-validation-status-vocabulary.md`), allowlist entry.

---

## Next Steps

- [ ] Review implementation
- [ ] Create PR (user merges — never agent)
