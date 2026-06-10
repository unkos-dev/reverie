# Implementation Report

**Plan**: `.claude/PRPs/plans/docs-as-done-phase-2.plan.md` (PR1 slice only)
**Source Issue**: UNK-376 (epic; this PR is `Part of`, does not close)
**Branch**: `feat/unk-376-api-v1-mount-move`
**Commit**: `032fc1c`
**Date**: 2026-06-10
**Status**: COMPLETE (PR1 of ~6; epic ongoing)

---

## Summary

Executed PR1 of the UNK-376 epic: the `/api/*` → `/api/v1/*` mount move mandated by
`adr/2026-06-08-api-versioning-openapi.md`. 24 JSON data routes moved to `/api/v1`;
`/health`, `/auth`, `/opds` stay unversioned. Frontend call sites, query keys, and
docstrings followed. A gone-endpoint regression test proves the move is a move (old
path → JSON Problem 404, not SPA HTML). OpenAPI spec byte-identical (no utoipa work in
PR1). PR2..N (coverage + security model) and PR-final (grep-guard) remain.

---

## Assessment vs Reality

| Metric     | Predicted        | Actual            | Reasoning                                                                                          |
| ---------- | ---------------- | ----------------- | ------------------------------------------------------------------------------------------------- |
| Complexity | MEDIUM (wide)    | MEDIUM            | Mechanical but caught one real trap (external-API path collision) the scripted sweep would've shipped |
| Confidence | 9/10 one-pass    | Held              | No rework on approach; the one bug was caught at diff-review, pre-validation                       |

**Deviations from the plan:**

1. **External-API path collision (caught + reverted).** The plan's Task 1-3 greps
   targeted `backend/src/routes`, but a repo-wide `perl` sweep (chosen for determinism)
   also rewrote `services/enrichment/{orchestrator.rs, sources/open_library.rs}`, where
   `/api/books` is the **external Open Library** endpoint (live outbound URL + wiremock
   mocks), not our route. Detected in the diff-stat review (the two files were the only
   `services/` hits), confirmed every change was third-party, and `git checkout`-reverted
   both wholesale. The plan's narrower per-directory greps would have avoided the
   collision but also wouldn't have covered docstrings repo-wide — net, the sweep +
   review was the right call, with the revert as the safety net.

2. **Plan NOT archived to `completed/`.** Skill Phase 5 says archive, but this plan
   covers a 6-PR epic and only PR1 landed. Archiving would misrepresent the epic as done.
   Plan stays in `.claude/PRPs/plans/` (committed) until PR-final.

3. **Tasks 4 vs 5 split.** Task 5 (reserved-prefix `/api/v1` coverage) needed no new
   code — the `/api/` → `/api/v1/` sweep updated the existing `is_reserved_prefix` unit
   test and the lib.rs `unmatched_api_route_returns_problem_with_instance` integration
   test to cover `/api/v1/*` automatically. Only Task 4 (gone-endpoint, the genuinely-new
   assertion about the *old* path) needed a written test.

---

## Tasks Completed

| #   | Task                                          | Status |
| --- | --------------------------------------------- | ------ |
| 1   | Backend route strings → `/api/v1`             | ✅ (scripted sweep) |
| 2   | 5 `cover_url` `format!` sites → `/api/v1`     | ✅     |
| 3   | Backend test URLs → `/api/v1`                 | ✅     |
| 4   | Gone-endpoint regression test (new)           | ✅ `routes/library/tests.rs` |
| 5   | Reserved-prefix `/api/v1` coverage            | ✅ (existing tests now cover it) |
| 6   | Frontend `src/api/*` URLs → `/api/v1`         | ✅ (`@/api` aliases guarded) |
| 7   | Frontend test URL assertions → `/api/v1`      | ✅     |
| 8   | Docstrings/comments + `docs/` prose           | ✅ (Sentry/Open-Library/`/api/*` superset left) |
| 9   | Full backend validation                       | ✅ (see below) |
| 10  | Repo-lint + commit                            | ✅ `032fc1c` |

---

## Validation Results

| Check        | Result | Details                                                                                  |
| ------------ | ------ | ---------------------------------------------------------------------------------------- |
| fmt          | ✅     | `cargo fmt --all -- --check` exit 0 (longer strings reflowed)                             |
| clippy       | ✅     | `--workspace --all-targets --locked -D warnings` clean (compiles new test offline)       |
| gen_openapi  | ✅     | 2 drift tests pass; `docs/openapi.json` byte-identical (no regen) — spec untouched        |
| Frontend lint| ✅     | `eslint . --max-warnings 0` clean                                                        |
| Frontend test| ✅     | vitest 300/300 pass (34 files)                                                            |
| Backend test (integration) | ⏭️ CI-gated | `#[sqlx::test]` needs a live provisioning DB; localhost:5433 unreachable in this Coder workspace (DooD). Per `project_backend_local_validation_gap`, integration tests are CI's gate. New test compiles (clippy --all-targets). |
| Repo-lint    | ✅     | typos, markdownlint, gitleaks (lint-staged) clean                                         |

---

## Files Changed

63 files, +1004/-493 (includes plan + report). Code: 12 backend route modules + cover
handler, 5 cover_url sites, ~12 backend test files, 1 new test, 16 frontend `api/*` +
9 frontend pages/components/keys, `docs/RELEASE_DOCS_BACKLOG.md`. Untouched (verified):
`RESERVED_PREFIXES` const, `openapi.rs:26`, external Open Library + Sentry paths, vite
proxy, `@/api` import aliases, `/auth`/`/health`/`/opds`.

---

## Security (hard-rule-6)

Touched `security/headers.rs` (tests only). The fallback-routing surface is correct:
old `/api/*` paths hit `is_reserved_prefix` (prefix-match, unchanged) → JSON Problem
404, never SPA HTML — so stale API clients get a machine-readable error, no HTML-200
leak. No auth/CSP/session behaviour changed. **Will this stand up to security review?**
Yes — the change is a path rename; the only security-relevant surface (reserved-prefix
fallback) is unchanged in behaviour and now locked by the gone-endpoint test.

---

## Tests Written

| Test File                        | Test Case                                          |
| -------------------------------- | -------------------------------------------------- |
| `routes/library/tests.rs`        | `api_v1_move_old_path_returns_problem_not_spa` — old `/api/books`→Problem 404, new `/api/v1/books`→200 |

---

## Next Steps

- [ ] Push branch + open PR (`Part of UNK-376`, NOT `Closes`)
- [ ] CI runs the integration tests (local DB unreachable)
- [ ] User reviews + merges
- [ ] PR2: `prp-plan` the first coverage batch (securitySchemes + `library`/`series`/`dashboard`)
