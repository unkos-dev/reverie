# Implementation Report — OPDS 1.2 Catalog (BLUEPRINT Step 9)

**Plan**: `.claude/PRPs/plans/opds-catalog.plan.md`
**Branch**: `feat/opds-catalog`
**Date**: 2026-04-21
**Status**: COMPLETE — fmt ✅ clippy ✅ 369/369 tests ✅

---

## Summary

Mounted `/opds/*` Atom XML feeds + EPUB downloads + cover image service behind a
Basic-only extractor so OPDS reader apps (KOReader, Moon+, Librera, KyBook 3)
can pair against Reverie with a device token. Scope is URL-based:
`/opds/library/*` exposes the whole library (filtered by child RLS underneath);
`/opds/shelves/{id}/*` exposes one shelf. Cover handlers are dual-mounted:
`/opds/books/:id/cover{,/thumb}` under `BasicOnly` (for OPDS credentials to
stay within the paired RFC 7617 protection space) and
`/api/books/:id/cover{,/thumb}` under `CurrentUser` for the Step 10 web UI.

---

## Assessment vs Reality

| Metric     | Predicted                                | Actual                                | Reasoning                                                                                            |
| ---------- | ---------------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Complexity | Large (~30 files, ~2500 LOC)             | 30 new/updated files, ~2400 LOC       | Matched.                                                                                             |
| Confidence | Strong — plan prescribed file-by-file    | High — implementation matched spec    | Advisor caught three plan gaps up front: `.env.example` path, `manifestation_tags` name, Atom namespace default — corrected before coding. |

**Deviations from the plan:**

- **Atom namespace**: plan narrates `<atom:author>`, `<atom:summary>`, etc. but only
  declares `xmlns`, `xmlns:opds`, `xmlns:dc`, `xmlns:opensearch` (no `xmlns:atom`).
  Implementation uses the default Atom namespace (bare `<author>`, `<summary>`,
  `<title>`, etc.) per OPDS 1.2 convention. Plan text treated as informal.
- **`.env.example` location**: plan says `backend/.env.example`; file actually
  lives at repo root `.env.example`. Appended there.
- **`manifestation_tags` vs `metadata_tags`**: plan text says `metadata_tags` in
  a few spots; migration creates `tags` + `manifestation_tags`. Implementation
  uses the actual names.
- **`push_visible_manifestation` unused**: plan's Task 9 prescribes this helper
  for navigation feeds, but the actual `emit_authors`/`emit_series` handlers
  inline the `EXISTS` predicate directly — simpler and equally correct. Helper
  retained with `#[allow(dead_code)]` for potential future callers; unit test
  for it also kept.
- **Drive-by fix**: `services/metadata/draft.rs` had a pre-existing
  `explicit_auto_deref` clippy error under the current stable toolchain
  (unrelated to this PR). Fixed in-line so `cargo clippy -D warnings` gates
  the new code.

---

## Tasks Completed

All 26 tasks from plan:

| Phase | Tasks          | Status |
| ----- | -------------- | ------ |
| A     | Config + error | ✅     |
| B     | Auth + debounce | ✅    |
| C     | XML primitives | ✅     |
| D     | Feed handlers  | ✅     |
| E     | OpenSearch     | ✅     |
| F     | Download       | ✅     |
| G     | Covers         | ✅     |
| H     | Wiring         | ✅     |
| I     | Tests          | ✅     |

---

## Files Changed

**New (22)**:

| File                                         | Lines |
| -------------------------------------------- | ----- |
| `backend/src/auth/basic_only.rs`             | ~40   |
| `backend/src/routes/opds/mod.rs`             | ~55   |
| `backend/src/routes/opds/xml.rs`             | ~55   |
| `backend/src/routes/opds/cursor.rs`          | ~115  |
| `backend/src/routes/opds/scope.rs`           | ~110  |
| `backend/src/routes/opds/feed.rs`            | ~490  |
| `backend/src/routes/opds/root.rs`            | ~80   |
| `backend/src/routes/opds/library.rs`         | ~535  |
| `backend/src/routes/opds/shelves.rs`         | ~200  |
| `backend/src/routes/opds/opensearch.rs`      | ~120  |
| `backend/src/routes/opds/download.rs`        | ~200  |
| `backend/src/routes/opds/covers.rs`          | ~100  |
| `backend/src/routes/opds/tests.rs`           | ~600  |
| `backend/src/services/covers/mod.rs`         | ~95   |
| `backend/src/services/covers/error.rs`       | ~25   |
| `backend/src/services/covers/extract.rs`     | ~50   |
| `backend/src/services/covers/resize.rs`      | ~125  |
| `backend/src/services/covers/cache.rs`       | ~60   |

**Updated (14)**:

