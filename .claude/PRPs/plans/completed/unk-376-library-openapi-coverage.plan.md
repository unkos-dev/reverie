# Feature: OpenAPI coverage for the `library` module (UNK-376 first coverage cluster)

## Summary

Migrate the `library` route module from a plain `axum::Router<AppState>` to a
`utoipa_axum::OpenApiRouter<AppState>` so its four data routes
(`GET /api/v1/books`, `/api/v1/books/{id}`, `/api/v1/works/{id}`, `/api/v1/search`)
contribute to the generated `docs/openapi.json`. Mirrors the `health` pilot
(`backend/src/routes/health.rs`) — the only module on the pattern today. Annotates
each handler with `#[utoipa::path]`, derives `ToSchema` on the real response DTOs and
`IntoParams` on the query structs, and folds `library` into `openapi::pilot_router()`
while removing its separate `lib.rs` mount so it is served exactly once. This is the
**first** coverage cluster of the UNK-376 epic — deliberately scoped to one module to
establish and bless the full pattern stack before later PRs batch 3–4 modules each.

## User Story

As a **Reverie maintainer / external contributor / API client author**
I want **the `library` JSON endpoints documented in `docs/openapi.json` with their
request params, response shapes, auth requirement, and error envelope**
So that **the book/work/search API contract is machine-readable, renders on the docs
site, and cannot silently drift from the handlers (compile-checked by `routes!`).**

## Problem Statement

