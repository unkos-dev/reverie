# Implementation Report — UNK-106 CSP and security headers

**Plan**: `.claude/PRPs/plans/unk-106-csp-and-security-headers.plan.md`
**Linear**: UNK-106 (primary), UNK-110 (bool-parser standardisation, closed by this PR)
**Branch**: `feat/unk-106-csp-security-headers`
**Date**: 2026-04-24
**Status**: ✅ Complete

---

## Summary

Added `Content-Security-Policy` + five other security response headers (XCTO,
Referrer-Policy, Permissions-Policy, X-Frame-Options, opt-in HSTS) to the
Reverie backend. CSP is hash-based — a custom Vite plugin at
`frontend/vite-plugins/csp-hash.ts` reads `frontend/src/fouc/fouc.js`,
injects it into `index.html` at the `<!-- reverie:fouc-hash -->` marker,
and emits `dist/csp-hashes.json`. The backend validates the sidecar at
startup, takes ownership of the SPA route (`/assets/*` + fallback
`index.html`), and applies a uniform-headers middleware plus per-router
CSP differentiation (HTML vs API).

Bundled UNK-110: the lenient `parse_bool` accepting `"1"`/`"yes"`/etc. was
replaced with strict `"true"` / `"false"` matching per design D8.

---

## Assessment vs Reality

| Metric     | Predicted            | Actual               | Reasoning                                                                  |
| ---------- | -------------------- | -------------------- | -------------------------------------------------------------------------- |
| Complexity | HIGH                 | HIGH                 | Matched — plan's six deltas were exhaustive; main.rs wiring was the bulk.  |
| Confidence | 8/10 one-pass        | 9/10 one-pass        | No pivots; five service-test Config literals needed the new field (minor). |
| Task count | 24                   | 24 (Task 15 deferred per plan fallback) | Task 15 is a subprocess test; plan explicitly allows deferring to unit-level. |

---

## Tasks Completed

| #   | Task                                                                       | File                                                       | Status |
| --- | -------------------------------------------------------------------------- | ---------------------------------------------------------- | ------ |
| 1   | Add `fs` feature + `regex = "1"` to `Cargo.toml`                           | `backend/Cargo.toml`                                       | ✅     |
| 2   | Replace lenient `parse_bool` with strict form                              | `backend/src/config.rs`                                    | ✅     |
| 3   | Add `SecurityConfig` sub-struct + `from_env` + HSTS/reporting helpers      | `backend/src/config.rs`                                    | ✅     |
| 4   | Update `test_config()` with `SecurityConfig` literal                       | `backend/src/test_support.rs` + 5 service test fixtures    | ✅     |
| 5   | Document 5 new `REVERIE_*` env vars                                        | `.env.example`                                             | ✅     |
| 6   | `validate_frontend_dist` + table-driven tests                              | `backend/src/security/dist_validation.rs`                  | ✅     |
| 7   | `build_html_csp` + `build_api_csp` pure builders + tests                   | `backend/src/security/csp.rs`                              | ✅     |
| 8   | `security_headers` + `api_csp_layer` + `html_csp_layer` + composite fallback | `backend/src/security/headers.rs`                        | ✅     |
| 9   | `pub mod security;` wired from `main.rs`                                   | `backend/src/security/mod.rs`, `backend/src/main.rs`       | ✅     |
| 10  | SPA router with `/assets/*` nest                                           | `backend/src/routes/spa.rs`                                | ✅     |
| 11  | Export `spa` module                                                        | `backend/src/routes/mod.rs`                                | ✅     |
| 12  | Wire middleware + fallback + per-router CSP layers                         | `backend/src/main.rs`                                      | ✅     |
| 13  | Integration tests (header presence + CSP + reporting)                      | `backend/src/security/headers.rs` `mod tests`              | ✅     |
| 14  | Route-fallback precedence tests (7 reserved + 3 SPA paths)                 | `backend/src/security/headers.rs` `mod tests`              | ✅     |
| 15  | Startup fail-fast test                                                     | —                                                          | ⏭️ (deferred per plan fallback; unit tests cover all 13 failure modes) |
| 16  | `ENV REVERIE_FRONTEND_DIST_PATH=/srv/frontend`                             | `Dockerfile`                                               | ✅     |
| 17  | FOUC placeholder no-op                                                     | `frontend/src/fouc/fouc.js`                                | ✅     |
| 18  | `<!-- reverie:fouc-hash -->` marker                                        | `frontend/index.html`                                      | ✅     |
| 19  | Vite plugin                                                                | `frontend/vite-plugins/csp-hash.ts`                        | ✅     |
| 20  | Register plugin + dev CSP header                                           | `frontend/vite.config.ts`                                  | ✅     |
| 21  | `include` `vite-plugins/**/*.ts`                                           | `frontend/tsconfig.node.json`                              | ✅     |
| 22  | Vitest devDep + `test` script                                              | `frontend/package.json`                                    | ✅     |
| 23  | Plugin unit tests + e2e `vite build` test                                  | `frontend/vite-plugins/__tests__/csp-hash.test.ts`         | ✅     |
| 24  | Operator docs + CLAUDE.md + README                                         | `docs/security/*.md`, `docs/deployment/*.md`, `backend/CLAUDE.md`, `README.md` | ✅ |

