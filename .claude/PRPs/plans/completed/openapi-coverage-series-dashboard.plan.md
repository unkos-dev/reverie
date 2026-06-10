# Feature: OpenAPI coverage for the `series` + `dashboard` modules (UNK-376 second coverage cluster)

## Summary

Migrate two more route modules onto the docs-as-done OpenAPI pattern blessed by the
`library` cluster (PR #449): `series` (1 route, `GET /api/v1/series/{id}`) and
`dashboard` (2 routes, `GET /api/v1/dashboard/stats` + `/activity`). Convert each from a
plain `axum::Router<AppState>` to a `utoipa_axum::OpenApiRouter<AppState>`, annotate the
handlers with `#[utoipa::path]`, derive `ToSchema` on the response DTOs and `IntoParams`
on the one query struct, merge both into `openapi::pilot_router()` (removing their
duplicate `lib.rs` mounts), and regenerate + commit `docs/openapi.json`. The novel
element this cluster introduces is **admin-gated** routes (`dashboard`): the UNK-380
security model has no admin dimension, so the admin gate is documented as a **`403`
response** + "Admin only" prose — **not** a new security scheme (that is out of epic
scope and surfaced as a ratify-at-approval decision).

## User Story

As a **Reverie maintainer / external contributor / API client author**
I want **the `series` and `dashboard` JSON endpoints documented in `docs/openapi.json`
with their params, response shapes, auth requirement, admin gate, and error envelope**
So that **the series-detail and admin-dashboard API contracts are machine-readable,
render on the docs site, and cannot silently drift from the handlers (compile-checked
by `routes!`).**

## Problem Statement

After the `library` cluster (#449), `docs/openapi.json` documents 6 paths (2 health + 4
library). The remaining ~20 data routes are still undocumented. `series` and `dashboard`
are the natural next cluster — book-adjacent, small (3 routes total), and `series` reuses
the `WorkManifestation` DTO that already carries `ToSchema` from #449, so the DTO surface
is mostly done. `dashboard` exercises a facet `library` did not: **admin-only** routes,
which forces an explicit decision on how authorization-beyond-authentication is reflected
in a spec whose security model only distinguishes authed-vs-public.

**Testable:** after this change, `reverie_api::openapi::spec_json()` contains the three
`/api/v1/series/{id}|dashboard/stats|dashboard/activity` paths, each with (a) the inherited
`session_cookie` requirement (no operation-level `security`), (b) a `200` body schema,
(c) the relevant error responses referencing `ProblemDetails` (`series` → `404`;
`dashboard` → `403`); the response DTO schema components are registered; and the committed
`docs/openapi.json` matches byte-for-byte (drift gate green) while the docs site resolves
every `$ref` (lychee green).

## Solution Statement

Identical mechanism to #449 (`OpenApiRouter::split_for_parts()` keeps served routes and
spec in lockstep from one registration; `health` + `library` already prove the seam). For
this cluster we:

1. Add `ToSchema` to the `series` response DTOs (`SeriesDetail`, `SeriesWork`) — the
   embedded `WorkManifestation` already has it from #449.
2. Add `ToSchema` to the `dashboard` response DTOs (`StatsResponse`, `FormatBucket`,
   `StatusCount`, `MetadataCoverage`, `ActivityResponse`, `BatchRow`).
