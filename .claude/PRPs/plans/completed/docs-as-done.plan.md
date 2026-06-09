# Feature: docs-as-done — generated reference + CI gate (Phase 1 mechanism slice)

> **Regenerated 2026-06-09 against merged `main`** (post-UNK-375 / PR #441 `5562855`). Supersedes the
> prior stale draft. The config-reference half is re-scoped to render from the committed declarative
> config schema (`schemars`) instead of `syn`-parsing Rust source — see "Solution Statement" and the
> "Why schema-driven, not syn" note. All OpenAPI-half line references re-verified against current main.

## Summary

Stand up the **docs-as-done mechanism** that makes generated reference documentation a build-gated
part of every PR (CLAUDE.md hard rule 10), mirroring how the TDD mandate prevents untested code. This
Phase-1 slice lands the full pipeline end-to-end on a thin vertical:

1. a **config reference** (`configuration.mdx`) rendered from the in-process declarative config schema
   (`schemars::schema_for!(Config)`) plus the operator-facing `ENV_MAP`, committed and drift-gated by a
   backend test;
2. an **OpenAPI 3.1 reference** proven on ONE pilot route module (`health`) via `utoipa` +
   `utoipa-axum`, committed as `docs/openapi.json` and rendered into Starlight by `starlight-openapi`;
3. a **CI gate**: a new `docs` job in `ci.yml` (astro build + lychee) wired into the `ci-gate`
   aggregator, plus two backend drift tests that fail when a generated artifact goes stale or a pilot
   handler loses its annotation.

Full OpenAPI coverage of the remaining ~13 route modules is **explicitly deferred** to a follow-up
issue (Phase 2) that ratchets module-by-module, mirroring the CSRF-rollout and comment-policy-rollout
patterns.

## User Story

As a **Reverie maintainer / external contributor**
I want **the API and config reference to regenerate from source and gate CI on drift**
So that **docs can never silently fall behind the code, the way the TDD mandate prevents untested code**.

## Problem Statement

Today there is zero generated reference. `docs.yml` (Starlight build) is a **separate workflow not
listed in `ci-gate`'s `needs`** (`ci.yml:1038-1064`), so a broken docs build does not block merge when
a PR also touches backend/frontend. There is no API reference and no rendered config reference, and no
mechanism to detect that a new endpoint shipped undocumented. "Docs are part of done" is policy with
no enforcement.

**Testable:** after this change, (a) deleting a `#[utoipa::path]` annotation from a pilot handler
registered via `routes!` fails compilation; (b) editing a config field's `///` doc / default / range
without regenerating `configuration.mdx` fails a backend test; (c) editing a pilot handler's response
shape without regenerating `docs/openapi.json` fails a backend test; (d) a Starlight build failure
fails `ci-gate`.

## Solution Statement

Reuse the repo's existing **generate→commit→CI-`--check`** precedent. The config-schema drift gate
already lives at `ci.yml:251-257` (`cargo run -- print-config-schema | diff -u config.schema.json -`),
itself modelled on `cargo sqlx prepare --check` (`ci.yml:248-249`). Both generators run as backend
**integration tests that double as writers** (`REGEN=1` writes the artifact; default asserts the
committed artifact matches) — so drift is gated _for free_ by the existing `backend` job's `cargo test`,
no DB required (pure schema render + pure utoipa doc build).

- **Config reference**: a crate function `config::reference::render_markdown()` builds the
  `configuration.mdx` body from `schemars::schema_for!(config::Config)` (same source the committed
  `config.schema.json` is gated against) joined with `ENV_MAP` to recover operator-facing variable
  names. The schema already carries every field's description, type, default, and range constraints,
  and renders secret defaults as `""`/`null` — so the table is **drift-free by construction and
  secret-safe by construction**. No `syn`, no second source of truth.
- **OpenAPI reference**: a `#[derive(OpenApi)] struct ApiDoc` aggregates the pilot's paths; the `health`
  pilot is wired through `OpenApiRouter<AppState>` + `routes!(...)` so an un-annotated handler fails to
  compile. `split_for_parts()` yields `(Router<AppState>, OpenApi)` — the `Router` part merges back into
  `lib.rs`'s router (runtime behavior unchanged), the `OpenApi` part feeds the spec writer.
- **CI gate**: a new `docs` job in `ci.yml` runs `npm ci` + `astro build` + lychee internal-link check,
  gated on a new `docs` path-filter output, and is appended to `ci-gate.needs` (mirroring how existing
  conditional jobs are aggregated). `docs.yml` drops its `pull_request` trigger to dedupe; it keeps the
  push-to-`main` + tag Pages deploy. `starlight-openapi` renders the committed `docs/openapi.json` into
  the API sidebar group; the existing `reference/` autogenerate slot (`astro.config.mjs:29`) picks up
  the narrative `index.md` + generated `configuration.mdx`.

## Metadata

| Field            | Value                                                                                                                                                                           |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type             | NEW_CAPABILITY                                                                                                                                                                  |
| Complexity       | MEDIUM (bounded slice of a HIGH feature)                                                                                                                                        |
| Systems Affected | backend (Cargo deps, openapi module, health routes, config::reference, 2 gen tests), docs (Starlight plugin + generated pages), CI (ci.yml docs job + ci-gate; docs.yml dedupe) |
| Dependencies     | `utoipa` 5.5.x, `utoipa-axum` 0.2.x (prod); `starlight-openapi` ^0.25.3 (docs npm). No new dev-deps — config render reuses existing `schemars` 1.2 + `serde_json`.              |
| Estimated Tasks  | 12                                                                                                                                                                              |
| Linear           | UNK-370 (Phase 1, v0.1.0 milestone). Phase 2 = new follow-up issue for full coverage.                                                                                           |

---

## UX Design

### Before State

```text
docs site (Starlight, base /reverie)
  Getting Started ─ Introduction
  Design ─ philosophy / visual-identity / testing-scope
  Reference ─ (EMPTY — autogenerate slot wired at astro.config.mjs:29, no files)

CI on a PR touching backend + docs:
  backend job ─ test/clippy/fmt/sqlx-check/config-schema-check ✓
  docs.yml ─ astro build  (SEPARATE workflow, pull_request trigger, NOT in ci-gate)
  ci-gate ─ needs: [changes, backend, msrv, audit, dependency-review, workflow-lint,
                    workflow-security, repo-lint, frontend, a11y, staging-smoke, secret-scan]
            ← docs build is NOT a dependency

  → A broken Starlight build or an undocumented new pilot endpoint MERGES GREEN.
```

### After State

```text
docs site
  Reference ─ Overview        (index.md — narrative; "generated, do not hand-edit")
            ─ Configuration   (configuration.mdx ← rendered from schemars schema + ENV_MAP)
  API Reference ─ Health      (injected by starlight-openapi from docs/openapi.json)
                  └ pilot module proves the pipeline; phase 2 adds the rest

CI on a PR touching backend + docs:
  backend job ─ ...existing... + gen_openapi drift test + gen_config_ref drift test ✓
              ─ (delete a #[utoipa::path] on a routes!-registered handler → COMPILE ERROR)
  docs job (NEW in ci.yml) ─ npm ci + astro build + lychee --offline ✓  (gated on changes.docs)
  ci-gate ─ needs: [...existing..., docs]   ← docs build now BLOCKS merge
  docs.yml ─ push(main + v* tags) + workflow_dispatch only  (PR build moved to ci.yml)

  → Stale/missing reference or broken docs build CANNOT merge.
```

### Interaction Changes

| Location                 | Before                   | After                                      | User Impact                                            |
| ------------------------ | ------------------------ | ------------------------------------------ | ------------------------------------------------------ |
| `docs/.../reference/`    | empty sidebar group      | Overview + Configuration                   | Operators read generated env/config reference          |
| docs sidebar             | no API group             | `API Reference ─ Health` (plugin-injected) | Operators read generated API reference                 |
| `ci.yml` `ci-gate.needs` | no docs                  | includes `docs` job                        | docs build failure blocks merge                        |
| `backend` test suite     | no doc drift tests       | 2 drift tests                              | doc drift fails CI for free                            |
| `health` handlers        | plain `Router<AppState>` | utoipa-annotated via `OpenApiRouter`       | endpoints appear in generated spec; behavior unchanged |
| `docs.yml`               | builds on PR + push      | push/tags/dispatch only                    | no duplicate PR build                                  |

---

## Mandatory Reading

**Implementation agent MUST read these before starting. Line refs verified against current `main` 2026-06-09.**

| Priority | File                                                    | Lines                               | Why Read This                                                                                                                                                                                                                                                                                                                                                                                                        |
| -------- | ------------------------------------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P0       | `backend/src/lib.rs`                                    | 81-168                              | Router assembly. Health merge is at **line 111** inside the `let mut api_like = Router::new().merge(...)` chain (110-128). Sole integration point for the pilot.                                                                                                                                                                                                                                                     |
| P0       | `backend/src/lib.rs`                                    | 477-524                             | `parse_command` + `print_config_schema()` — the schema-emit pattern (`schemars::schema_for!`, `serde_json::to_string_pretty`, direct `io::Write` — NOT `println!`) the config-ref renderer mirrors.                                                                                                                                                                                                                  |
| P0       | `backend/src/routes/health.rs`                          | all (35 lines)                      | Pilot module. `router()` (15-19) returns `Router<AppState>`; `health()` (21-23) → bare `&'static str` "ok"; `ready()` (25-34) → `Result<impl IntoResponse, StatusCode>` ("ok" or 503). Mirror this shape.                                                                                                                                                                                                            |
| P0       | `backend/src/config/provider.rs`                        | 23-111, 138-139                     | `pub const ENV_MAP: &[(&str, &str)]` (55 entries, `(env_var, dotted.field.path)`). Docstring at 138-139 names it as the var↔field source the config-ref generator consumes. Note `RUST_LOG`+`REVERIE_LOG_LEVEL` both → `log_level`; `REVERIE_PUBLIC_URL` → `opds.public_url`.                                                                                                                                        |
| P0       | `backend/src/config/mod.rs`                             | 48-178, 325-336, 644-667, 1177-1200 | `Config` struct + sub-struct fields with `///` docs (schema desc source); **Gate 3 required-field check (325-336) — the authoritative required-var set, since the schema's `required` array is EMPTY (container `#[serde(default)]`)**; `staging_runtime_example_keys_are_in_env_map` (644-667); `config_schema_has_no_secret_default_values` (1177-1200 — proves secrets render safe).                              |
| P0       | `adr/2026-06-08-api-versioning-openapi.md`              | all                                 | The ADR this issue implements. **Decision V1+S1: data API under `/api/v1/...`; OpenAPI 3.1 generated code-first.** Critically: `/health`, `/readiness`, `/auth`, `/opds` stay **unversioned** (operational/standard-protocol paths) — so the `/health` pilot is ADR-correct as-is. The `/api/*`→`/api/v1/*` mount move is Phase-2 companion work (touches backend mounts + frontend client + tests), NOT this pilot. |
| P0       | `backend/config.schema.json`                            | all (428 lines)                     | The shape the renderer walks: top-level `properties` + `$defs` (sub-structs by `$ref`), `description`/`type`/`format`/`default`/`minimum`/`maximum`, secrets as `""`/`null`, `url::Url`→`["string","null"],format:uri`, enums→`oneOf` const.                                                                                                                                                                         |
| P0       | `.github/workflows/ci.yml`                              | 29-34, 248-257, 1038-1064           | `changes` outputs (29-34 — **no `docs` output yet**); sqlx `--check` (248-249) + config-schema `--check` (251-257) precedent; `ci-gate` aggregator + its 12-entry `needs` (1038-1064).                                                                                                                                                                                                                               |
| P1       | `.github/workflows/docs.yml`                            | all (129 lines)                     | Build steps to lift into the `ci.yml` `docs` job (node 24.16.0, no npm cache, `cd docs && npm ci && npm run build`, lychee `--offline` gate). Strip the `pull_request` trigger here (Task 2). Keep `deploy` job.                                                                                                                                                                                                     |
| P1       | `docs/astro.config.mjs`                                 | all (34 lines)                      | Add `starlightOpenAPI([...])` to `plugins:` + spread `openAPISidebarGroups` into `sidebar`. `reference` autogenerate slot already present (line 29). `base: "/reverie"`.                                                                                                                                                                                                                                             |
| P1       | `backend/src/error/mod.rs`                              | 54-137, 139-248                     | `AppError` (12 variants) → RFC 7807 `application/problem+json` (`IntoResponse` impl 139-248; body assembled 230-235; `problems.rs` `PROBLEM_BASE = https://reverie.example/probs`). Document the problem+json response shape via a doc-only `ToSchema` DTO.                                                                                                                                                          |
| P1       | `backend/tests/cookie_jar_sanity.rs`                    | all                                 | The sole existing `tests/` integration file — the header/import/`#![allow(...)]` pattern to mirror. No `tests/support/` exists; shared helpers live at `backend/src/test_support.rs`.                                                                                                                                                                                                                                |
| P2       | `docs/src/content/docs/getting-started/introduction.md` | 1-4                                 | Frontmatter shape (`title` + `description`, content immediately after closing `---`) to mirror for new reference pages.                                                                                                                                                                                                                                                                                              |

**External Documentation:**

| Source                                                                                                    | Section                                           | Why Needed                                                                                                                                                                                                                                                                             |
| --------------------------------------------------------------------------------------------------------- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [utoipa-axum 0.2.0 docs](https://docs.rs/utoipa-axum/latest/utoipa_axum/router/struct.OpenApiRouter.html) | `OpenApiRouter`, `routes!`, `split_for_parts`     | Pilot integration; `split_for_parts()` returns `(Router<S>, OpenApi)` — split BEFORE `.with_state()`; merge the `Router<AppState>` part into `lib.rs`.                                                                                                                                 |
| [utoipa 5.5.0 docs](https://docs.rs/utoipa/5.5.0/utoipa/)                                                 | `#[utoipa::path]`, `#[derive(OpenApi)]`, features | Emits OpenAPI `3.1.0`. `to_pretty_json()` for the spec writer. Features `axum_extras` + `time` + `uuid` (NOT `chrono` — Reverie uses `time`). A `routes!`-registered handler without `#[utoipa::path]` fails to compile (coverage mechanism).                                          |
| [starlight-openapi config](https://starlight-openapi.vercel.app/getting-started/)                         | local `schema` path, `openAPISidebarGroups`       | `0.25.3`; peer `@astrojs/starlight ≥0.38` / `astro ≥6` / node ≥22.12 (satisfied: ^0.39.2 / ^6.4.2). `schema` is project-root-relative → `./openapi.json` resolves to `docs/openapi.json`. Renders 3.1. Pin **≥0.25.2** (fixes `required`+`anyOf`, load-bearing for utoipa 3.1 output). |
| [schemars 1.x output shape](https://docs.rs/schemars/latest/schemars/)                                    | `$defs`, `anyOf`-null, `required`                 | Renderer walks `serde_json::Value`: `$defs` (not `definitions`); `Option<T>`→`anyOf:[T,{null}]` (not in `required`); `required` is a parent-object array; resolve `$ref` via `#/$defs/` strip. ~50-100 LOC, no extra crate.                                                            |

---

## Patterns to Mirror

**DRIFT-CHECK (generate → commit → CI `--check`) — config-schema gate, the closest precedent:**

```yaml
# SOURCE: .github/workflows/ci.yml:251-257
- name: Check config schema freshness
  run: cargo run --quiet --locked -- print-config-schema | diff -u config.schema.json -
```

**SCHEMA EMIT (renderer mirrors this — direct io::Write, never println!):**

```rust
// SOURCE: backend/src/lib.rs:515-524
pub fn print_config_schema() -> anyhow::Result<()> {
    use std::io::Write as _;
    let schema = schemars::schema_for!(config::Config);
    let mut json = serde_json::to_string_pretty(&schema).context("serialize config schema")?;
    json.push('\n');
    std::io::stdout().write_all(json.as_bytes()).context("write config schema to stdout")?;
    Ok(())
}
```

**ENV_MAP (var↔field source of truth for operator-facing var names):**

```rust
// SOURCE: backend/src/config/provider.rs:23-111  (pub const ENV_MAP: &[(&str, &str)], 55 entries)
("REVERIE_PORT", "port"),
("REVERIE_DB_MAX_CONNECTIONS", "db_max_connections"),
("REVERIE_ENRICHMENT_CONCURRENCY", "enrichment.concurrency"),
("REVERIE_PUBLIC_URL", "opds.public_url"),     // var name diverges from field path
("RUST_LOG", "log_level"),                      // alias: REVERIE_LOG_LEVEL also → log_level
```

**ROUTER MODULE SHAPE (pilot becomes an OpenApiRouter equivalent):**

```rust
// SOURCE: backend/src/routes/health.rs:15-19
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(ready))
}
```

**ROUTER MERGE POINT (replace the health merge with the split Router part):**

```rust
// SOURCE: backend/src/lib.rs:110-111  (api_like chain; health at line 111)
let mut api_like = Router::new()
    .merge(routes::health::router())   // ← becomes the OpenApiRouter split Router<AppState>
    .merge(routes::auth::router())
    // ...
```

**RFC 7807 RESPONSE BODY (document this shape in pilot responses):**

```rust
// SOURCE: backend/src/error/mod.rs:230-235
let mut body = serde_json::json!({
    "type": problems::problem_type(slug),   // https://reverie.example/probs/<slug>
    "title": title,
    "status": status.as_u16(),
    "detail": detail,
});  // Content-Type: application/problem+json (line 244)
```

**INTEGRATION TEST HEADER (mirror for the two gen tests):**

```rust
// SOURCE: backend/tests/cookie_jar_sanity.rs:1-16
//! <module docstring explaining why the test lives here>
#![allow(clippy::expect_used, clippy::unwrap_used)]
use reverie_api::/* ... */;
```

**CI-GATE AGGREGATOR (append `docs` to needs; mirror conditional-job result handling):**

```yaml
# SOURCE: .github/workflows/ci.yml:1038-1064  (needs list 1041-1053)
ci-gate:
  needs: [
      changes,
      backend,
      msrv,
      audit,
      dependency-review,
      workflow-lint,
      workflow-security,
      repo-lint,
      frontend,
      a11y,
      staging-smoke,
      secret-scan,
    ] # ← append: docs
```

---

## Files to Change

| File                                                                  | Action        | Justification                                                                                                                                                                                                                                                                                                    |
| --------------------------------------------------------------------- | ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `backend/Cargo.toml`                                                  | UPDATE        | Add to `[dependencies]`: `utoipa = { version = "5.5", features = ["axum_extras", "time", "uuid"] }`, `utoipa-axum = "0.2"`. No new dev-deps (config render reuses `schemars` + `serde_json`).                                                                                                                    |
| `backend/src/openapi.rs`                                              | CREATE        | `#[derive(OpenApi)] struct ApiDoc` (info/tags/components); doc-only `ProblemDetails` `#[derive(ToSchema)]` mirroring RFC 7807 body; `pub fn openapi() -> utoipa::openapi::OpenApi` (builds the pilot `OpenApiRouter`, `split_for_parts`, returns the merged spec); `pub fn spec_json() -> String` (pretty 3.1).  |
| `backend/src/lib.rs`                                                  | UPDATE        | `pub mod openapi;`. Build the pilot via `OpenApiRouter::<AppState>::new().routes(routes!(health::health, health::ready))`, `.split_for_parts()`, merge the `Router<AppState>` part at the line-111 merge point (replacing `routes::health::router()`).                                                           |
| `backend/src/routes/health.rs`                                        | UPDATE        | `#[utoipa::path(...)]` on `health` + `ready` documenting the existing string/status responses (200 text body; 503 problem+json referencing `ProblemDetails`). NO runtime change, NO new DTO on the handlers themselves.                                                                                          |
| `backend/src/config/mod.rs` (+ new `backend/src/config/reference.rs`) | CREATE/UPDATE | New `pub(crate) mod reference;` with `render_markdown() -> String`: walk `schemars::schema_for!(Config)` joined to `ENV_MAP`, emit the MDX (frontmatter + table). `mod.rs` declares the submodule AND extracts the Gate 3 required list into `pub(crate) const REQUIRED_ENV_VARS` (shared by Gate 3 + renderer). |
| `backend/tests/gen_openapi.rs`                                        | CREATE        | Drift test: `reverie_api::openapi::spec_json()` vs committed `docs/openapi.json`; asserts `openapi` field == `3.1.x` and pilot paths present; `REGEN=1` writes.                                                                                                                                                  |
| `backend/tests/gen_config_ref.rs`                                     | CREATE        | Drift test: `reverie_api::config::reference::render_markdown()` (re-exported) vs committed `configuration.mdx`; `REGEN=1` writes.                                                                                                                                                                                |
| `docs/openapi.json`                                                   | CREATE        | Committed generated spec (3.1), pilot routes only. Path = project-root-relative `./openapi.json` for the plugin.                                                                                                                                                                                                 |
| `docs/src/content/docs/reference/index.md`                            | CREATE        | Narrative skeleton: what Reference is, that pages are generated from source, how to regenerate (`REGEN=1 cargo test`), "do not hand-edit generated pages".                                                                                                                                                       |
| `docs/src/content/docs/reference/configuration.mdx`                   | CREATE        | Generated config reference (committed artifact).                                                                                                                                                                                                                                                                 |
| `docs/package.json`                                                   | UPDATE        | Add `starlight-openapi` `^0.25.3`.                                                                                                                                                                                                                                                                               |
| `docs/astro.config.mjs`                                               | UPDATE        | Import + register `starlightOpenAPI([{ base: 'api', schema: './openapi.json', label: 'API Reference' }])` in `plugins:`; spread `openAPISidebarGroups` into `sidebar`.                                                                                                                                           |
| `.github/workflows/ci.yml`                                            | UPDATE        | Add `docs` output to `changes` (`docs: docs/**` + `.github/workflows/docs.yml`); add a `docs` job (node 24.16.0, `npm ci`, `astro build`, lychee `--offline`) gated `if: needs.changes.outputs.docs == 'true'`; append `docs` to `ci-gate.needs` and mirror existing conditional-job result handling.            |
| `.github/workflows/docs.yml`                                          | UPDATE        | Remove the `pull_request` trigger (dedupe). Keep `push` (main + `v*`) + `workflow_dispatch` + the `deploy` job.                                                                                                                                                                                                  |
| `docs/src/content/docs/getting-started/introduction.md`               | UPDATE        | One short paragraph documenting the docs-as-done mechanism for contributors (rationale-in-user-docs).                                                                                                                                                                                                            |

---

## NOT Building (Scope Limits — Phase 2 / future)

- **OpenAPI coverage of the other ~13 route modules** (`library`, `users`, `shelves`, `series`,
  `settings`, `auth`, `tokens`, `metadata`, `dashboard`, `enrichment`, `ingestion`, `opds`). Deferred to
  a new follow-up issue that ratchets module-by-module.
- **The `/api/*` → `/api/v1/*` mount move** mandated by `adr/2026-06-08-api-versioning-openapi.md`.
  The pilot `/health` is an ADR-exempt operational path (stays unversioned), so Phase 1 needs no prefix
  move. The versioning move is Phase-2 companion work (it touches backend mounts + frontend API client +
  tests in one change) — fold it into the Phase-2 follow-up issue's scope.
- **The raw-`.route(` grep-guard + allowlist** (phase-2 coverage enforcement). Cannot be enabled in
  phase 1 because 13 modules still use plain `.route(`. Pilot coverage is enforced by `routes!`
  compile-checking only.
- **Annotating real response DTOs** (Book, etc.) with `#[derive(ToSchema)]`. Phase 1 documents only the
  pilot's existing string/status responses + the shared `ProblemDetails` error shape. Rich schema
  components arrive in phase 2 with the real route modules.
- **oasdiff breaking-change detection** on `openapi.json`. Future hardening once coverage is broad.
- **Serving the spec at a runtime endpoint** (`/api-docs/openapi.json`) or Swagger UI. Docs-site render only.
- **Automated "every PR ships narrative docs" enforcement.** Remains a CLAUDE.md policy gate (rule 10);
  CI gates the docs _build_, not narrative-presence-per-PR.
- **OpenAPI 3.2 output.** No Rust emitter exists; target 3.1, revisit when utoipa adds 3.2.
- **A `print-config-reference` CLI subcommand.** The renderer is exercised as a test-time function only;
  no runtime/CLI surface needed (unlike `print-config-schema`, which feeds the CI `diff` gate).

---

## Step-by-Step Tasks

Execute in order. TDD: write the failing test/assertion first where noted.

### Task 1: UPDATE `.github/workflows/ci.yml` — `docs` job + `ci-gate` wiring (separable first win)

- **ACTION**: (a) ADD `docs: ${{ steps.filter.outputs.docs }}` to the `changes` job outputs (29-34) and a `docs:` path group (`docs/**`, `.github/workflows/docs.yml`) to the filter. (b) ADD a `docs` job mirroring `docs.yml`'s build steps (node 24.16.0, no npm cache, `cd docs && npm ci && npm run build`, lychee `--offline` internal-link gate), gated `needs: changes` + `if: needs.changes.outputs.docs == 'true'`. (c) APPEND `docs` to `ci-gate.needs` (1041-1053) and mirror the aggregator's existing skipped/success result handling for conditional jobs.
- **MIRROR**: `.github/workflows/docs.yml` build steps; `ci.yml:1038-1064` aggregator; existing conditional jobs (`frontend`, `a11y`) for the result-check pattern.
- **GOTCHA**: A conditionally-skipped `docs` job must be treated as pass by the aggregator (memory: skipped conditional jobs block merge unless the gate handles `result == 'skipped'`). Cross-workflow `needs` is impossible — this is why the job lives in `ci.yml`, not referenced from `docs.yml`. `docs.json`/`configuration.mdx` are under `docs/`, so a backend regen that touches them trips `docs/**` automatically; no `backend` trigger needed.
- **VALIDATE**: `actionlint .github/workflows/ci.yml` (repo `workflow-lint`); `zizmor .github/workflows/ci.yml` clean.

### Task 2: UPDATE `.github/workflows/docs.yml` — drop PR trigger (dedupe)

- **ACTION**: REMOVE the `pull_request` trigger block. Keep `push` (main + `v*` tags) + `workflow_dispatch` + the `deploy` job.
- **GOTCHA**: Do NOT remove the Pages `deploy` job or the `upload-pages-artifact` step — only the PR-path build, now duplicated by Task 1.
- **VALIDATE**: `actionlint`; confirm no PR double-builds the site.

### Task 3: UPDATE `backend/Cargo.toml` — deps

- **ACTION**: ADD to `[dependencies]`: `utoipa = { version = "5.5", features = ["axum_extras", "time", "uuid"] }`, `utoipa-axum = "0.2"`.
- **GOTCHA**: Do NOT enable `chrono` (Reverie uses the `time` crate). utoipa pulls no `schemars` — intentional. Run `cargo audit` after (new deps); justify any ignore per repo policy.
- **VALIDATE**: `cargo build -p reverie_api` compiles; `cargo audit` clean (or justified ignore).

### Task 4: CREATE `backend/src/config/reference.rs` — schema→markdown renderer (TDD: write the assertion in Task 5 first)

- **ACTION**: First, EXTRACT the Gate 3 required-field list (`config/mod.rs:326-331` — `DATABASE_URL`, `OIDC_ISSUER_URL`, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET`, `OIDC_REDIRECT_URI`) into a shared `pub(crate) const REQUIRED_ENV_VARS: &[&str]` so Gate 3 and the renderer consume ONE source; refactor Gate 3 to iterate it; add a unit test asserting the set is non-empty and every entry is in `ENV_MAP`. Then CREATE `pub(crate) fn render_markdown() -> String`. Build `let schema = serde_json::to_value(schemars::schema_for!(super::Config))`. Resolve `$defs`. Build a field-path→env-var index from `ENV_MAP` (handle the `log_level` alias: list both vars; handle `opds.public_url` divergence — index by dotted path). Walk top-level `properties` + each sub-struct (`$ref`→`$defs`), emitting one MDX section/table per top-level scalar group + per sub-struct, columns `Variable | Type | Required | Default | Description`. **Required = membership in `REQUIRED_ENV_VARS`** (do NOT use the schema's `required` array — it is empty because every struct is `#[serde(default)]`). Note `DATABASE_URL_MIGRATION` as conditionally required (only when `REVERIE_AUTO_MIGRATE=true`) per `config/mod.rs:289` / the migration ADR. Prepend frontmatter (`---\ntitle: Configuration\ndescription: ...\n---`) and a "generated — do not hand-edit" banner.
- **MIRROR**: `lib.rs:515-524` schema-emit; `config/mod.rs` sub-struct names; Gate 3 loop at `mod.rs:326-336`.
- **GOTCHA**: The schema `required` array is EMPTY (container-level `#[serde(default)]`) and EVERY var is in `ENV_MAP` — so neither is a valid required-ness source; only Gate 3's set is. `Option<T>`/`url::Url` fields appear as `anyOf:[T,{null}]` with no top-level `type` — filter the `null` arm. `$defs` (not `definitions`). `default` key absent if unresolved — render blank. Secrets already carry `""`/`null` defaults (do NOT special-case values; the schema is secret-safe). Vars in `ENV_MAP` with no schema property (none expected) → fail loud. Deterministic ordering (drive row order from `ENV_MAP`'s declaration order, not the schema's map iteration).
- **VALIDATE**: covered by Task 5.

### Task 5: CREATE `backend/tests/gen_config_ref.rs` — config drift test (TDD)

- **ACTION**: Re-export the renderer (`pub use` path or a thin `pub fn` in `lib.rs`) so the integration test can call it. Test reads `CARGO_MANIFEST_DIR/../docs/src/content/docs/reference/configuration.mdx`, asserts equal to `render_markdown()`; on `REGEN=1` writes the file. Then run `REGEN=1` to produce the committed `.mdx`; commit it.
- **MIRROR**: `cookie_jar_sanity.rs` header; sqlx `--check` snapshot-on-env-flag idiom.
- **GOTCHA**: CWD in integration tests = `backend/`; use `env!("CARGO_MANIFEST_DIR")` joined with `../docs/...` for robustness. Assert at least one known var (e.g. `REVERIE_PORT`, default `3000`) and one secret (e.g. `OIDC_CLIENT_SECRET`) appear by name with no value.
- **VALIDATE**: `cargo test --test gen_config_ref` green with committed file; flips red if a config doc/default/range changes without regen.

### Task 6: UPDATE `backend/src/routes/health.rs` — annotate pilot handlers

- **ACTION**: ADD `#[utoipa::path(get, path = "/health", responses((status = 200, description = "Liveness OK", body = String)))]` to `health`, and `#[utoipa::path(get, path = "/health/ready", responses((status = 200, description = "Ready", body = String), (status = 503, description = "Dependencies unavailable", body = ProblemDetails)))]` to `ready` (import `ProblemDetails` from `crate::openapi`).
- **MIRROR**: existing `health.rs` signatures; do NOT change runtime behavior (responses stay `&'static str` / bare `StatusCode`).
- **GOTCHA**: `path =` must match the `.route()` path exactly (`/health`, `/health/ready`). The 503's `body = ProblemDetails` is documentation only — `ready` still returns a bare 503 with no body at runtime; that mismatch is acceptable for the pilot (note it) and resolved in phase 2.
- **VALIDATE**: `cargo build`; `cargo test routes::health` green (behavior tests unchanged).

### Task 7: CREATE `backend/src/openapi.rs` + wire pilot in `lib.rs`

- **ACTION**: CREATE `ApiDoc` (`#[derive(OpenApi)]`, `info(title="Reverie API", version=...)`, `components(schemas(ProblemDetails))`, `tags(...)`). CREATE doc-only `#[derive(ToSchema, Serialize)] struct ProblemDetails { type, title, status, detail, instance: Option<String> }` mirroring `error/mod.rs:230-240`. ADD `pub fn openapi() -> utoipa::openapi::OpenApi` building `OpenApiRouter::with_openapi(ApiDoc::openapi()).routes(routes!(routes::health::health, routes::health::ready)).split_for_parts()` and returning the `OpenApi` part; `pub fn spec_json() -> String` = `openapi().to_pretty_json().unwrap()` + trailing newline. In `lib.rs`: `pub mod openapi;`, and at line 111 replace `.merge(routes::health::router())` with the merged `Router<AppState>` part of the same `split_for_parts()` (factor a shared `fn health_openapi_router() -> OpenApiRouter<AppState>` used by both `openapi()` and `lib.rs` to avoid divergence).
- **MIRROR**: utoipa-axum `split_for_parts` pattern; `lib.rs:110-111` merge style.
- **GOTCHA**: `OpenApiRouter` is generic over state — use `OpenApiRouter::<AppState>`. `split_for_parts()` returns `(Router<AppState>, OpenApi)` when called before `.with_state()`. A `routes!` handler WITHOUT `#[utoipa::path]` fails to compile — verify by temporarily removing one annotation (expect compile error), then restore; document this in `gen_openapi.rs` as a comment.
- **VALIDATE**: `cargo build`; manually confirm the temporary-removal compile error.

### Task 8: CREATE `backend/tests/gen_openapi.rs` — OpenAPI drift test (TDD)

- **ACTION**: Test calls `reverie_api::openapi::spec_json()`, reads `CARGO_MANIFEST_DIR/../docs/openapi.json`, asserts equal; `REGEN=1` writes. Assert `serde_json` parse → `openapi` field starts with `3.1`; assert `/health` + `/health/ready` paths present. Run `REGEN=1` to produce `docs/openapi.json`; commit it.
- **MIRROR**: sqlx `--check` semantics; Task 5 path handling.
- **GOTCHA**: `to_pretty_json` output is deterministic. Keep the trailing-newline convention consistent between writer and committed file.
- **VALIDATE**: `cargo test --test gen_openapi` green with committed file; red if `health.rs` annotations change without regen.

### Task 9: CREATE narrative skeleton `docs/src/content/docs/reference/index.md`

- **ACTION**: Short overview: Reference is generated from source — Configuration from the declarative config schema, API from the OpenAPI spec; "generated pages are not hand-edited — regenerate via `REGEN=1 cargo test --test gen_openapi --test gen_config_ref`". Link to Configuration + the API group.
- **MIRROR**: `getting-started/introduction.md:1-4` frontmatter + tone.
- **VALIDATE**: appears in the sidebar `Reference` group after `astro build`.

### Task 10: UPDATE `docs/package.json` + `docs/astro.config.mjs` — render plugin

- **ACTION**: `cd docs && npm install starlight-openapi@^0.25.3` (adds dep + updates lock). In `astro.config.mjs`: `import starlightOpenAPI, { openAPISidebarGroups } from 'starlight-openapi'`; add `plugins: [ starlightOpenAPI([{ base: 'api', schema: './openapi.json', label: 'API Reference' }]) ]` to the `starlight({...})` call; spread `...openAPISidebarGroups` into the `sidebar` array (after the existing groups).
- **MIRROR**: research's `astro.config.mjs` example.
- **GOTCHA**: `schema` is project-root-relative → `./openapi.json` resolves to `docs/openapi.json` (so the committed spec must live at `docs/openapi.json`, matching Task 8's write path). Peer deps satisfied (`@astrojs/starlight ^0.39.2`, `astro ^6.4.2`, node ≥22.12). The plugin injects its own sidebar group via `openAPISidebarGroups`; the `reference` autogenerate slot stays for `index.md` + `configuration.mdx`.
- **VALIDATE**: `cd docs && npm ci && npm run build` succeeds; `/api/...` and `/reference/...` pages exist in `docs/dist`.

### Task 11: UPDATE contributor-facing narrative (rationale in user docs)

- **ACTION**: Add one short paragraph to `getting-started/introduction.md` (or a small "Contributing" note) explaining the docs-as-done mechanism: what regenerates (config ref, OpenAPI spec), how CI gates it (`docs` job + backend drift tests), how to regenerate locally.
- **VALIDATE**: `markdownlint-cli2` + lychee pass.

### Task 12: Full local verification + regenerate-clean check

- **ACTION**: Run the full repo-lint + backend + docs verification locally (see Validation Commands). Confirm a clean `REGEN=1` run produces NO diff.
- **VALIDATE**: all levels exit 0; `git status --porcelain` clean after a `REGEN=1` test pass.

---

## Testing Strategy

### Tests to Write (TDD — test/assertion before implementation)

| Test File                                               | Test Cases                                                                                                             | Validates                         |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| `backend/tests/gen_config_ref.rs`                       | committed mdx matches render; `REVERIE_PORT`/`OIDC_CLIENT_SECRET` present (secret by name, no value); `REGEN=1` writes | config drift gate + secret-safety |
| `backend/tests/gen_openapi.rs`                          | committed spec matches generated; `openapi` == `3.1.x`; pilot paths present; `REGEN=1` writes                          | OpenAPI drift gate                |
| (compile-time) `routes!(health::health, health::ready)` | removing `#[utoipa::path]` → compile error                                                                             | coverage mechanism                |
| existing `routes::health` tests                         | unchanged behavior                                                                                                     | no regression in pilot            |

### Edge Cases Checklist

- [ ] Secret-named vars (`OIDC_CLIENT_SECRET`, `*_API_KEY`, `*_API_TOKEN`) render by name only — `""`/`null` default, never a value.
- [ ] `Option<T>` / `url::Url` config field → "Required: no", `anyOf`-null arm filtered from the Type cell.
- [ ] `log_level` alias (`RUST_LOG` + `REVERIE_LOG_LEVEL`) rendered without duplicating a single field confusingly.
- [ ] `REVERIE_PUBLIC_URL` → `opds.public_url` divergence resolved via `ENV_MAP`, not name-derivation.
- [ ] Editing a config field's `///` doc / default / range without regen → `gen_config_ref` red.
- [ ] Editing a pilot handler's documented response without regen → `gen_openapi` red.
- [ ] Removing a `#[utoipa::path]` on a `routes!`-registered handler → compile error.
- [ ] Malformed `docs/openapi.json` → `docs` job (astro build) fails (gate works).
- [ ] `docs` job skipped on a backend-only PR that doesn't touch `docs/**` → `ci-gate` still passes (skip handled).

---

## Validation Commands

### Level 1: STATIC ANALYSIS

```bash
cd backend && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cd docs && npx --no-install markdownlint-cli2 'src/**/*.{md,mdx}'
```

**EXPECT**: exit 0.

### Level 2: DRIFT + UNIT TESTS (no DB needed for the generators)

```bash
cd backend && cargo test --test gen_openapi --test gen_config_ref
```

```bash
cd backend && cargo test routes::health
```

**EXPECT**: all green; drift tests red if artifacts stale.

### Level 3: DOCS BUILD + FULL SUITE

```bash
cd docs && npm ci && npm run build
```

```bash
cd backend && cargo test
```

**EXPECT**: Starlight build succeeds; `Reference/Configuration` + `API Reference/Health` pages present; full backend suite green.

### Level 4: WORKFLOW LINT

```bash
actionlint .github/workflows/ci.yml .github/workflows/docs.yml
```

```bash
zizmor .github/workflows/ci.yml
```

**EXPECT**: exit 0.

### Level 5: REGEN-CLEAN

```bash
cd backend && REGEN=1 cargo test --test gen_openapi --test gen_config_ref && cd .. && git status --porcelain
```

**EXPECT**: no diff — committed artifacts already current.

### Level 6: REPO-LINT STACK (pre-push)

```bash
typos && shellcheck $(git ls-files '*.sh') ; hadolint Dockerfile ; yamllint .github/workflows/ci.yml .github/workflows/docs.yml
```

**EXPECT**: exit 0 (run the repo-lint job's tools locally before push — a stray typo cost a CI round-trip on PR #430).

---

## Acceptance Criteria

- [ ] Config reference rendered from `schemars::schema_for!(Config)` + `ENV_MAP`, committed, drift-gated by `gen_config_ref`.
- [ ] OpenAPI 3.1 spec generated for the `health` pilot, committed at `docs/openapi.json`, drift-gated; rendered by `starlight-openapi`.
- [ ] Removing a pilot handler's `#[utoipa::path]` fails compilation (coverage mechanism proven).
- [ ] `docs` job exists in `ci.yml`, gated on `changes.docs`, and is in `ci-gate.needs`; a broken docs build blocks merge; a skipped `docs` job does not.
- [ ] `docs.yml` no longer PR-builds (dedupe); still deploys Pages on main + tags.
- [ ] Reference section has narrative `index.md` + generated Configuration + API pages.
- [ ] Secrets documented by name/shape only (no values).
- [ ] No `chrono` feature pulled; `cargo audit` clean or justified.
- [ ] Phase-2 follow-up issue filed for full OpenAPI coverage + the raw-`.route(` grep-guard.
- [ ] PR body includes `Closes UNK-370`.

---

## Completion Checklist

- [ ] All tasks completed in dependency order.
- [ ] Each task validated immediately after completion.
- [ ] Level 1: fmt + clippy + markdownlint pass.
- [ ] Level 2: drift + health tests pass.
- [ ] Level 3: docs build + full backend suite pass.
- [ ] Level 4: actionlint + zizmor pass.
- [ ] Level 5: REGEN produces no diff.
- [ ] Level 6: repo-lint stack passes.
- [ ] All acceptance criteria met.

---

## Risks and Mitigations

| Risk                                                                       | Likelihood | Impact | Mitigation                                                                                                                                                                   |
| -------------------------------------------------------------------------- | ---------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `OpenApiRouter` state-generics friction with `AppState` at the merge point | MED        | MED    | Pilot is `health` (minimal state use); `split_for_parts()` before `.with_state()` returns `Router<AppState>`; shared `health_openapi_router()` keeps spec + runtime in sync. |
| schemars output shape changes across minor versions → render drift         | LOW        | MED    | `schemars` pinned at `1.2.1`; the `diff` gate at `ci.yml:251-257` already guards schema shape; renderer reads the same in-process schema.                                    |
| `starlight-openapi` local-schema path resolution                           | MED        | MED    | Path verified project-root-relative (`./openapi.json` → `docs/openapi.json`); Task 10 validates `astro build` before CI wiring; pin ≥0.25.2 (`required`+`anyOf` fix).        |
| Skipped `docs` job blocks `ci-gate` (conditional-job trap)                 | MED        | MED    | Mirror existing conditional-job result handling in the aggregator (memory: skipped conditional jobs block merge unless handled).                                             |
| Documented 503 `ProblemDetails` body differs from runtime (bare 503)       | LOW        | LOW    | Acceptable + noted for the pilot; phase 2 reconciles when real DTOs are annotated.                                                                                           |
| New deps flagged by `cargo audit`                                          | LOW        | LOW    | Run audit in Task 3; justify any ignore per `audit-ignores-are-last-resort`.                                                                                                 |

---

## Notes

- **Why schema-driven, not `syn` (the re-scope):** UNK-375 (PR #441) made config declarative
  (`figment` + `serde` + `validator` + `schemars`) and added a committed, CI-gated `config.schema.json`
  emitted by `print-config-schema`. The original `syn`-parse-the-prose generator is now both unsound
  (config split across 7 files) and redundant (the schema already carries description/type/default/range
  and is secret-safe). Rendering the reference from `schemars::schema_for!(Config)` + `ENV_MAP` is one
  gated source of truth, drift-free by construction, no `syn`.
- **Why 3.1 not 3.2:** OpenAPI 3.2.0 (Sept 2025) exists but no Rust library emits it and
  `starlight-openapi` doesn't render it. 3.2 is backward-compatible with 3.1; its delta doesn't touch
  Reverie's REST surface. Decision ratified 2026-06-08 (Linear UNK-370 corrected from "3.2" 2026-06-09).
- **API versioning (ADR reconciliation):** `adr/2026-06-08-api-versioning-openapi.md` mandates the JSON
  data API under `/api/v1/...` (URL-path major version) and OpenAPI 3.1 generated code-first. It
  **explicitly exempts** `/health`, `/readiness`, `/auth`, `/opds` as operational/standard-protocol
  paths that stay unversioned. The Phase-1 pilot is `/health` — so it is correct unversioned and the
  committed `docs/openapi.json` will carry unversioned pilot paths. The `/api/*`→`/api/v1/*` move lands
  with Phase 2 (real data-route annotation); `ApiDoc::info.version` here is the doc version, independent
  of the URL path version.
- **Why utoipa not aide:** aide forces `schemars` onto every DTO and an invasive `ApiRouter` return-type
  swap across all router modules; utoipa needs no schemars and integrates at the merge point only.
- **Why generators-as-tests not bins:** drift gating is free via the existing `backend` `cargo test`
  job — parallel to the sqlx `--check` and config-schema `--check` precedents. No production-binary or
  CLI surface added.
- **Phasing:** Phase 1 (mechanism slice). Phase 2 (full handler coverage + raw-`.route(` grep-guard
  ratchet) is a separate follow-up issue, executed module-by-module like the CSRF and comment-policy
  rollouts.
- **Security (hard rule 6):** config surface documented by name only (schema renders secret defaults as
  `""`/`null`, guarded by `config_schema_has_no_secret_default_values`); no secret values emitted; no new
  user-input path; generators are outbound-free; no response headers changed. Will stand up to review.

**Confidence: 8/10** for one-pass Phase-1 success. Residual unknowns isolated to Task 7
(`OpenApiRouter` ↔ `AppState` ergonomics, `split_for_parts` before `with_state`) and Task 10
(`starlight-openapi` local-schema path) — both gated by explicit validation before downstream tasks.