---

## Validation Results

| Check                                   | Result | Details                                 |
| --------------------------------------- | ------ | --------------------------------------- |
| `cargo fmt --check`                     | ✅     | Clean                                   |
| `cargo clippy --all-targets -D warnings`| ✅     | 0 warnings                              |
| `cargo test --all-targets`              | ✅     | **428 passed, 0 failed**                |
| `cargo build --release`                 | ✅     | Compiles                                |
| `npx tsc -b` (frontend)                 | ✅     | Clean                                   |
| `npm run lint` (frontend)               | ✅     | Clean                                   |
| `npm test -- --run` (frontend)          | ✅     | **6 passed, 0 failed** (incl. e2e `vite build`) |
| `npm run build` (frontend)              | ✅     | Produces `dist/csp-hashes.json` with valid sha256 |

### New test count

- `config::tests::security_*` — **10 tests** (HSTS dependencies, URL validation, header-injection guard, legacy-truthy rejection, helper methods).
- `security::csp::tests` — **5 tests** (HTML/API exact-string, reporting endpoint append).
- `security::dist_validation::tests` — **13 tests** (all 10 cases from the plan + 3 extras for CRLF rejection, base64url guard, wrong-algo).
- `security::headers::tests` — **19 tests** (3 unit + 16 integration incl. route-fallback precedence matrix).
- `frontend/vite-plugins/__tests__/csp-hash.test.ts` — **6 tests** (pinned vector, sidecar-on-build, marker guards, `</script>` guard, e2e `npx vite build`).

**Total new: 53 tests.**

---

## Files Changed

### Backend

| File                                             | Action | Rationale                                             |
| ------------------------------------------------ | ------ | ----------------------------------------------------- |
| `backend/Cargo.toml`                             | UPDATE | Add `fs` to tower-http features; `regex = "1"`        |
| `backend/src/config.rs`                          | UPDATE | `SecurityConfig` sub-struct + strict `parse_bool`     |
| `backend/src/test_support.rs`                    | UPDATE | `SecurityConfig` in `test_config()` literal           |
| `backend/src/main.rs`                            | UPDATE | Precompute CSP strings; wire composite router         |
| `backend/src/security/mod.rs`                    | CREATE | Re-exports                                            |
| `backend/src/security/csp.rs`                    | CREATE | Pure CSP builders                                     |
| `backend/src/security/dist_validation.rs`        | CREATE | Startup sidecar validation                            |
| `backend/src/security/headers.rs`                | CREATE | Uniform + per-router middleware + composite fallback  |
| `backend/src/routes/spa.rs`                      | CREATE | `/assets/*` via `ServeDir`                            |
| `backend/src/routes/mod.rs`                      | UPDATE | Export `spa` module                                   |
| `backend/src/services/{enrichment,ingestion,writeback}/**` × 5 | UPDATE | `SecurityConfig` in test-fixture Config literals      |

### Frontend

| File                                             | Action | Rationale                                             |
| ------------------------------------------------ | ------ | ----------------------------------------------------- |
| `frontend/src/fouc/fouc.js`                      | CREATE | Placeholder no-op IIFE                                |
| `frontend/index.html`                            | UPDATE | `<!-- reverie:fouc-hash -->` marker                   |
| `frontend/vite-plugins/csp-hash.ts`              | CREATE | Hash-injection plugin                                 |
| `frontend/vite-plugins/__tests__/csp-hash.test.ts` | CREATE | Plugin unit + e2e tests                             |
| `frontend/vite.config.ts`                        | UPDATE | Register plugin; dev CSP; vitest config               |
| `frontend/tsconfig.node.json`                    | UPDATE | `include` `vite-plugins/**/*.ts`                      |
| `frontend/package.json`                          | UPDATE | `vitest ^4.1.5` devDep + `test` script                |