3. Add `IntoParams` + `#[into_params(parameter_in = Query)]` to `ActivityParams`.
4. Annotate the three handlers with `#[utoipa::path]` — **no** `security(...)` (all inherit
   the document-level deny-by-default `session_cookie`). `series` documents `404`; both
   `dashboard` ops document `403` (the admin gate's only contract signal) with "Admin only"
   description prose.
5. Convert `series::router()` and `dashboard::router()` to return `OpenApiRouter<AppState>`
   via `routes!`.
6. Merge both into `openapi::pilot_router()`, add `series` + `dashboard` tags, and **remove
   the two `lib.rs` duplicate mounts** (else the paths register twice → Axum panic).
7. Regenerate and commit `docs/openapi.json` (hard rule 10); run the lychee docs gate.

## Metadata

| Field            | Value                                                                            |
| ---------------- | -------------------------------------------------------------------------------- |
| Type             | ENHANCEMENT (OpenAPI annotations) + small REFACTOR (router signature/wiring)     |
| Complexity       | LOW–MEDIUM (smaller + simpler than #449; the one new wrinkle is the admin `403`) |
| Systems Affected | backend `routes/series`, `routes/dashboard`, `models/series`, `openapi.rs`, `lib.rs`, `tests/gen_openapi.rs`, `docs/openapi.json` |
| Dependencies     | utoipa 5.5.0, utoipa-axum 0.2.0 (already in `Cargo.toml`; no new deps)            |
| Estimated Tasks  | 9                                                                                |
| Linear           | UNK-376 (v0.1.0 milestone) — **`Part of UNK-376`, NOT `Closes`** (epic Linear close policy) |
| Branch           | `feat/openapi-coverage-series-dashboard` (**MUST NOT contain `unk-376`** — see Notes) |

---

## UX Design

This is an API-contract change. "UX" is the generated spec + docs-site render. No runtime
request/response behaviour changes (same paths, same bodies, same auth, same admin gate).

### Before State

```text
  Handlers (series/mod.rs, dashboard/mod.rs)    docs/openapi.json
  ┌─────────────────────────────────────┐       ┌─────────────────────────────────────┐
  │ series::detail   (plain axum Router) │ ──✗─► │ paths: /health{,/ready},            │
  │ dashboard::stats (plain axum Router) │(absent│  /api/v1/books{,/{id}}, /works/{id}, │
  │ dashboard::activity                  │       │  /api/v1/search   ONLY              │
  └─────────────────────────────────────┘       └─────────────────────────────────────┘
  Served via lib.rs:121 .merge(series::router()) + lib.rs:120 .merge(dashboard::router())
  PAIN: series-detail + admin-dashboard API undocumented; the admin gate is invisible in
        the contract; drift undetectable.
```

### After State

```text
  Handlers + #[utoipa::path]                     docs/openapi.json
  ┌─────────────────────────────────────┐       ┌──────────────────────────────────────────┐
  │ series::detail   (OpenApiRouter)     │ ──►─► │ paths: …library…,                         │
  │ dashboard::stats (OpenApiRouter)     │routes!│  /api/v1/series/{id}      (GET, 200/401/404)│
  │ dashboard::activity                  │       │  /api/v1/dashboard/stats  (GET, 200/401/403)│
  │ each inherits session_cookie;        │       │  /api/v1/dashboard/activity (GET, 200/401/403)│
  │ dashboard ops document 403 (admin)   │       │ components: + SeriesDetail, SeriesWork,    │
  └─────────────────────────────────────┘       │  StatsResponse, ActivityResponse, …        │
  Served via openapi::pilot_router() →           └──────────────────────────────────────────┘
  openapi::router() at lib.rs:114
  (lib.rs:120-121 REMOVED — single registration)
  VALUE: series + admin-dashboard contract documented; admin gate visible as 403; compile-
         checked; renders on docs site.
```

### Interaction Changes

| Location                 | Before                              | After                                                | Impact                                 |
| ------------------------ | ----------------------------------- | ---------------------------------------------------- | -------------------------------------- |
| `docs/openapi.json`      | 6 paths (health + library)          | + 3 paths (series + 2 dashboard), + their DTO schemas | consumers get series + dashboard contract |
| Served routes            | series/dashboard via `lib.rs:120-121` | via `openapi::router()` (`lib.rs:114`)               | identical routing; CSP layer unchanged |
| Runtime request/response | unchanged                           | unchanged                                            | none — annotations are doc-only        |

---

## Mandatory Reading

| Priority | File                                   | Lines               | Why                                                                                       |
| -------- | -------------------------------------- | ------------------- | ----------------------------------------------------------------------------------------- |
| P0       | `backend/src/routes/library/mod.rs`    | 32-66, 92-147       | The **blessed worked example** (#449): `OpenApiRouter`+`routes!`, `IntoParams` with `parameter_in=Query`, `#[utoipa::path]` shape, `body = crate::openapi::ProblemDetails` |
| P0       | `backend/src/routes/health.rs`         | 21-69               | Minimal `OpenApiRouter` pattern; `security(())` is the PUBLIC opt-out — series/dashboard do NOT use it |
| P0       | `backend/src/openapi.rs`               | 92-160              | `ApiDoc` tags + `components(schemas(...))`; `pilot_router()` merge site; `ProblemDetails` |
| P0       | `backend/src/routes/series/mod.rs`     | 1-32, 42-46         | module doc, `router()`, `detail` signature (`Result<Json<SeriesDetail>, AppError>`, `Path<Uuid>`) |
| P0       | `backend/src/routes/dashboard/mod.rs`  | 1-47, 53-93, 233-262 | module THREAT doc, `router()`, the 6 private DTOs, `ActivityParams`, handler signatures   |
| P0       | `backend/src/models/series.rs`         | 27-60               | `SeriesDetail` + `SeriesWork` (both `#[non_exhaustive]`) — the DTOs to annotate           |
| P0       | `backend/src/lib.rs`                   | 111-134             | `api_like` block — REMOVE the `dashboard` (`:120`) + `series` (`:121`) merges             |
| P1       | `backend/tests/gen_openapi.rs`         | 68-206              | `requires_no_auth` helper + `spec_covers_library_routes` — mirror its shape for the new test |
| P1       | `backend/src/error/mod.rs`             | 51-83, 144-224      | `AppError` → status: `NotFound`=404, `Unauthorized`=401, `Forbidden`=403 (admin gate)      |
| P2       | `backend/src/models/library.rs`        | 263-284             | `WorkManifestation` — confirm it already derives `ToSchema` (it does, #449)               |
| P2       | `.claude/PRPs/plans/completed/unk-376-library-openapi-coverage.plan.md` | all | full prior worked plan + its report (`.claude/PRPs/reports/unk-376-library-openapi-coverage-report.md`) — the two discovery gotchas |

**External Documentation:**

| Source | Section | Why |
| ------ | ------- | --- |
| [utoipa 5.5 `attr.path`](https://docs.rs/utoipa/5.5.0/utoipa/attr.path.html) | responses, params | `body = T`, `params(("id" = Uuid, Path, …))`, multi-status `responses(...)` |
| [utoipa 5.5 `derive.IntoParams`](https://docs.rs/utoipa/5.5.0/utoipa/derive.IntoParams.html) | `parameter_in` | `#[into_params(parameter_in = Query)]` (the #449 trap) |
| [utoipa 5.5 `derive.ToSchema`](https://docs.rs/utoipa/5.5.0/utoipa/derive.ToSchema.html) | `value_type`, time/uuid features | `#[schema(value_type = String)]` fallback for `&'static str`; `OffsetDateTime`/`Uuid` covered by enabled features |

---

## Patterns to Mirror

**ROUTER (OpenApiRouter) — SOURCE `backend/src/routes/library/mod.rs:60-66`:**

```rust
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list))     // one .routes() per DISTINCT path
        .routes(routes!(detail))
}
```

**`IntoParams` query struct — SOURCE `backend/src/routes/library/mod.rs:92-94`** (the
explicit `parameter_in = Query` is the #449-paid-for trap — do NOT omit it):

```rust
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct ListParams { /* … */ }
```

**`#[utoipa::path]` with a `Path` param + `ProblemDetails` error — SOURCE
`backend/src/routes/library/mod.rs:135-147`** (note: library's `detail` is the closest
shape to `series::detail`; library used `body = crate::openapi::ProblemDetails`):

```rust
#[utoipa::path(
    get,
    path = "/api/v1/books",
    tag = "library",
    params(ListParams),
    responses(
        (status = 200, description = "Paginated list of visible books", body = BookListResponse, headers(…)),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, …)
    )
)]
async fn list(/* … */) -> Result<impl IntoResponse, AppError> { /* … */ }
```

**PUBLIC opt-out (do NOT copy onto these authed routes) — SOURCE `health.rs:34-37`:**

```rust
security(()),  // ← ONLY for public ops. series + dashboard OMIT this (they inherit session_cookie).
```

**MERGE into pilot_router + tag — SOURCE `backend/src/openapi.rs:107-110, 139-143`:**

```rust
tags(
    (name = "health", description = "Liveness and readiness probes."),
    (name = "library", description = "Books, works, and full-text search.")
    // ADD: (name = "series", …), (name = "dashboard", …)
)
// …
fn pilot_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(crate::routes::health::router())
        .merge(crate::routes::library::router())
        // ADD: .merge(crate::routes::series::router())
        // ADD: .merge(crate::routes::dashboard::router())
}
```

**DRIFT-TEST ASSERTION — SOURCE `backend/tests/gen_openapi.rs:134-206`** (`spec_covers_library_routes`):
mirror its structure (paths present; no op-level `security`; response schemas registered;
error responses declared) for a new `spec_covers_series_dashboard_routes`.

**INTEGRATION TEST HARNESS — SOURCE `backend/src/test_support.rs:372-387`**
(`server_with_real_pools` → `crate::build_router(state)`): both `series::tests` and
`dashboard::tests` build the **full composite app**, so the router-type move is transparent
to them — they are the move's safety net (run in CI).

---

## Files to Change

| File                                | Action | Justification                                                                                          |
| ----------------------------------- | ------ | ----------------------------------------------------------------------------------------------------- |
| `backend/tests/gen_openapi.rs`      | UPDATE | ADD `spec_covers_series_dashboard_routes` (TDD: write first, watch fail)                               |
| `backend/src/models/series.rs`      | UPDATE | add `utoipa::ToSchema` to `SeriesDetail`, `SeriesWork`                                                 |
| `backend/src/routes/dashboard/mod.rs` | UPDATE | `ToSchema` on 6 DTOs; `IntoParams` on `ActivityParams`; `#[utoipa::path]` ×2; `router()`→`OpenApiRouter` |
| `backend/src/routes/series/mod.rs`  | UPDATE | `#[utoipa::path]` on `detail`; `router()`→`OpenApiRouter`                                              |
| `backend/src/openapi.rs`            | UPDATE | merge `series`+`dashboard` routers into `pilot_router()`; add `series`+`dashboard` tags                |
| `backend/src/lib.rs`                | UPDATE | **REMOVE** `.merge(routes::dashboard::router())` (`:120`) + `.merge(routes::series::router())` (`:121`) — single registration |
| `docs/openapi.json`                 | UPDATE | regenerate via `REGEN=1 cargo test --test gen_openapi`; commit same PR (hard rule 10)                  |
| `debt/2026-06-10-inconsistent-query-error-envelope.md` | CREATE (done) | tracks the deferred problem+json envelope unification the no-`400` scope decision points to; commit in this branch |
| `debt/README.md`                    | UPDATE (done) | add the new entry to the Active list (newest-first)                                                   |

`backend/.sqlx/` cache: **no change** — no SQL added/modified (matches #449).
Frontend: **no change** — paths and response bodies are identical.
`backend/src/models/library.rs` (`WorkManifestation`): **no change** — already `ToSchema`.
Narrative docs (hard rule 10): **satisfied via `starlight-openapi` auto-generation** — the
plugin renders one page per operation plus a tag-overview page per non-empty tag description
(added in Task 7) directly from `docs/openapi.json`; #449 precedent shipped zero manual
`docs/src/` files. No `.mdx` stub needed this PR.

---

## NOT Building (Scope Limits)

- **No admin security scheme / scope.** The admin gate is documented via a `403` response +
  "Admin only" prose only. Inventing an `apiKey`/OAuth-scope admin scheme is a security-model
  change (out of UNK-380/UNK-376 epic scope) and needs its own decision — surfaced as an
  **open decision below**, not silently resolved here. (Per "no deviation without approval".)
- **No `400` on `dashboard/activity`.** Its handler uses a plain `Query<ActivityParams>`
  extractor (not the `Result<Query<…>, QueryRejection>` + `From<QueryRejection>` form that
  `library::list` uses — that form is reverie's only problem+json query-error path), so a
  malformed `?limit` yields axum's **default plaintext 400**, not a `ProblemDetails` envelope.
  Per **RFC 9457** (obsoletes 7807), problem+json is the standard error envelope; documenting
  `400` as `ProblemDetails` when the wire is plaintext would **misrepresent the contract** —
  worse than omission (accuracy over completeness). **Precedent:** #449 documented
  `library::search` (also a plain `Query<_>` extractor) with `200/401` and **no `400`** — so
  `activity` matches the blessed cluster, not a deviation. We document only what the handler
  emits through its own envelope (`200/401/403`). Unifying all input-error 400s onto problem+json
  (switching `activity`/`search`/OPDS to the Result-wrapped form — the Zalando-MUST-226 / RFC 9457
  consistency ideal) is a runtime behaviour change → tracked as
  `debt/2026-06-10-inconsistent-query-error-envelope.md` (created with this plan), out of scope for a
  doc-only PR.
- **No readiness `ProblemDetails`→503 reconcile, no `.route(` grep-guard (UNK-379), no OPDS
  decision** — those are sequenced to later cluster PRs / strictly last per the umbrella plan.
- **No other modules** (`shelves`, `settings`, `users`, `metadata`, `enrichment`, `ingestion`,
  `tokens`, `auth`, `opds`) — later clusters.
- **No `ToResponse` / reusable response objects, no `API_V1_BASE` constant, no Swagger UI** —
  matches the `health`/`library` pattern; no premature abstraction.

---

## Step-by-Step Tasks

Execute in order. Compile after each — `routes!`, `IntoParams`, `ToSchema`, and
`#[utoipa::path]` errors are caught at `cargo check`.

### Task 1 (TDD): ADD failing spec-coverage assertion test

- **ACTION**: ADD `spec_covers_series_dashboard_routes` to `backend/tests/gen_openapi.rs`,
  mirroring `spec_covers_library_routes` (`:134-206`). Do **NOT** call the existing
  `requires_no_auth` helper — it asserts the PUBLIC opt-out shape (`security: [{}]`), the
  opposite of what these authed routes need. Instead assert `.get("security").is_none()`
  (absent key = inherit), mirroring `spec_covers_library_routes` at `gen_openapi.rs:154`.
- **IMPLEMENT**: parse `reverie_api::openapi::spec_json()` as `serde_json::Value`; assert:
  - `paths` contains `/api/v1/series/{id}`, `/api/v1/dashboard/stats`, `/api/v1/dashboard/activity`.
  - none of the three `get` operations carries an operation-level `security` key (they inherit
    the document default — deny-by-default; only public ops opt out).
  - `components.schemas` contains all eight DTOs: `SeriesDetail`, `SeriesWork`,
    `StatsResponse`, `FormatBucket`, `StatusCount`, `MetadataCoverage`, `ActivityResponse`,
    `BatchRow` — including the nested-only `StatusCount`/`MetadataCoverage`, proving
    `routes!` auto-collected the transitive DTO graph (the red state must pin all six
    dashboard derives from Task 3, not just the top-level ones).
  - `GET /api/v1/series/{id}` declares a `404` response.
  - **`GET /api/v1/dashboard/stats` AND `/activity` each declare a `403` response** (the admin
    gate's contract signal — the novel assertion this cluster adds vs #449's 404-only).
- **GOTCHA**: this MUST fail now (only health + library are covered) — the TDD red state.
- **VALIDATE** (from `backend/`): `SQLX_OFFLINE=true cargo test --test gen_openapi spec_covers_series_dashboard_routes`
  → FAILS (expected). (No `-p` flag — package name is `reverie-api`, hyphenated; all
  validation commands run from `backend/` like Levels 1–2.)

### Task 2: ADD `ToSchema` to the `series` response DTOs

- **ACTION**: add `utoipa::ToSchema` to the `#[derive(...)]` of `SeriesDetail` and `SeriesWork`
  in `backend/src/models/series.rs`.
- **GOTCHA**: both are `#[non_exhaustive]` — utoipa 5.5's `ToSchema` reads named fields and
  ignores `#[non_exhaustive]` (confirmed working on the library DTOs in #449). If it errors,
  do NOT strip `#[non_exhaustive]` (deliberate API-stability attribute) — report. The embedded
  `WorkManifestation` already derives `ToSchema` (#449); `Uuid`/`OffsetDateTime`/`f64`/`String`
  field types are covered by the enabled utoipa `uuid`/`time` features.
- **VALIDATE**: `SQLX_OFFLINE=true cargo check --tests` (from `backend/`)

### Task 3: ADD `ToSchema` to the `dashboard` response DTOs

- **ACTION**: add `utoipa::ToSchema` to the `#[derive(serde::Serialize)]` of `StatsResponse`,
  `FormatBucket`, `StatusCount`, `MetadataCoverage`, `ActivityResponse`, `BatchRow` in
  `backend/src/routes/dashboard/mod.rs`.
- **GOTCHA (verify-at-compile)**: `StatusCount.status` is `&'static str`. utoipa is expected to
  map a string reference to a `string` schema; **if the derive rejects the borrowed lifetime**,
  add `#[schema(value_type = String)]` on that field (documented fallback — does not change the
  wire). `BatchRow` carries `time::OffsetDateTime` fields — `started_at` with
  `#[serde(with = "time::serde::rfc3339")]`, `ended_at` with the `rfc3339::option` variant;
  utoipa reads the field *type* (→ `string`/`date-time` via the `time` feature) and is unaffected
  by either serde-`with` adapter — no action needed there.
- **NOTE**: dashboard flattens its enums to strings server-side (`ValidationStatus::…as_str()` →
  `StatusCount.status`), so `ValidationStatus`/`EnrichmentStatus` are NOT response types here —
  no enum `ToSchema` work for dashboard (unlike #449's embedded-enum task).
- **VALIDATE**: `SQLX_OFFLINE=true cargo check --tests` (from `backend/`)

### Task 4: ADD `IntoParams` to `ActivityParams`

- **ACTION**: add `utoipa::IntoParams` to the `#[derive(serde::Deserialize)]` of `ActivityParams`
  (`dashboard/mod.rs:236-239`) and `#[into_params(parameter_in = Query)]`. Add a field-level
  `///` doc on `limit` for the OpenAPI parameter description (plain prose, e.g.
  `/// Max recent batches to return; clamped to 1..=100, default 20.` — keep the existing
  intra-doc `[`DEFAULT_ACTIVITY_LIMIT`]` reference on the struct doc, which is NOT folded into
  the spec by `IntoParams` and is `crate::`-style anyway).
- **GOTCHA**: `parameter_in = Query` is mandatory even for a plain `Query<T>` extractor — the
  #449 trap defaulted to `Path`. `limit: Option<i64>` is a primitive → no `$ref`, so the dangling-
  `$ref` trap (#449 #2) does NOT apply this cluster.
- **VALIDATE**: `SQLX_OFFLINE=true cargo check --tests` (from `backend/`)

### Task 5: ADD `#[utoipa::path]` to the three handlers

- **ACTION**: annotate each handler. **No `security(...)`** (all inherit `session_cookie`).
- **IMPLEMENT** (`series::detail`, `series/mod.rs`):
  ```rust
  #[utoipa::path(
      get,
      path = "/api/v1/series/{id}",
      tag = "series",
      params(("id" = Uuid, Path, description = "Series id")),
      responses(
          (status = 200, description = "Series identity + ordered works with visible manifestations", body = SeriesDetail),
          (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
          (status = 404, description = "Series not found, or no work has a visible manifestation (existence-not-leaked)", body = crate::openapi::ProblemDetails)
      )
  )]
  ```
  (`detail` returns `Result<axum::Json<SeriesDetail>, AppError>` — `body = SeriesDetail` is correct.)
- **IMPLEMENT** (`dashboard::stats`, `dashboard/mod.rs`):
  ```rust
  #[utoipa::path(
      get,
      path = "/api/v1/dashboard/stats",
      tag = "dashboard",
      responses(
          (status = 200, description = "Library-wide aggregate health metrics. Admin only.", body = StatsResponse),
          (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
          (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails)
      )
  )]
  ```
  (Keep the existing `#[allow(clippy::too_many_lines, …)]` — `#[utoipa::path]` goes above or below
  it; match formatter output.)
- **IMPLEMENT** (`dashboard::activity`, `dashboard/mod.rs`):
  ```rust
  #[utoipa::path(
      get,
      path = "/api/v1/dashboard/activity",
      tag = "dashboard",
      params(ActivityParams),
      responses(
          (status = 200, description = "Most-recent ingestion batches, newest first. Admin only.", body = ActivityResponse),
          (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
          (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails)
      )
  )]
  ```
- **GOTCHA**: handlers are private `async fn` — `routes!(detail)` etc. work on private fns intra-module
  (matches `library`). Path-param brace syntax `{id}` matches the Axum route string `/api/v1/series/{id}`.
  `stats`/`activity` return `impl IntoResponse`, so the explicit `body = …` is required (utoipa can't
  infer it).
- **VALIDATE**: `SQLX_OFFLINE=true cargo check --tests` (from `backend/`) (the macro validates the
  referenced types impl `ToSchema`/`IntoParams`).

### Task 6: CONVERT both `router()`s to `OpenApiRouter`

- **ACTION (series/mod.rs)**: change `pub fn router() -> Router<AppState>` to
  `-> OpenApiRouter<AppState>`; body `OpenApiRouter::new().routes(routes!(detail))`. Add
  `use utoipa_axum::router::OpenApiRouter; use utoipa_axum::routes;`. Drop now-unused
  `use axum::Router;` and `use axum::routing::get;` (keep `axum::extract::{Path, State}` and
  `axum::Json` — still used).
- **ACTION (dashboard/mod.rs)**: same conversion; body
  `OpenApiRouter::new().routes(routes!(stats)).routes(routes!(activity))` (two DISTINCT paths →
  two `.routes()`). Add the two `utoipa_axum` imports. From `use axum::{Json, Router};` keep `Json`
  (used in handlers), drop `Router`; drop `use axum::routing::get;`. Keep `use axum::extract::{Query, State}`.
- **BLAST RADIUS (pre-verified)**: the only callers of `series::router()`/`dashboard::router()` are
  `lib.rs:120-121` (removed in Task 7) + the new `openapi.rs` merges. No test calls `router()`
  directly — both test modules use `server_with_real_pools` → `build_router`. Any OTHER compiler
  error from the return-type change is a surprise to resolve before proceeding.
- **VALIDATE**: `SQLX_OFFLINE=true cargo check --tests` (from `backend/`)

### Task 7: WIRE into `pilot_router()` + tags; REMOVE the `lib.rs` duplicate mounts

- **ACTION (openapi.rs)**: in `pilot_router()` add, after the `library` merge:
  `.merge(crate::routes::series::router())` and `.merge(crate::routes::dashboard::router())`.
  In `ApiDoc`'s `tags(...)` add `(name = "series", description = "Series and their ordered works.")`
  and `(name = "dashboard", description = "Admin-only library-health aggregates.")`.
- **ACTION (lib.rs)**: DELETE both `.merge(routes::dashboard::router())` (`:120`) and
  `.merge(routes::series::router())` (`:121`) from the `api_like` block. **Critical** — leaving
  either causes its path(s) to register twice → Axum router build panic at startup / in the
  integration tests.
- **GOTCHA**: no `components(schemas(...))` addition needed this cluster — no type is referenced
  *only* via an `IntoParams` field (the #449 `SortMode` trap), so there is no dangling `$ref`.
  All response DTOs are auto-collected by `routes!`. The `api_csp_layer` already wraps the whole
  `api_like` block (incl. `openapi::router()` at `lib.rs:114`), so series/dashboard CSP
  (`default-src 'none'`) is preserved automatically — do NOT move any CSP layer.
- **VALIDATE**: `SQLX_OFFLINE=true cargo check --tests` (from `backend/`)

### Task 8: VALIDATE no double-serve + existing tests compile (the move's safety net)

- **ACTION**: confirm the crate compiles cleanly with `--tests` (the integration tests in
  `series::tests` + `dashboard::tests` hit the moved paths through the full app and are the
  regression guard; they run authoritatively in CI — `#[sqlx::test]` needs a provisioning DB
  unreachable locally per `backend/CLAUDE.md`).
- **GOTCHA**: `server_with_real_pools` builds via `build_router` (pre-verified at
  `test_support.rs:385`), so no `.split_for_parts().0` shim is needed.
- **VALIDATE**: `SQLX_OFFLINE=true cargo check --tests` (from `backend/`) clean. (DB-backed run is
  CI: `PoolTimedOut` locally = unreachable DB, not a regression.)

### Task 9: REGENERATE + commit `docs/openapi.json`; full local gate

- **ACTION**: regenerate the artifact, then confirm the drift test (incl. Task 1's assertion) is
  green, and run the lychee docs gate (the #449 round-trip cause).
- **VALIDATE**:
  - `cd backend && REGEN=1 cargo test --test gen_openapi` (writes `docs/openapi.json`)
  - `git diff docs/openapi.json` — sanity check: 3 new paths; series + dashboard schemas; no
    `security` block on any of the three ops (inherited); `403` on both dashboard ops; `404` on
    series detail
  - `SQLX_OFFLINE=true cargo test --test gen_openapi` (no REGEN) → GREEN incl. `spec_covers_series_dashboard_routes`
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --locked -- -D warnings`
  - **lychee docs gate locally** (per memory `reference_utoipa_openapi_coverage_gotchas`):
    ```bash
    cd docs && npm run build
    ```
    ```bash
    rm -rf /tmp/linkroot && mkdir -p /tmp/linkroot && ln -sfn $PWD/dist /tmp/linkroot/reverie
    ```
    ```bash
    lychee --offline --no-progress --root-dir /tmp/linkroot /tmp/linkroot/reverie
    ```
  - commit `docs/openapi.json` in the same PR (hard rule 10)
  - after code changes: `graphify update .`

---

## Testing Strategy

### Tests to Write (new)

| Test                                          | Validates                                                                                                  |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `gen_openapi::spec_covers_series_dashboard_routes` | 3 paths present; all inherit security (no op-level key); series declares `404`; both dashboard ops declare `403`; all eight series + dashboard DTO schemas registered (incl. nested `StatusCount`/`MetadataCoverage`) |

### Tests that Guard the Move (must stay green, unchanged — CI)

- All `routes::series::tests::*` and `routes::dashboard::tests::*` — the router signature/wiring
  change must not alter served behaviour; same paths/status/bodies prove the move is internal-only.
- `gen_openapi::openapi_spec_matches_committed_artifact` — green after REGEN.
- `gen_openapi::{spec_declares_security_model, spec_covers_library_routes}` — unaffected; still green.

### Edge Cases Checklist

- [ ] All three ops have **no** `security` in the spec (inherit `session_cookie`)
- [ ] `series/{id}` documents `404`; both `dashboard` ops document `403` (admin gate)
- [ ] `dashboard` ops are NOT documented as public, and NOT given an invented admin scheme
- [ ] `dashboard/activity` documents its `?limit` query param as `in: query`, `required: false`
- [ ] all eight response DTO schemas registered (`SeriesDetail`/`SeriesWork`/`StatsResponse`/`FormatBucket`/`StatusCount`/`MetadataCoverage`/`ActivityResponse`/`BatchRow`)
- [ ] no path served twice (crate compiles; integration tests pass in CI)
- [ ] `docs/openapi.json` committed; drift test green with no further regen; lychee resolves every `$ref`

---

## Validation Commands

### Level 1 — STATIC (local bar)

```bash
cd backend && cargo fmt --all -- --check
```

```bash
cd backend && cargo clippy --workspace --all-targets --locked -- -D warnings
```

```bash
cd backend && SQLX_OFFLINE=true cargo check --tests
```

### Level 2 — DRIFT GATE (regen once, then green clean)

```bash
cd backend && REGEN=1 cargo test --test gen_openapi
```

```bash
cd backend && cargo test --test gen_openapi
```

EXPECT: green; `git status` shows only `docs/openapi.json` modified.

### Level 3 — DOCS BUILD + lychee (the #449 round-trip cause — run locally)

```bash
cd docs && npm run build
```

```bash
rm -rf /tmp/linkroot && mkdir -p /tmp/linkroot && ln -sfn $PWD/dist /tmp/linkroot/reverie
```

```bash
lychee --offline --no-progress --root-dir /tmp/linkroot /tmp/linkroot/reverie
```

EXPECT: Starlight renders 3 new operation pages AND 2 new tag-overview pages (`series`,
`dashboard` — generated because both tag descriptions are non-empty); lychee resolves all
`$ref`s on both page types.

### Level 4 — INTEGRATION (CI-authoritative)

`#[sqlx::test]` suite runs in CI (postgres:18 service). Local `PoolTimedOut` = unreachable
provisioning DB, not a regression (`backend/CLAUDE.md` Dev Database).

### Level 5 — REPO-LINT

```bash
typos
```

(No shell/yaml/Dockerfile/markdown changes expected → `typos` is the relevant one.)

---

## Acceptance Criteria

- [ ] Three routes documented in `docs/openapi.json`: `series/{id}` (200/401/404),
      `dashboard/stats` + `dashboard/activity` (200/401/403)
- [ ] All three inherit `session_cookie` (no per-op `security`); none documented as public; no
      invented admin scheme
- [ ] `series::router()` + `dashboard::router()` return `OpenApiRouter`; merged once via
      `pilot_router()`; the two `lib.rs` duplicate mounts removed; app builds, each path served once
- [ ] `dashboard/activity` `?limit` documented `in: query`, `required: false`
- [ ] TDD: `spec_covers_series_dashboard_routes` written first (red), green after implementation
- [ ] Levels 1–3 + 5 green locally; `docs/openapi.json` committed in the same PR
- [ ] No regression in existing `series`/`dashboard` tests (CI)
- [ ] Branch name omits `unk-376`; PR body `Part of UNK-376` (NOT `Closes`)

---

## Risks and Mitigations

| Risk                                                              | Likelihood | Impact | Mitigation                                                                                      |
| ----------------------------------------------------------------- | ---------- | ------ | ----------------------------------------------------------------------------------------------- |
| Double-serve panic (left `lib.rs:120` or `:121` in)               | MED        | HIGH   | Task 7 deletes BOTH; Task 8 compile + CI integration tests fail loudly if either remains        |
| `&'static str` `ToSchema` derive rejects the borrow              | LOW        | LOW    | Task 3 verify-at-compile; documented `#[schema(value_type = String)]` fallback                  |
| `IntoParams` renders `?limit` as `in: path` (the #449 trap)       | MED        | LOW    | Task 4 `#[into_params(parameter_in = Query)]`; Task 1 test + `git diff` review catch it          |
| Admin gate read as a doc omission by a reviewer                   | MED        | LOW    | `403` + "Admin only" prose + PR-body note explain the in-scope treatment (no scheme); see Notes |
| Spec drags in an unexpected schema                                | LOW        | LOW    | Task 9 `git diff docs/openapi.json` review; assertion test pins the key shape                   |
| Branch/title accidentally carries `unk-376` → premature epic close | LOW       | HIGH   | Branch `feat/openapi-coverage-series-dashboard`; PR title without the id; body `Part of`        |

---

## Security (hard rule 6)

Touches `routes/series` + `routes/dashboard` (the latter a **Tier-2 THREAT-annotated**,
admin-gated surface) and the `openapi.rs` security model. Answer before done: **does it stand
up to security review?**

- **Auth contract:** all three ops are `CurrentUser`-gated at runtime and inherit the
  document-level deny-by-default `session_cookie` requirement in the spec — none opts out with
  `security(())`, so the spec never misdocuments an authed route as public (OWASP fail-safe;
  UNK-380). Pinned by the new test's no-op-level-security assertion.
- **Admin authorization is documented, not modelled away:** `dashboard` is `require_admin()`-gated.
  The UNK-380 scheme set (`session_cookie` apiKey + `opds_basic`) has no admin dimension, and the
  correct, in-scope treatment is to document the gate as a **`403` response** (+ "Admin only"
  prose) — the only contract signal available without a model change. Inventing an admin scheme
  is deferred as an open decision (below). Runtime `require_admin()` is unchanged; this PR adds no
  authorization surface.
- **No new runtime surface:** annotations are doc-only; `split_for_parts().0` is the same served
  router. RLS (`db::acquire_with_rls`), `api_csp_layer`, and the `ProblemDetails` envelope are
  unchanged. The dashboard THREAT doc (privilege-escalation / info-disclosure, clamped `?limit`)
  remains accurate.
- **No info leak in the spec:** documented error bodies reference the existing `ProblemDetails`
  shape; dashboard responses are aggregate counts/byte totals (no per-user rows, paths, or PII) —
  the spec exposes nothing the API does not already return.
- Consult `.claude/security/codeguard-0-authorization-access-control.md` (admin gate / `403`
  semantics) and `.claude/security/codeguard-0-http-headers.md` (response headers — CSP layer
  must stay put) during implementation.

---

## Open Decision — RATIFIED at approval gate 2026-06-10

**Should the admin gate get a first-class spec representation, or stay documented as `403`?**
This plan takes the in-scope path: document `403` + "Admin only" prose, **no** new security
scheme. Standards grounding (verified, not from memory):

- **OpenAPI 3.1** expresses authorization first-class *only* via OAuth2/OpenID Connect **scopes**.
  For `apiKey`/`http` schemes the OpenAPI Initiative's own guidance (learn.openapis.org) is that the
  security-requirement array is **empty** — role/permission requirements are not expressible. (The
  raw OAS 3.1 spec text permits a role-name array for non-OAuth2 schemes, but it is explicitly "not
  otherwise defined or exchanged in-band," non-enforced, ignored by tooling, and using it would
  override the document-level `session_cookie` default and break the UNK-380 deny-by-default
  inheritance signal.) Reverie uses a cookie `apiKey` scheme, not OAuth2 — so scopes are not available.
- **RFC 9110 §15.5.4**: `403 Forbidden` = request understood but authorization refused, and
  re-authenticating will not help — the HTTP-semantics-correct status for an admin-role denial
  (vs `401` = authentication required).
- **OWASP API Security Top 10 2023, API5:2023 Broken Function Level Authorization**: admin endpoints
  are the canonical BFLA case. The control is server-side enforcement (reverie's `require_admin()`,
  unchanged); the spec's role is to surface the requirement — a documented `403` does exactly that.

A first-class representation (an OAuth2 scope, or an `x-`-prefixed vendor extension marking admin-only
ops) is a security-model change beyond UNK-380, would touch every future admin route, and warrants its
own ADR — analogous to the OPDS migrate-vs-allowlist open decision in the umbrella plan.

**Direction (user, 2026-06-10):** the maintainer is keen to work *toward* an OAuth2/OIDC API security
model. Reverie already uses OIDC for *login*, but the API surface is reached via the `session_cookie`
(`apiKey`) scheme; representing the API as an OIDC-protected resource server with **scopes** is the
standards-blessed path that would make the admin gate first-class (an `admin` scope), and is the future
home for this open decision. That migration (cookie-`apiKey` → OAuth2/OIDC scheme + scopes) is a
substantial architectural change deserving its own ADR + epic — **not** this doc-only coverage PR.

**RATIFIED (user, 2026-06-10): ship `403`-as-signal now (standards-correct and the only first-class
option for the current cookie-`apiKey` scheme); treat it as the explicit *interim*, with scoped API
auth as the chartered destination — tracked as **UNK-382** (`feat(auth): evolve API security model —
scoped tokens, with OIDC resource-server as first-class optional mode`, Backlog; supersedes the `403`
interim once delivered). Post-sanity-check refinement recorded on the ticket: primary candidate =
first-party scoped tokens (evolved device tokens); OIDC resource-server (Authentik et al.) =
first-class *optional* deployment mode, never required; the chartering ADR weighs both, no
presupposed outcome.**

---

## Notes

- **Branch/title/body hygiene (epic close policy):** branch `feat/openapi-coverage-series-dashboard`
  (NO `unk-376` — the branch id auto-closed UNK-376 on #449), PR title without the epic id, PR body
  `Part of UNK-376` (never `Closes`). Only the final coverage PR closes UNK-376. Per
  `feedback_epic_pr1_of_n_no_bare_ref` — all three sync surfaces (title, body close-form, branch)
  must omit the id for an interim PR.
- **Why this cluster second:** smallest book-adjacent surface (3 routes), `series` reuses the
  already-`ToSchema` `WorkManifestation`, and it introduces exactly one new wrinkle (admin `403`)
  worth blessing before the larger batches — matching the #449 report's named "Next" cluster.
- **`#[sqlx::test]` is CI-only here** (provisioning DB unreachable from the workspace post-Incus
  cutover); local bar = check/clippy/fmt + `gen_openapi` drift + the lychee docs gate.
- **Two #449 traps that do NOT bite this cluster** (stated so the implementer doesn't hunt for
  them): #2 dangling `$ref` (no param-only custom type — `?limit` is `i64`); embedded-enum
  `ToSchema` (dashboard flattens enums to `&str` server-side; series's only embedded DTO already
  has `ToSchema`). The #449 trap that DOES bite: `IntoParams` `parameter_in = Query` (Task 4).