- `backend/src/config.rs` — `OpdsConfig`, env parse + fail-fast
- `backend/src/error.rs` — `AppError::BasicAuthRequired { realm }` + RFC 7617 response
- `backend/src/auth/mod.rs`, `auth/middleware.rs` — `verify_basic` helper shared between `CurrentUser` and `BasicOnly`
- `backend/src/models/device_token.rs` — SQL-side 5-minute debounce + tests
- `backend/src/routes/mod.rs`, `main.rs` — mount OPDS + always-mount covers-api
- `backend/src/services/mod.rs`, `epub/cover_layer.rs` — promote `find_cover_href` to pub(crate)
- `backend/src/test_support.rs` — `server_with_opds_enabled`, child/adult auth helpers, tagged EPUB fixture
- `backend/.env.example` (root) — `REVERIE_OPDS_*` + `REVERIE_PUBLIC_URL`
- `backend/Cargo.toml` — `url`, `percent-encoding` direct deps
- `backend/services/{enrichment,ingestion,writeback}` test configs — add `opds: OpdsConfig{...}` field
- `backend/services/metadata/draft.rs` — fix pre-existing clippy `explicit_auto_deref`

---

## Validation Results

| Check          | Result                                    |
| -------------- | ----------------------------------------- |
| `cargo fmt --check`                         | ✅ clean       |
| `cargo clippy --all-targets -- -D warnings` | ✅ zero warnings |
| **Full `cargo test` suite**                 | ✅ **369 passed, 0 failed, 55.93 s** |
| OPDS unit + integration tests               | ✅ 45 cases pass |
| `services::covers::*` tests                 | ✅ 5 passed   |
| `models::device_token::*` tests             | ✅ 4 passed   |
| `cargo build --release`                     | ✅ compiled in 4m 04s |

### Full-suite journey (fixed in-flight)

First full-suite run on this branch: 279 passed / 89 failed at default
parallelism. Baseline: `main` passes 315/315 in 49.76 s. Initial hypothesis:
DB connection pressure from the new `#[sqlx::test]` cases. Advisor pushed
back — that was speculation, not attribution. Investigation revealed the
real cause:

- `OpdsConfig::from_env()` defaults to `enabled=true` (per plan Task 1),
  which fail-fasts when `REVERIE_PUBLIC_URL` is unset.
- 9 pre-existing `config::tests::*` tests don't set `REVERIE_PUBLIC_URL`,
  so `Config::from_env().unwrap()` began panicking on my branch.
- The panic happens inside the `ENV_LOCK` (crate-wide `Mutex` guarding
  process-global env access), poisoning the mutex.
- ~80 DB-backed tests also acquire `ENV_LOCK` (to read `DATABASE_URL`
  safely), and hit `PoisonError` on their `lock().unwrap()`, cascading
  into the 89-failure count.

Fix: commit `b899fba` — add `("REVERIE_OPDS_ENABLED", "false")` to the
`vars` list of each config test. The tests don't care about OPDS; they're
validating unrelated env-parse paths. Post-fix: 369/0/0, 55.93 s.

---

## Tests Written

| File                                                | Test Cases                                                                                                                                                                          |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `routes/opds/xml.rs` (inline)                       | strips control codepoints; preserves tab/LF/CR; preserves emoji + high codepoints; does not escape 5-entity chars; strips FFFE/FFFF                                                 |
| `routes/opds/cursor.rs` (inline)                    | encode/parse round-trip; reject invalid base64; reject missing delimiter; reject bad timestamp; reject bad UUID                                                                      |
| `routes/opds/scope.rs` (inline)                     | library scope pushes nothing; shelf scope pushes 1 bind; visible_manifestation library; visible_manifestation shelf                                                                 |
| `routes/opds/feed.rs` (inline)                      | 14 cases: namespaces; entry/feed URN contract; acquisition rel links; control-char stripping in text AND attributes; ampersand/angle-bracket escape; emoji preserved; absolute URLs; ISBN-preferred/UUID-fallback; navigation profile type |
| `routes/opds/download.rs` (inline)                  | ASCII fallback strip; empty→UUID fallback; RFC 5987 percent-encode; content-disposition format                                                                                       |
| `services/covers/resize.rs` (inline)                | resizes to thumb cap; resizes to full cap; format preserved (PNG); skip resize under cap; reject unsupported format                                                                  |
| `models/device_token.rs` (new `#[sqlx::test]`)      | `list_for_user_excludes_revoked`; `update_last_used_debounced_within_window`                                                                                                        |
| `routes/opds/tests.rs` (13 `#[sqlx::test]`)         | Tests 20–32 from plan: root feed; 401 challenge; revoked token; opensearch descriptor; search round-trip; child RLS; adult shelf scope; cross-user 404; pagination walk 125; XML robustness; XSS-safe search reflection; download streams + path traversal 403; cover cache populates |

**53 new test cases total.**

---

## Acceptance Criteria

All 13 from plan's "Acceptance Criteria" section verifiable via the
integration tests plus the manual smoke plan (Level 4 — deferred to post-
merge against a live instance per plan's own Level 4 note).

---

## Next Steps

- [ ] Review this report
- [ ] Open PR: `feat/opds-catalog` → `main`
- [ ] Pair KOReader + one of {Moon+, Librera, KyBook 3} against a running
      instance (Level 4 manual smoke per plan)
