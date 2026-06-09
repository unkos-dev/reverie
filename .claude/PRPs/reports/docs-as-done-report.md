# Implementation Report — docs-as-done (Phase 1 mechanism slice)

**Plan**: `.claude/PRPs/plans/completed/docs-as-done.plan.md` (archived on close)
**Linear**: [UNK-370](https://linear.app/unkos/issue/UNK-370) (Phase 1, v0.1.0 milestone)
**Branch**: `feat/unk-370-docs-as-done`
**Date**: 2026-06-09
**Status**: COMPLETE (Phase 1 — pilot slice; full route coverage + `/api/v1` move deferred to Phase 2)

---

## Summary

Stands up the docs-as-done mechanism (CLAUDE.md hard rule 10): generated
reference documentation is now a build-gated part of every PR, mirroring how the
TDD mandate prevents untested code. Landed end-to-end on a thin vertical:

1. **Config reference** — `config::reference::render_markdown()` renders
   `configuration.mdx` from `schemars::schema_for!(Config)` joined with
   `ENV_MAP`. Drift-free by construction (same schema source as the CI-gated
   `config.schema.json`) and secret-safe by construction (schema renders secret
   defaults as `""`/`null`). Gate 3's required-var set extracted into a shared
   `REQUIRED_ENV_VARS` so the renderer and the startup check consume one source.
2. **OpenAPI 3.1 reference** — proven on the `health` pilot via `utoipa` +
   `utoipa-axum`. The pilot is wired through `OpenApiRouter<AppState>` +
   `routes!(...)`, so an un-annotated handler fails to compile (the coverage
   mechanism). `split_for_parts()` feeds the `Router` part back into `lib.rs`
   (runtime unchanged) and the `OpenApi` part to the spec writer. Committed at
   `docs/openapi.json`, rendered into the Starlight sidebar by
   `starlight-openapi`.
3. **CI gate** — new `docs` job in `ci.yml` (astro build + lychee), gated on a
   `changes.docs` path filter and appended to `ci-gate.needs` (skipped-job
   result handled). `docs.yml` dropped its `pull_request` trigger to dedupe.
   Two backend drift tests double as writers (`REGEN=1`) and gate staleness for
   free under the existing `backend` `cargo test` job — no DB required.

**NOT in scope (Phase 2 follow-up issue):**

- OpenAPI coverage of the other ~13 route modules (ratchets module-by-module).
- The `/api/*` → `/api/v1/*` mount move (ADR 2026-06-08-api-versioning-openapi).
- The raw-`.route(` grep-guard + allowlist (can't enable while 13 modules still
  use plain `.route(`; pilot coverage is enforced by `routes!` compile-checking).
- Annotating real response DTOs with `#[derive(ToSchema)]`.

---

## Assessment vs Reality

| Metric     | Predicted               | Actual | Reasoning                                                                                                                                  |
| ---------- | ----------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Complexity | MEDIUM (slice of HIGH)  | MEDIUM | Two generators + CI wiring + docs render; the `OpenApiRouter`↔`AppState` ergonomics and the `starlight-openapi` schema path were the two residual unknowns, both resolved without rework. |
| Confidence | 8/10 one-pass           | Met    | Both flagged risks (Task 7 state generics, Task 10 local schema path) landed clean; `split_for_parts()` before `.with_state()` returned `Router<AppState>` as predicted.                  |

**Deviations from plan**: none load-bearing. `ProblemDetails` documented as a
doc-only `ToSchema` DTO (no runtime body on the pilot's 503 — noted, reconciled
in Phase 2). No `chrono` feature pulled. No new dev-deps.

---

## Tasks Completed

| #     | Task                                                         | Milestone | Status |
| ----- | ------------------------------------------------------------ | --------- | ------ |
| 1–2   | `ci.yml` `docs` job + `ci-gate` wiring; `docs.yml` PR dedupe | M1 `5deb55a` | ✅     |
| 3     | `Cargo.toml` deps (`utoipa` 5.5, `utoipa-axum` 0.2)          | M3        | ✅     |
| 4–5   | `config/reference.rs` renderer + `REQUIRED_ENV_VARS` extract + drift test | M2 `6febe36` | ✅ |
| 6–8   | `health.rs` annotations; `openapi.rs` + `lib.rs` wire; OpenAPI drift test | M3 `a8aff38` | ✅ |
| 9–11  | `reference/index.md`; `astro.config.mjs` + `starlight-openapi`; intro note | M4 `11e1ed3` | ✅ |
| 12    | Full local verification + REGEN-clean                       | M5        | ✅     |

---

## Validation Results

| Check                                            | Result | Details                                            |
| ------------------------------------------------ | ------ | -------------------------------------------------- |
| `cargo fmt --check`                              | ✅     | Clean                                              |
| `cargo clippy --workspace --all-targets -D warnings` | ✅ | Zero warnings (offline)                            |
| `cargo test --test gen_openapi --test gen_config_ref` | ✅ | 4 pass (drift + secret-safety + 3.1 + paths)       |
| `REGEN=1` drift tests → `git status`             | ✅     | No diff — committed artifacts current              |
| `npm run build` (docs)                           | ✅     | 12 pages; `/api/operations/{health,ready}` + `/reference/{,configuration}` rendered |
| `markdownlint-cli2` (CI glob `*.md`)             | ✅     | 64 files, 0 errors (`.mdx` correctly excluded)     |
| `actionlint` ci.yml + docs.yml                   | ✅     | Clean                                              |
| `zizmor` ci.yml                                  | ✅     | No findings (6 suppressed)                         |
| `typos` / `shellcheck` / `hadolint` / `yamllint` | ✅     | All clean                                          |

**Full DB-backed `cargo test`**: runs in CI. Per the repo validation split
(`.husky/pre-push` = fmt + clippy; DB tests stay CI), local verification covers
fmt + clippy + the DB-free generators. All changed Rust paths (config render,
openapi module, health annotations, gen tests) are DB-free; no schema/query
changes, so DB-backed suite risk is nil.

---

## Files Changed (vs `main`, excl. lockfiles)

| Area    | Files | Net LOC |
| ------- | ----- | ------- |
| backend | `Cargo.toml`, `clippy.toml`, `config/mod.rs`, `config/reference.rs` (new), `lib.rs`, `openapi.rs` (new), `routes/health.rs`, `tests/gen_config_ref.rs` (new), `tests/gen_openapi.rs` (new) | ~+660 |
| docs    | `astro.config.mjs`, `package.json`, `openapi.json` (new), `reference/index.md` (new), `reference/configuration.mdx` (new), `getting-started/introduction.md` | ~+240 |
| CI      | `ci.yml` (+63), `docs.yml` (±30), `.prettierignore` (+11) | ~+100 |

Lockfiles: `Cargo.lock` +46, `docs/package-lock.json` +832 (starlight-openapi
dep tree).

---

## Coverage mechanism — proof

The plan's load-bearing claim is that removing a `#[utoipa::path]` from a
`routes!`-registered handler fails compilation. The pilot wires both health
handlers through `routes!(health::health, health::ready)`; utoipa's `routes!`
macro references the per-handler `__path_*` items the attribute generates, so a
missing annotation is a hard compile error, not a silent doc gap. This is the
Phase-1 enforcement floor; Phase 2 adds the raw-`.route(` grep-guard once all
modules are annotated.

---

## Security (hard rule 6)

Config surface documented by name only — the schema renders secret defaults as
`""`/`null`, guarded by the pre-existing
`config_schema_has_no_secret_default_values` test, and `gen_config_ref` asserts
`OIDC_CLIENT_SECRET` appears by name with no value. No new user-input path; the
generators are outbound-free and run at build/test time only; no response
headers changed. Stands up to security review.

---

## Notes for Phase 2

1. **`ProblemDetails` is doc-only** — the pilot's `ready` still returns a bare
   503 with no body at runtime; the documented `body = ProblemDetails` is
   aspirational. Reconcile when real route DTOs are annotated.
2. **`ApiDoc::info.version` is the doc version**, independent of the URL-path
   version. The `/api/v1` move travels with Phase-2 data-route annotation (it
   touches backend mounts + frontend client + tests as one change).
3. **`starlight-openapi` schema path** is project-root-relative (`./openapi.json`
   → `docs/openapi.json`). If the spec ever moves, update `astro.config.mjs`.

---

## Artifacts

- Report: `.claude/PRPs/reports/docs-as-done-report.md` (this file)
- Plan: `.claude/PRPs/plans/completed/docs-as-done.plan.md` (archived on close)
