# Implementation Report

**Plan**: `.claude/PRPs/plans/completed/openapi-coverage-series-dashboard.plan.md`
**Linear**: UNK-376 (v0.1.0 milestone) — `Part of`, NOT `Closes` (epic close policy)
**Branch**: `feat/openapi-coverage-series-dashboard`
**Date**: 2026-06-10
**Status**: COMPLETE

---

## Summary

Migrated the `series` (1 route) and `dashboard` (2 routes) modules onto the
docs-as-done OpenAPI pattern blessed by the `library` cluster (#449). Both
`router()`s now return `utoipa_axum::OpenApiRouter<AppState>` built via
`routes!`; the three handlers carry `#[utoipa::path]`; the response DTOs derive
`ToSchema` and `ActivityParams` derives `IntoParams`. Both routers are merged
once into `openapi::pilot_router()` and their duplicate `lib.rs` mounts removed.
`docs/openapi.json` was regenerated (+429 lines, 3 new paths) and committed in
lockstep. The admin gate on `dashboard` is documented as a `403` response (no
new security scheme — deferred to UNK-382).

---

## Assessment vs Reality

| Metric     | Predicted    | Actual      | Reasoning                                                                 |
| ---------- | ------------ | ----------- | ------------------------------------------------------------------------- |
| Complexity | LOW–MEDIUM   | LOW         | Pure mechanical mirror of #449; the admin-403 wrinkle needed no new code  |
| Confidence | High         | Confirmed   | All `ToSchema`/`IntoParams` derives compiled first try; no fallbacks hit  |

Predicted traps that did **not** bite (confirmed):

- `&'static str` `ToSchema` on `StatusCount.status` — compiled clean, no
  `#[schema(value_type = String)]` fallback needed.
- `IntoParams` `?limit` rendering as `in: path` (the #449 trap) — `#[into_params(parameter_in = Query)]`
  applied; verified in the artifact: `limit` is `"in": "query"`, `"required": false`.
- Dangling `$ref` / embedded-enum `ToSchema` — N/A this cluster (dashboard
  flattens enums to `&str` server-side; series's only embedded DTO already had
  `ToSchema` from #449).

---

## Tasks Completed

| #   | Task                                                            | File                                  | Status |
| --- | --------------------------------------------------------------- | ------------------------------------- | ------ |
| 1   | TDD: `spec_covers_series_dashboard_routes` (red→green)          | `backend/tests/gen_openapi.rs`        | ✅     |
| 2   | `ToSchema` on `SeriesDetail`, `SeriesWork`                      | `backend/src/models/series.rs`        | ✅     |
| 3   | `ToSchema` on 6 dashboard DTOs                                  | `backend/src/routes/dashboard/mod.rs` | ✅     |
| 4   | `IntoParams` + `parameter_in = Query` on `ActivityParams`       | `backend/src/routes/dashboard/mod.rs` | ✅     |
| 5   | `#[utoipa::path]` on `detail`, `stats`, `activity`              | series + dashboard mod.rs             | ✅     |
| 6   | Convert both `router()`s to `OpenApiRouter`                     | series + dashboard mod.rs             | ✅     |
| 7   | Merge into `pilot_router()` + tags; remove `lib.rs` mounts      | `openapi.rs`, `lib.rs`                | ✅     |
| 8   | Verify no double-serve / `--tests` compiles                    | (validation)                          | ✅     |
| 9   | Regenerate + commit `docs/openapi.json`; full local gate       | `docs/openapi.json`                   | ✅     |

---

## Validation Results

| Check                          | Result | Details                                                         |
| ------------------------------ | ------ | -------------------------------------------------------------- |
| `cargo fmt --all -- --check`   | ✅     | One self-introduced diff fixed                                  |
| `cargo clippy --all-targets`   | ✅     | 0 warnings (`-D warnings`)                                      |
| `cargo check --tests`          | ✅     | clean                                                           |
| `gen_openapi` drift (no REGEN) | ✅     | 5/5 pass incl. new `spec_covers_series_dashboard_routes`        |
| Docs build (`npm run build`)   | ✅     | 21 pages; 3 new op pages + 2 new tag-overview pages (series/dashboard) |
| lychee link check              | ✅     | 631 OK, 0 errors, 60 excluded — every `$ref` resolves          |
| `typos`                        | ✅     | clean (via lint-staged)                                         |
| Integration (`#[sqlx::test]`)  | ⏭️     | CI-authoritative (provisioning DB unreachable locally)         |

Artifact spot-checks (advisor-prompted): `activity.parameters[0]` = `{in:query, required:false}`;
the three 200 bodies `$ref` `SeriesDetail` / `StatsResponse` / `ActivityResponse`.

---

## Deviations from Plan

None. Implemented exactly as specified.

---

## Issues Encountered

- **Disk pressure (ENOSPC).** The 96G workspace was ~99% full (main `target/debug`
  = 31G warm cache). A worktree-local second `target/` could not fit and a partial
  write corrupted `lib.rs` mid-edit (restored from HEAD). Resolution: pointed all
  worktree cargo builds at the **main** `target/` via `CARGO_TARGET_DIR` (dependency
  artifacts content-hashed and reused; only the small reverie crates added), and
  reclaimed re-downloadable caches (npm/uv/pip + disconnected sdl-mcp). No source
  or user data touched.
- **`lint-staged` / docs `node_modules` absent in the fresh worktree.** Symlinked
  the main checkout's `node_modules` (and `docs/node_modules`) into the worktree —
  zero disk cost — so the pre-commit hook and docs build resolve their binaries.
- **Commit message corruption.** The first commit body had a line starting with
  `#[utoipa::path]`, which git's default `--cleanup=strip` deleted as a comment.
  Caught by advisor; re-committed via a message file with `--cleanup=verbatim`.

---

## Tests Written

| Test File                      | Test Case                              |
| ------------------------------ | -------------------------------------- |
| `backend/tests/gen_openapi.rs` | `spec_covers_series_dashboard_routes`  |

Asserts: 3 paths present; all three inherit document security (no op-level key);
all 8 series+dashboard DTO schemas registered (incl. nested-only
`StatusCount`/`MetadataCoverage`); `series/{id}` declares `404`; both dashboard
ops declare `403` (the admin-gate signal).

---

## Security (hard rule 6)

**Does it stand up to security review? Yes.** Annotations are doc-only; no runtime
surface changes. All three ops inherit the deny-by-default `session_cookie`
requirement (pinned by the new no-op-level-security assertion) — the spec never
misdocuments an authed route as public. The admin gate is documented (`403` +
"Admin only"), not modelled away; runtime `require_admin()` is unchanged. No info
leak: documented bodies are aggregate counts (no per-user rows/paths/PII).
First-class admin representation (OAuth2/OIDC scope) deferred to **UNK-382**.

---

## Next Steps

- [ ] Push branch (with `CARGO_TARGET_DIR` exported so the pre-push clippy reuses
      the shared target — otherwise ENOSPC)
- [ ] Open PR — title without `unk-376`; body `Part of UNK-376` (NOT `Closes`)
- [ ] CI runs the `#[sqlx::test]` integration suite (the router-move regression net)
- [ ] User reviews + merges