After PR1 (`/api/v1` mount move, #444) and UNK-380 (security model, #446),
`docs/openapi.json` still documents only the two `health` probes. The 24 data routes
are undocumented. The `library` module — the richest data surface (list with cursor
pagination + `Link` header, detail, work detail, full-text search, the `Book*` DTOs) —
is the natural first module to migrate: it exercises every facet of the pattern
(`IntoParams` query structs, `ToSchema` DTOs with a `#[serde(skip)]` field and embedded
enums, multi-status error responses, a documented response header), so blessing it here
de-risks the mechanical batches that follow.

**Testable:** after this change, `reverie_api::openapi::spec_json()` contains the four
`/api/v1/books*|works|search` paths, each with (a) the `session_cookie` security
requirement (inherited, not opted out), (b) its query parameters, (c) a `200` body
schema, and (d) the relevant `4xx` responses referencing `ProblemDetails`; and the
committed `docs/openapi.json` matches byte-for-byte (drift gate green).

## Solution Statement

`utoipa-axum`'s `OpenApiRouter::split_for_parts()` keeps served routes and the spec in
lockstep from one registration. `health` already proves the seam. We:

1. Add `ToSchema` to the `library` response DTOs (+ embedded enums + the private/`pub`
   response wrappers + `SortMode`), with `#[schema(ignore)]` on the `#[serde(skip)]`
   `created_at` so schema and wire format agree.
2. Add `IntoParams` to `ListParams` and `SearchParams`.
3. Annotate the four handlers with `#[utoipa::path]` — **no** `security(...)` (they
   inherit the document-level deny-by-default `session_cookie`; only public ops opt out).
4. Convert `library::router()` to return `OpenApiRouter<AppState>` via `routes!`.
5. Merge `library` into `openapi::pilot_router()`, add a `library` tag to `ApiDoc`, and
   **remove the separate `.merge(routes::library::router())` in `lib.rs`** (else the
   paths register twice → Axum panic).
6. Regenerate and commit `docs/openapi.json` (hard rule 10).

## Metadata

| Field            | Value                                                                        |
| ---------------- | ---------------------------------------------------------------------------- |
| Type             | ENHANCEMENT (OpenAPI annotations) + small REFACTOR (router signature/wiring) |
| Complexity       | MEDIUM                                                                       |
| Systems Affected | backend `routes/library`, `models/library`, `routes/cursor`, `openapi.rs`, `lib.rs`, `tests/gen_openapi.rs`, `docs/openapi.json` |
| Dependencies     | utoipa 5.5.0 (`axum_extras`,`time`,`uuid`), utoipa-axum 0.2.0 (already in `Cargo.toml`) |
| Estimated Tasks  | 9                                                                            |
| Linear           | UNK-376 (v0.1.0 milestone) — **`Part of UNK-376`, NOT `Closes`** (see Linear close policy) |
| Branch           | `feat/unk-376-library-openapi-coverage`                                      |

---

## UX Design

This is an API-contract change; "UX" is the generated spec + docs-site render. No
runtime request/response behaviour changes (same paths, same bodies, same auth).

### Before State

```text
  Handlers (library/mod.rs, search.rs)        docs/openapi.json
  ┌───────────────────────────────┐           ┌──────────────────────────┐
  │ list / detail / work_detail    │  ──✗──►   │ paths: /health, /health/ │
  │ search   (plain axum Router)   │  (absent) │        ready  ONLY        │
  └───────────────────────────────┘           └──────────────────────────┘
  Served via lib.rs:120 .merge(routes::library::router())  (separate mount)
  PAIN: book/work/search API undocumented; contract drift invisible.
```

### After State

```text
  Handlers + #[utoipa::path]                   docs/openapi.json
  ┌───────────────────────────────┐           ┌──────────────────────────────────┐
  │ list / detail / work_detail    │  ──►──►   │ paths: /health{,/ready},          │
  │ search  (OpenApiRouter)        │  routes!  │  /api/v1/books        (GET, list)  │
  │ each: params + responses +     │           │  /api/v1/books/{id}   (GET)        │
  │       inherited session_cookie │           │  /api/v1/works/{id}   (GET)        │
  └───────────────────────────────┘           │  /api/v1/search       (GET)        │
  Served via openapi::pilot_router() →         │ components: ProblemDetails, Book*, │
  openapi::router() at lib.rs:114              │  WorkDetail, SearchResponse, …     │
  (lib.rs:120 REMOVED — single registration)   └──────────────────────────────────┘
  VALUE: full book/work/search contract; compile-checked; renders on docs site.
```

### Interaction Changes

| Location                 | Before                      | After                                           | Impact                                  |
| ------------------------ | --------------------------- | ----------------------------------------------- | --------------------------------------- |
| `docs/openapi.json`      | health-only                 | + 4 library paths, library DTO components        | API consumers get a real contract       |
| Served routes            | library via `lib.rs:120`    | library via `openapi::router()` (`lib.rs:114`)   | identical routing; CSP layer unchanged  |
| Runtime request/response | unchanged                   | unchanged                                        | none — annotations are doc-only         |

---

## Mandatory Reading

| Priority | File                                    | Lines        | Why                                                                                  |
| -------- | --------------------------------------- | ------------ | ------------------------------------------------------------------------------------ |
| P0       | `backend/src/routes/health.rs`          | 1-69         | The pattern to MIRROR exactly (`OpenApiRouter`+`routes!`+`#[utoipa::path]`)           |
| P0       | `backend/src/openapi.rs`                | 30-157       | `SecurityAddon` (deny-by-default contract), `ApiDoc` tags, `pilot_router`, `spec_json`|
| P0       | `backend/src/lib.rs`                    | 111-135      | `api_like` block: merge sites + `api_csp_layer`. Remove the `library` merge (line ~120)|
| P0       | `backend/src/routes/library/mod.rs`     | 54-135, 109-113, 263-272 | `router()`, `list` sig, `BookListResponse` wrapper, `Link` header, `ListParams` |
| P0       | `backend/src/models/library.rs`         | 32-284       | All DTOs to annotate (derives + `#[non_exhaustive]` + `#[serde(skip)]created_at`)     |
| P1       | `backend/src/routes/library/search.rs`  | 81-104       | `SearchParams`, `search` handler signature, `SearchResponse`                          |
| P1       | `backend/tests/gen_openapi.rs`          | 1-42         | drift test + `REGEN=1` regen path; the new assertion test mirrors UNK-380's           |
| P1       | `backend/src/routes/cursor.rs`          | (`SortMode`) | `SortMode` enum — needs `ToSchema` (or `#[param(value_type=String)]` on `sort`)       |
| P1       | `backend/src/error/mod.rs`              | 139-248      | `AppError`→`application/problem+json`; which statuses each route emits                 |
| P2       | `backend/src/routes/library/tests.rs`   | 22-43, 103-144, 494-542 | integration harness (`server_with_real_pools`) — confirm it builds the full router |
| P2       | `backend/src/models/{enrichment_status,ingestion_status,validation_status}.rs` | derives | embedded enums needing `ToSchema` |

**External Documentation:**

| Source | Section | Why |
| ------ | ------- | --- |
| [utoipa 5.5 `attr.path`](https://docs.rs/utoipa/5.5.0/utoipa/attr.path.html) | responses, params, headers | response `headers((..))`, `body = T`, `params(StructName)` syntax |
| [utoipa 5.5 `derive.IntoParams`](https://docs.rs/utoipa/5.5.0/utoipa/derive.IntoParams.html) | field params | `#[derive(IntoParams)]`; doc-comments → descriptions; `#[param(value_type=…)]` |
| [utoipa 5.5 `derive.ToSchema`](https://docs.rs/utoipa/5.5.0/utoipa/derive.ToSchema.html) | `#[schema(ignore)]` | mark `created_at` ignored to match `#[serde(skip)]` |
| [utoipa-axum 0.2 `OpenApiRouter`](https://docs.rs/utoipa-axum/0.2.0/utoipa_axum/router/struct.OpenApiRouter.html) | `routes!`, `split_for_parts` | router wiring; schema auto-collection from `routes!` |

---

## Patterns to Mirror

**ROUTER (OpenApiRouter) — SOURCE `backend/src/routes/health.rs:21-27`:**

```rust
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(health))   // one .routes() per DISTINCT path
        .routes(routes!(ready))    // routes!(a, b) is for multiple METHODS on one path
}
```

**PUBLIC OP OPT-OUT — SOURCE `backend/src/routes/health.rs:30-41`** (library does NOT
use this — shown so the implementer does not copy `security(())` onto authed routes):

```rust
#[utoipa::path(
    get, path = "/health", tag = "health",
    security(()),  // ← opt OUT of session_cookie. AUTHED library routes OMIT this line.
    responses((status = 200, description = "Process is live", body = String, content_type = "text/plain"))
)]
```

**DTO SCHEMA — SOURCE `backend/src/openapi.rs:112-126`** (the only `ToSchema` today):

```rust
#[derive(utoipa::ToSchema)]
pub struct ProblemDetails {
    #[schema(example = "https://reverie.example/probs/not-found")]
    pub r#type: String,
    // …
}
```

**MERGE INTO pilot_router — SOURCE `backend/src/openapi.rs:132-134`:**

```rust
fn pilot_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(crate::routes::health::router())
        // ADD: .merge(crate::routes::library::router())
}
```

**DRIFT-TEST ASSERTION — SOURCE `backend/tests/gen_openapi.rs:22-41`** + UNK-380's
`spec_declares_security_model` (PR #446) — mirror its shape for a new
`spec_covers_library_routes` test that parses `spec_json()` and asserts the paths exist.

**INTEGRATION TEST HARNESS — SOURCE `backend/src/routes/library/tests.rs:103-144`:**
existing tests hit `/api/v1/books` via `server_with_real_pools(&app_pool, &ingestion_pool)`
and assert status + body. They are the safety net for the router move — they must keep
passing unchanged (paths and bodies are identical).

---

## Files to Change

| File                                              | Action | Justification                                                                                      |
| ------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------- |
| `backend/src/models/library.rs`                   | UPDATE | add `utoipa::ToSchema` to every response DTO; `#[schema(ignore)]` on `BookListRow::created_at`      |
| `backend/src/models/enrichment_status.rs`         | UPDATE | add `ToSchema` (embedded in `BookListRow`/`BookDetail`)                                              |
| `backend/src/models/ingestion_status.rs`          | UPDATE | add `ToSchema` (embedded)                                                                           |
| `backend/src/models/validation_status.rs`         | UPDATE | add `ToSchema` (embedded)                                                                           |
| `backend/src/routes/cursor.rs`                    | UPDATE | add `ToSchema` to `SortMode` (used by `ListParams.sort`) — or use `#[param(value_type=String)]`     |
| `backend/src/routes/library/mod.rs`               | UPDATE | `router()`→`OpenApiRouter`+`routes!`; `#[utoipa::path]` on `list`/`detail`/`work_detail`; `IntoParams` on `ListParams`; `ToSchema` on `BookListResponse` |
| `backend/src/routes/library/search.rs`            | UPDATE | `#[utoipa::path]` on `search`; `IntoParams` on `SearchParams` (`SearchResponse` gets `ToSchema` in models) |
| `backend/src/openapi.rs`                          | UPDATE | merge `library::router()` into `pilot_router()`; add `library` tag to `ApiDoc`                      |
| `backend/src/lib.rs`                              | UPDATE | **REMOVE** the `.merge(routes::library::router())` line (~120) — single registration                |
| `backend/tests/gen_openapi.rs`                    | UPDATE | add `spec_covers_library_routes` assertion test (TDD: write first, watch fail)                      |
| `docs/openapi.json`                               | UPDATE | regenerate via `REGEN=1 cargo test --test gen_openapi`; commit in same PR (hard rule 10)            |

`backend/.sqlx/` cache: **no change expected** — no SQL queries are added or modified.
Frontend: **no change** — paths and response bodies are identical.

---

## NOT Building (Scope Limits)

- **The api-mounted cover endpoint `/api/v1/books/{id}/cover*`** — it lives in
  `backend/src/routes/opds/covers.rs::api_router()` (a different module, serves image
  bytes not JSON, wired separately via `routes::opds::covers_router()` at `lib.rs:128`).
  Pulling it in would drag an `opds/covers` refactor into a "library module" PR and
  blur scope. It folds into a later batch / the OPDS coverage decision PR. Deferred.
- **`ProblemDetails` doc-only → runtime DTO reconcile + DB-unreachable→503 readiness
  regression test** — per Linear UNK-376 these "fold into the cluster PR that touches
  readiness" (the `health`/`ready` module), NOT this PR. We *reference* the existing
  `ProblemDetails` component in error responses; we do not change its doc-only status.
- **`.route(` grep-guard + allowlist** — that is UNK-379, hard-sequenced LAST (it would
  false-positive on the ~11 not-yet-migrated modules).
- **Other modules** (`users`, `shelves`, `series`, `settings`, `tokens`, `metadata`,
  `dashboard`, `enrichment`, `ingestion`, `auth`, `opds`) — later cluster PRs.
- **`ToResponse` derive / reusable response objects** — `body = T` + inline `responses`
  is sufficient and matches the `health` pilot; no premature abstraction.
- **No central `API_V1_BASE` constant, no spec-serving HTTP endpoint, no Swagger UI** —
  out of epic scope per UNK-376 "Out of scope".

---

## Step-by-Step Tasks

Execute in order. Compile after each — `routes!` and `IntoParams`/`ToSchema` errors are
caught at `cargo check`.

### Task 1 (TDD): ADD failing spec-coverage assertion test

- **ACTION**: ADD `spec_covers_library_routes` to `backend/tests/gen_openapi.rs`,
  mirroring UNK-380's `spec_declares_security_model` shape.
- **IMPLEMENT**: parse `reverie_api::openapi::spec_json()` as `serde_json::Value`; assert
  `paths` contains `/api/v1/books`, `/api/v1/books/{id}`, `/api/v1/works/{id}`,
  `/api/v1/search`; assert the `GET /api/v1/books` operation has NO operation-level
  `security` key (so it inherits the document default) AND that `components.schemas`
  contains `BookListResponse` (or the chosen wrapper name) + `BookDetail`; assert the
  `GET /api/v1/books/{id}` operation declares a `404` response. **Also guard the two
  silently-breakable edge cases here (not just in the manual checklist):** (a) the
  `BookListRow` schema's `properties` does **not** contain `created_at`; (b) the
  `GET /api/v1/books` `200` response declares a `Link` header.
- **GOTCHA**: this MUST fail now (only health is covered) — that is the TDD red state.
- **VALIDATE**: `cargo test -p reverie_api --test gen_openapi spec_covers_library_routes`
  → FAILS (expected). Do not proceed expecting green.

### Task 2: ADD `ToSchema` to embedded enums + `SortMode`

- **ACTION**: add `utoipa::ToSchema` to the `#[derive(...)]` of `EnrichmentStatus`,
  `IngestionStatus`, `ValidationStatus` (in `backend/src/models/*_status.rs`) and
  `SortMode` (in `backend/src/routes/cursor.rs`).
- **GOTCHA**: keep existing derives/attrs intact; `ToSchema` honours `#[serde(rename_all)]`
  so the documented enum variants match the wire casing. Do NOT add `#[non_exhaustive]`
  to `ValidationStatus` (its absence is deliberate — see its doc comment).
- **ALT for `SortMode`**: if adding `ToSchema` to `cursor.rs` is undesirable, instead put
  `#[param(value_type = String, example = "title")]` on `ListParams.sort` in Task 4. Prefer
  the `ToSchema` derive (documents the real enum); fall back only if it pulls in surprises.
- **VALIDATE**: `cargo check -p reverie_api`

### Task 3: ADD `ToSchema` to `library` response DTOs

- **ACTION**: add `utoipa::ToSchema` to the `#[derive(...)]` of `SeriesRef`, `BookListRow`,
  `BookDetail`, `MetadataVersionRow`, `MetadataVersionSummary`, `WorkDetail`,
  `SearchResponse`, `SearchHit`, `SearchHitKind`, `WorkManifestation` in
  `backend/src/models/library.rs`.
- **IMPLEMENT**: on `BookListRow::created_at` (currently `#[serde(skip)]`) add
  `#[schema(ignore)]` so the schema omits it exactly as the wire does.
- **GOTCHA (research item — verify empirically)**: these structs are `#[non_exhaustive]`.
  utoipa 5.5's `ToSchema` derive reads named fields and is expected to ignore
  `#[non_exhaustive]` — confirm at compile. If it errors, the fallback is to keep
  `#[non_exhaustive]` and report (do not silently strip it — it is a deliberate API-stability
  attribute). `time`/`uuid` utoipa features (already enabled) supply `ToSchema` for
  `OffsetDateTime`/`Uuid` — no manual schema needed for those field types.
- **VALIDATE**: `cargo check -p reverie_api`

### Task 4: ADD `IntoParams` to query structs

- **ACTION**: add `utoipa::IntoParams` to `ListParams` (`library/mod.rs:88`) and
  `SearchParams` (`library/search.rs:81`). Add `///` doc-comments to each field (they
  become the OpenAPI parameter descriptions) where not already present.
- **GOTCHA**: `ListParams.sort: SortMode` requires `SortMode: ToSchema` (Task 2) — or the
  `#[param(value_type = String)]` fallback. `tag: Vec<String>` documents as a repeatable
  query param (`axum_extras` feature handles the `axum_extra::Query` form). `ListParams`
  is a private struct — `IntoParams` + `params(ListParams)` in the same module is fine.
- **VALIDATE**: `cargo check -p reverie_api`

### Task 5: ADD `ToSchema` to the `BookListResponse` wrapper + annotate handlers

- **ACTION**: add `#[derive(utoipa::ToSchema)]` to `BookListResponse` (private, `mod.rs:109`).
  Then add `#[utoipa::path]` to `list`, `detail`, `work_detail` (`mod.rs`) and `search`
  (`search.rs`).
- **IMPLEMENT** (no `security(...)` — authed routes inherit `session_cookie`; add a
  `library` tag string to each):
  - `list`: `get, path = "/api/v1/books", tag = "library", params(ListParams),
    responses((status = 200, description = "Paginated book list", body = BookListResponse,
    headers(("Link" = String, description = "RFC 8288 next-page link; rel=\"next\" when more rows remain"))),
    // NOTE: Context7 returned two header forms — `headers((..))` and `headers = [..]`.
    // The exact utoipa-5.5 form is VERIFY-AT-`cargo check`; do not assume this bracket shape.
    (status = 400, description = "Malformed query parameter", body = ProblemDetails),
    (status = 401, description = "Authentication required", body = ProblemDetails),
    (status = 422, description = "Invalid cursor / too many tag filters", body = ProblemDetails))`
  - `detail`: `get, path = "/api/v1/books/{id}", tag = "library",
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses((status = 200, body = BookDetail), (status = 401, body = ProblemDetails),
    (status = 404, description = "Not found or RLS-hidden", body = ProblemDetails))`
  - `work_detail`: same shape as `detail` but `path = "/api/v1/works/{id}"`, `body = WorkDetail`,
    statuses `200/401/404`.
  - `search`: `get, path = "/api/v1/search", tag = "library", params(SearchParams),
    responses((status = 200, body = SearchResponse), (status = 401, body = ProblemDetails))`
- **GOTCHA**: handlers are private `async fn` — `routes!(list)` in the same module's
  `router()` works on private fns (matches how `health` exposes `pub`, but private is fine
  intra-module). Path-param names in `#[utoipa::path(path=...)]` use `{id}` to match Axum's
  `{id}` capture (utoipa 5 uses brace syntax — consistent with `health` having none and the
  `/api/v1/books/{id}` route string).
- **VALIDATE**: `cargo check -p reverie_api` (the `#[utoipa::path]` macro validates that the
  referenced types impl `ToSchema`/`IntoParams`).

### Task 6: CONVERT `library::router()` to `OpenApiRouter`

- **ACTION**: change `pub fn router() -> Router<AppState>` to
  `pub fn router() -> OpenApiRouter<AppState>` and rebuild it with `routes!` (one
  `.routes(routes!(handler))` per distinct path), mirroring `health::router()`. Drop the
  now-unused `use axum::Router;` / `use axum::routing::get;` if they become unused; add
  `use utoipa_axum::router::OpenApiRouter;` + `use utoipa_axum::routes;`.
- **GOTCHA**: `search::search` is `pub(super)` in a submodule — reference it as `search::search`
  inside `routes!` exactly as the current `.route(... get(search::search))` does.
- **BLAST RADIUS**: before changing the return type, grep every caller of
  `routes::library::router()` (`rg -n 'library::router\(\)' backend/src` — or sdl symbol
  usages) — the `Router → OpenApiRouter` type change breaks any direct caller (test harness,
  alternate `build_router` paths). The compiler flags them, but know the set up front: expect
  `lib.rs:120` (removed in Task 7) and `openapi.rs` (added in Task 7); any OTHER hit is a
  surprise to resolve before proceeding.
- **VALIDATE**: `cargo check -p reverie_api`

### Task 7: WIRE into `pilot_router()` + add tag; REMOVE the `lib.rs` duplicate mount

- **ACTION (openapi.rs)**: in `pilot_router()` add `.merge(crate::routes::library::router())`
  after the `health` merge. In `ApiDoc`'s `tags(...)` add
  `(name = "library", description = "Books, works, and search.")`.
- **ACTION (lib.rs)**: DELETE the `.merge(routes::library::router())` line in the `api_like`
  block (~line 120). This is the critical step — leaving it causes the four paths to register
  twice (once via `openapi::router()`, once here) → Axum router build panic.
- **GOTCHA**: the `api_csp_layer` is applied to the whole `api_like` block (`lib.rs:132-135`),
  and `openapi::router()` is already inside that block (`lib.rs:114`), so library's CSP
  (`default-src 'none'`) is preserved automatically. Do NOT move any CSP layer.
- **VALIDATE**: `cargo check -p reverie_api`; then run the existing library integration tests
  (Task 8) which exercise the served routes through the full app.

### Task 8: VALIDATE no double-serve + existing tests green (the move's safety net)

- **ACTION**: run the existing `library` integration tests — they hit `/api/v1/books*` via
  the full app and are the regression guard for the router move (paths/bodies unchanged).
- **GOTCHA**: confirm `server_with_real_pools` builds the composite app via `build_router`
  (so library now flows through `openapi::router()`); if it mounts `library::router()`
  directly it would need a `.split_for_parts().0` shim — verify and adjust only if needed.
- **VALIDATE**: `cargo test -p reverie_api routes::library` (needs dev DB —
  `docker compose up -d`; seed roles per `backend/CLAUDE.md` Coder caveat).

### Task 9: REGENERATE + commit `docs/openapi.json`; full gate

- **ACTION**: regenerate the artifact, then confirm the drift test (incl. Task 1's new
  assertion) is green.
- **VALIDATE**:
  - `cd backend && REGEN=1 cargo test --test gen_openapi` (writes `docs/openapi.json`)
  - `git diff docs/openapi.json` — sanity-check: 4 new paths, library schemas, no
    `security` block on the library ops (inherited), `created_at` ABSENT from `BookListRow`
  - `cargo test --test gen_openapi` (no REGEN) → GREEN; `spec_covers_library_routes` passes
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --locked -- -D warnings`
  - `cargo test -p reverie_api`
  - commit `docs/openapi.json` in the same PR (hard rule 10)

---

## Testing Strategy

### Tests to Write (new)

| Test                                       | Validates                                                              |
| ------------------------------------------ | ---------------------------------------------------------------------- |
| `gen_openapi::spec_covers_library_routes`  | 4 library paths present; `list` inherits security (no op-level key); `books/{id}` declares 404; library DTO schemas registered |

### Tests that Guard the Move (must stay green, unchanged)

- All `routes::library::tests::*` — the router signature/wiring change must not alter
  served behaviour; these hitting `/api/v1/books*` with the same status/body assertions are
  the proof the move is internal-only.
- `gen_openapi::openapi_spec_matches_committed_artifact` — green after REGEN.
- `gen_openapi::spec_declares_security_model` (UNK-380) — unaffected; still green.

### Edge Cases Checklist

- [ ] `GET /api/v1/books` operation has **no** `security` in the spec (inherits `session_cookie`)
- [ ] `BookListRow` schema does **not** include `created_at` (`#[schema(ignore)]` parity)
- [ ] `list` `200` response documents the `Link` header
- [ ] `books/{id}` + `works/{id}` document `404`; `list` documents `400`+`422`
- [ ] enum schemas (`validation_status`,`ingestion_status`,`enrichment_status`) use correct casing
- [ ] no path is served twice (app builds; integration tests pass)
- [ ] `docs/openapi.json` committed and drift test green with no further regen

## Validation Commands

### Level 1 — STATIC

```bash
cd backend && cargo fmt --all -- --check
```

```bash
cd backend && cargo clippy --workspace --all-targets --locked -- -D warnings
```

### Level 2 — UNIT / INTEGRATION (dev DB up)

```bash
cd backend && cargo test -p reverie_api
```

### Level 3 — DRIFT GATE (regen once, then must be green clean)

```bash
cd backend && REGEN=1 cargo test --test gen_openapi
```

```bash
cd backend && cargo test --test gen_openapi
```

EXPECT: green; `git status` shows `docs/openapi.json` modified (staged in this PR), nothing
else regenerated.

### Level 4 — DOCS BUILD (Starlight renders the spec)

```bash
cd docs && npm run build
```

EXPECT: `starlight-openapi` renders the 4 new library operations without error.

### Level 5 — REPO-LINT (per memory `feedback_run_repo_lint_stack_locally`)

```bash
typos
```

(No shell/yaml/Dockerfile/markdown changes expected → `typos` is the relevant one.)

## Acceptance Criteria

- [ ] Four `library` routes documented in `docs/openapi.json` with params, `200` body,
      relevant `4xx` (`ProblemDetails`) responses, and the `list` `Link` header
- [ ] Library ops inherit `session_cookie` (no per-op `security`); none documented as public
- [ ] `library::router()` returns `OpenApiRouter`; merged once via `pilot_router()`; the
      `lib.rs` duplicate mount removed; app builds and serves each path exactly once
- [ ] `created_at` stays off the wire AND off the schema
- [ ] TDD: `spec_covers_library_routes` written first (red), green after implementation
- [ ] Levels 1–4 green; `docs/openapi.json` committed in the same PR
- [ ] No regression in existing `routes::library` tests
- [ ] PR body: `Part of UNK-376` (NOT `Closes`)

## Risks and Mitigations

| Risk                                                                  | Likelihood | Impact | Mitigation                                                                                      |
| --------------------------------------------------------------------- | ---------- | ------ | ----------------------------------------------------------------------------------------------- |
| Double-serve panic (left `lib.rs:120` in)                             | MED        | HIGH   | Task 7 explicit delete + Task 8 build/integration tests; the existing tests fail loudly if so   |
| `ToSchema` rejects `#[non_exhaustive]` in utoipa 5.5                  | LOW        | MED    | Task 3 verify-at-compile; fallback documented; do NOT strip the attribute silently              |
| `ListParams.sort: SortMode` not `ToSchema` → `IntoParams` compile err | MED        | LOW    | Task 2 adds `ToSchema` to `SortMode`; documented `#[param(value_type=String)]` fallback         |
| Spec changes more than expected (some DTO drags in extra schemas)     | LOW        | LOW    | Task 9 `git diff docs/openapi.json` review before commit; assertion test pins the key shape     |
| Integration harness mounts `library::router()` directly (not full app)| LOW       | MED    | Task 8 verifies harness path; shim with `.split_for_parts().0` only if needed                   |

## Security (hard rule 6)

Touches `routes/library` (user-facing data) and the `openapi.rs` security model surface,
so answer before done: **does it stand up to security review?**

- **Auth contract:** every library op is `CurrentUser`-gated at runtime and inherits the
  document-level `session_cookie` requirement in the spec (deny-by-default — UNK-380). No
  library op opts out with `security(())`; the spec therefore never misdocuments an authed
  route as public. This is the OWASP fail-safe-defaults posture the `SecurityAddon` docstring
  describes. Confirmed by the `spec_covers_library_routes` assertion (no op-level `security`).
- **No new runtime surface:** annotations are doc-only; `split_for_parts().0` is the same
  served router. RLS (`db::acquire_with_rls`), CSP (`api_csp_layer`), and the `ProblemDetails`
  error envelope are all unchanged.
- **No info leak in the spec:** documented error bodies reference the existing `ProblemDetails`
  shape (type/title/status/detail/instance) — no internal detail beyond what the API already
  returns. `created_at` (internal cursor key) is `#[schema(ignore)]`, so it is not exposed.
- Consult `.claude/security/` (response-headers / input-handling files) during implementation.

## Notes

- **Why `library` first:** richest module — exercises `IntoParams`, `ToSchema` with a
  `#[serde(skip)]` field + embedded enums, a documented response header, and multi-status
  error responses. Blessing the full stack here de-risks the mechanical 3–4-module batches
  that follow (the CSP-rollout / comment-policy-rollout phased precedent Linear cites).
- **Security-inheritance decision:** UNK-380's text says handlers "attach per-op `security`."
  Under deny-by-default, *inheriting* the document default **is** the attachment for authed
  routes — restating `security(("session_cookie"=[]))` on every op would be redundant noise and
  weaken the signal that `security(())` carries (explicit public). We therefore annotate
  security ONLY to opt public ops out. Document this in the PR body so reviewers don't read the
  absence as an omission.
- **Next clusters (not this PR):** `series`+`dashboard` (small, book-adjacent) is the natural
  PR3; then `shelves`/`settings`/`users`; then `metadata`/`enrichment`/`ingestion`; `tokens`;
  the `opds` migrate-vs-allowlist decision (needs user input per umbrella plan); readiness
  `ProblemDetails` reconcile folds into whichever cluster touches `health`/`ready`; UNK-379
  grep-guard is strictly last.
- After code changes this session, run `graphify update .` to keep the graph current.
```