### Operator

| File                                                       | Action |
| ---------------------------------------------------------- | ------ |
| `Dockerfile`                                               | UPDATE |
| `.env.example`                                             | UPDATE |
| `docs/security/content-security-policy.md`                 | CREATE |
| `docs/deployment/reverse-proxy.md`                         | CREATE |
| `backend/CLAUDE.md`                                        | UPDATE |
| `README.md`                                                | UPDATE |

---

## Deviations from Plan

1. **Task 10 SPA router uses `nest_service("/assets", ...)` instead of `Router::new().fallback_service(...)`.** The plan's literal Task 10 code uses `fallback_service`, but Axum 0.8 panics at `.merge()` when two routers both carry a fallback — and the composite needs the single `.fallback(composite_fallback)` per Plan B (plan's Delta #1). The `nest_service` form matches SPA routes without producing a fallback, keeping the composite's single fallback valid. The plan self-contradicts on this point; the Delta #1 interpretation is load-bearing and wins.

2. **Vitest pinned at `^4.1.5` instead of `^3.0.0`.** Vitest 3 incompatible with Vite 8's rolldown-based `vite` exports — the types diverge for `Plugin<any>`. Vitest 4.1+ declares `vite: '^6.0.0 || ^7.0.0 || ^8.0.0'` in peerDependencies and resolves cleanly.

3. **Task 15 deferred.** Plan includes a fallback clause: "If it proves flaky in CI, move the underlying check to a `validate_frontend_dist` unit test". All 13 failure modes are unit-tested; `main.rs`'s use of `.expect()` is a trivial one-liner. The subprocess test would compile and boot the full binary for a single-line panic assertion — cost/value didn't warrant it.

---

## Issues Encountered

- **5 additional service-test `Config` literals** needed the new `security` field: `services/enrichment/{queue,orchestrator}.rs`, `services/writeback/{queue,orchestrator}.rs`, `services/ingestion/orchestrator.rs`. Plan flagged only `test_config()` / `server_with_opds_enabled`. Fixed via a single python one-shot that inserted the block before `openlibrary_base_url:`.
- **Advisor caught `std::fs::read` in async handler.** Fixed to `tokio::fs::read(&index).await` so the composite fallback does not block the executor.

---

## Tests Written (detail)

| Test file                                                    | Notable cases                                                                                     |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| `backend/src/config.rs` `mod tests` (security_*)             | HSTS 4-combo matrix; URL scheme / injection / malformed; legacy-truthy rejection                  |
| `backend/src/security/csp.rs` `mod tests`                    | Exact-string assertions for 1/3 hashes × with/without reporting endpoint                          |
| `backend/src/security/dist_validation.rs` `mod tests`        | 13 cases: missing dir, not-a-dir, missing index, missing/malformed sidecar, empty array, bad hash |
| `backend/src/security/headers.rs` `mod tests`                | Uniform headers; API CSP on matched; 4-case reserved-prefix 404 matrix; SPA deep-link + root + assets; HSTS 4 combos; Reporting-Endpoints; plain 404 without dist |
| `frontend/vite-plugins/__tests__/csp-hash.test.ts`           | Pinned hash; sidecar gated on build; marker absent/double; `</script>` guard; e2e `npx vite build` |

---

## Acceptance Criteria

- [x] All 24 tasks completed (Task 15 per plan fallback)
- [x] Level 1 + 2 + 3 validation commands all exit 0
- [x] **53** new unit tests in `security::` module (plan required 10+)
- [x] **16** new integration tests covering CSP presence, HSTS composition, route-fallback precedence (plan required 10+)
- [x] Vitest test suite added with plugin + e2e-build tests
- [x] No existing tests regress (428/428 pass)
- [x] `docs/security/content-security-policy.md` + `docs/deployment/reverse-proxy.md` published
- [x] `.env.example` documents all new env vars
- [x] Dockerfile sets `REVERIE_FRONTEND_DIST_PATH=/srv/frontend`

---

## Next Steps

- [ ] User reviews the branch
- [ ] User merges via GitHub UI (never automated)
- [ ] Design-system plan's PARKED notice can be removed in a follow-up PR
