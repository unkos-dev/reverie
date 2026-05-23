# Implementation Report — CSRF Synchronizer Token, Phase 1

**Plan**: `/home/coder/reverie/.claude/PRPs/plans/library-ui.plan.md` (Task 1c, sub-phase 11a-A.2)
**Linear umbrella**: [UNK-80](https://linear.app/unkos/issue/UNK-80) (no dedicated sub-issue filed — plan defers lazy sub-issue filing)
**Branch**: `feat/unk-80-library-ui-11a-A.2`
**Date**: 2026-05-23
**Status**: COMPLETE (Phase 1 only — middleware enable + `apiFetch` injection deferred to Phase 2)

---

## Summary

Phase 1 of the OWASP synchronizer-token CSRF defense:

- Backend mints a 32-byte CSPRNG token, base64url-unpadded, on successful
  OIDC `/auth/callback`. Stored in the session under key `csrf_token`.
- Backend exposes the token on `GET /auth/me` (existing endpoint), gaining
  a new `csrf_token: string | null` field. Basic-auth sessions (OPDS)
  see `null`; OIDC sessions see the 43-char token.
- Frontend ships a module-level reader at `src/api/csrf.ts` with
  `getCsrfToken()` + `refreshCsrfToken()` + zod-validated `/auth/me` parse.
  Module-level cache (single source of truth for the SPA); failures
  clear the cache to `null` rather than throw.

**NOT in scope (Phase 2)**:

- Backend `csrf_required` tower middleware (`security/csrf.rs`).
- Frontend `apiFetch` wrapper that injects `X-CSRF-Token` on mutating verbs
  - retries once on `403 csrf-mismatch`.
- Token rotation on role-change (`session_version` bump).

Order-of-operations split is codified in `adr/2026-05-22-json-api-conventions.md`
§"CSRF defense" — token issuance + reader must ship first so the
existing cookie-authed mutating endpoints (`POST /api/enrichment/*`,
`POST /api/tokens`) keep working between merges.

---

## Assessment vs Reality

| Metric     | Predicted        | Actual           | Reasoning                                                                                                                                                                              |
| ---------- | ---------------- | ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Complexity | HIGH (plan-wide) | LOW (this slice) | Phase 1 is the smallest possible slice that still ships something testable end-to-end. Three files changed (one backend, two frontend) plus 92 net LOC backend / 252 net LOC frontend. |
| Confidence | HIGH             | HIGH             | Mirror pattern (`auth::token::generate_device_token`) exact; advisor flagged Basic-auth contract assertion before commit, added.                                                       |

**Deviations from plan**: none load-bearing. Token-gen code stayed inline
in `routes/auth.rs::callback` instead of new `backend/src/security/csrf.rs`
(plan's CREATE target). Rationale: `security/csrf.rs` is the middleware
home (Phase 2). Creating a half-empty module now means a Phase 2 PR that
both creates the module and adds the middleware, vs. one PR with all
middleware logic. Advisor concurred.

---

## Tasks Completed

| #   | Task                                                                                    | File                                   | Status |
| --- | --------------------------------------------------------------------------------------- | -------------------------------------- | ------ |
| 1   | Branch + sync                                                                           | git                                    | ✅     |
| 2   | RED: extend `callback_succeeds_first_user_promoted_to_admin` for csrf_token + stability | `backend/src/routes/auth.rs`           | ✅     |
| 3   | GREEN: inline token gen + session insert + `/auth/me` field                             | `backend/src/routes/auth.rs`           | ✅     |
| 4   | Lock Basic-auth contract: assert `csrf_token: null` for Basic sessions                  | `backend/src/routes/auth.rs`           | ✅     |
| 5   | Frontend RED+GREEN: `csrf.ts` reader + 12 vitest cases                                  | `frontend/src/api/{csrf,csrf.test}.ts` | ✅     |
| 6   | Validation matrix                                                                       | (commands)                             | ✅     |

---

## Validation Results

| Check                         | Result | Details                                 |
| ----------------------------- | ------ | --------------------------------------- |
| `cargo fmt --check`           | ✅     | Clean                                   |
| `cargo clippy -- -D warnings` | ✅     | Zero warnings                           |
| `cargo sqlx prepare --check`  | ✅     | Cache in sync                           |
| `cargo test` (backend)        | ✅     | 497 unit + 2 integration = 499 pass     |
| `npx tsc -b` (frontend)       | ✅     | Clean                                   |
| `npm run lint` (frontend)     | ✅     | 0 errors, 0 warnings (--max-warnings 0) |
| `npm test` (frontend)         | ✅     | 114 pass across 10 files (12 new)       |
| `npm run build` (frontend)    | ✅     | 323 kB JS, 84 kB CSS                    |

---

## Files Changed

| File                            | Action | Lines    |
| ------------------------------- | ------ | -------- |
| `backend/src/routes/auth.rs`    | UPDATE | +92 / -2 |
| `frontend/src/api/csrf.ts`      | CREATE | +132     |
| `frontend/src/api/csrf.test.ts` | CREATE | +166     |

---

## Tests Written

### Backend — extends `callback_succeeds_first_user_promoted_to_admin`

- Asserts `me_body["csrf_token"]` is a 43-char base64url-unpadded string.
- Asserts `me_body["csrf_token"]` is stable across two consecutive
  `/auth/me` reads in the same session (no per-request rotation).

### Backend — extends `me_returns_theme_preference_default`

- Asserts `csrf_token: null` for Basic-auth sessions (OPDS contract lock).

### Frontend — `src/api/csrf.test.ts` (12 cases)

| #   | Case                                                        |
| --- | ----------------------------------------------------------- |
| 1   | `getCsrfToken()` returns null before refresh                |
| 2   | `refreshCsrfToken()` hydrates cache from 200                |
| 3   | Does NOT send AbortSignal when none passed                  |
| 4   | Forwards AbortSignal when provided                          |
| 5   | Clears cache to null on 401                                 |
| 6   | Clears cache to null on 5xx                                 |
| 7   | Clears cache on malformed JSON body                         |
| 8   | Clears cache when `csrf_token` field omitted (schema drift) |
| 9   | Treats `csrf_token: null` (Basic-auth) as no token          |
| 10  | Rejects non-string `csrf_token` (schema drift)              |
| 11  | Clears cache when fetch throws (network error)              |
| 12  | Last successful refresh wins on consecutive calls           |

---

## Notes for Phase 2 reviewer

1. **Session-key collision**: Phase 1 stores the long-lived app CSRF
   token under session key `csrf_token` — same key `/auth/login` uses
   for the OIDC transient state. The OIDC value is removed in `callback`
   before the app token is inserted, so the live flow is correct. A
   logged-in user re-hitting `/auth/login` overwrites the app token
   with a new OIDC transient value; this self-heals once Phase 2's
   `403 csrf-mismatch` retry calls `refreshCsrfToken()`. Documented
   in the callback's THREAT comment. Not lifted to `debt/` because
   the self-healing path is built into the Phase 2 design.

2. **Reader has no runtime caller in Phase 1** — by design.
   `apiFetch` (Task 11) will call `refreshCsrfToken()` once on app boot
   and again on `403 csrf-mismatch`. Tests exercise the module in
   isolation.

3. **Token rotation**: deliberately omitted from Phase 1. Plan calls
   for rotation on `session_version` bump (role change); that
   handling lives with the middleware in Phase 2.

---

## Artifacts

- Report: `.claude/PRPs/reports/csrf-phase1-report.md` (this file)
- Plan: `.claude/PRPs/plans/library-ui.plan.md` — NOT archived (~60
  tasks across 6 sub-phases remain).
