# Feature: docs-as-done Phase 2 — full OpenAPI coverage + `/api/v1` move + grep-guard (UNK-376)

## Summary

Phase 2 of docs-as-done (UNK-370 shipped Phase 1: the generate→commit→CI-`--check`
pipeline proven on the `health` pilot). This epic ratchets the OpenAPI half to full
route coverage, completes the ADR-mandated `/api`→`/api/v1` mount move, defines an
explicit OpenAPI security model, and finally enables a `.route(` grep-guard so new
endpoints cannot ship undocumented.

**This is a multi-PR epic.** This plan is written at two resolutions:

- **PR1 (mount move) is planned deep** — fully executable, one-pass-ready, below.
- **PR2..N (per-module coverage + security model) and PR-final (grep-guard) are a
  sequenced roadmap** with the full route/DTO inventory gathered during exploration.
  Each gets its own `prp-plan` pass when reached — they depend on PR1 having landed
  and on per-module detail that cannot be pinned down now without inventing it.

Rationale for the split: the prp-plan one-pass / NO_PRIOR_KNOWLEDGE gates cannot
honestly be met for PR2..N today; a 6-PR monolith pretending otherwise is a worse
artifact than the Linear issue. CLAUDE.md's "one plan per feature/PR" convention
backs planning each PR at execution time.

## User Story

As a **Reverie maintainer / external contributor / API client author**
I want **every data endpoint documented in `docs/openapi.json` and served under a
versioned `/api/v1` prefix, with coverage enforced in CI**
So that **the API contract can never silently drift from the code, breaking changes
get a clean version boundary, and no new endpoint ships undocumented**.

## Problem Statement

Today only the two `health` pilot endpoints appear in `docs/openapi.json`. The 24
data routes under `/api/*` are undocumented, unversioned, and there is no mechanism
preventing a new `.route(...)` from shipping without an OpenAPI annotation. The
`adr/2026-06-08-api-versioning-openapi.md` decision (data API under `/api/v1`,
URL-path major version) is unimplemented.

**Testable (PR1 slice):** after the mount move, (a) `GET /api/v1/books` returns 200
for an authed request; (b) the _old_ `GET /api/books` returns a JSON `404` Problem
(not SPA HTML), proving the route moved rather than being additively duplicated;
(c) the frontend reaches the API at its new prefix and its test suite is green.

## Solution Statement

Mechanism is already in place from Phase 1 (`backend/src/openapi.rs::pilot_router`
merges module `OpenApiRouter`s; `split_for_parts()` keeps served routes and the
generated spec identical; `tests/gen_openapi.rs` drift-gates `docs/openapi.json`).
Phase 2 sequences:

1. **PR1 — `/api`→`/api/v1` mount move.** Pure path rename across the 13 backend
   route modules + frontend client + tests. No utoipa work. `/health`, `/auth`,
   `/opds` stay unversioned (ADR-exempt). Done first so the later `#[utoipa::path]`
   annotations are written once at their final `/api/v1/...` paths.
2. **PR2..N — per-module coverage + security model.** Migrate each module to
   `OpenApiRouter` + `routes!` + `#[utoipa::path]`, add `#[derive(ToSchema)]` to
   response DTOs, define `securitySchemes` + per-operation `security` once and attach
   incrementally.
3. **PR-final — `.route(` grep-guard + allowlist**, plus the readiness-503
   `ProblemDetails` reconcile + negative test.

## Metadata

| Field            | Value                                                                    |
| ---------------- | ------------------------------------------------------------------------ |
| Type             | ENHANCEMENT (PR1 = REFACTOR/mechanical move)                             |
| Complexity       | MEDIUM (PR1 mechanical-but-wide); HIGH (epic)                            |
| Systems Affected | backend routes, frontend api client, security/headers fallback, CI, docs |
| Dependencies     | utoipa 5.5, utoipa-axum 0.2, tower-sessions 0.15 (PR2..N only)           |
| Estimated Tasks  | PR1: 10 tasks. Epic: ~6 PRs.                                             |
| Linear           | UNK-376 (v0.1.0 milestone), follows UNK-370                              |

---

# EPIC ROADMAP (shallow — each PR gets its own prp-plan when reached)

| PR           | Scope                                                                                                     | Branch                           | Depends on       |
| ------------ | --------------------------------------------------------------------------------------------------------- | -------------------------------- | ---------------- |
| **PR1**      | `/api`→`/api/v1` mount move (backend + frontend + tests + fallback test)                                  | `feat/unk-376-api-v1-mount-move` | —                |
| PR2          | securitySchemes + health `security:[]` + first coverage batch (`library`, `series`, `dashboard`)          | tbd                              | PR1              |
| PR3          | coverage batch (`shelves`, `settings`, `users`)                                                           | tbd                              | PR2              |
| PR4          | coverage batch (`metadata`, `enrichment`, `ingestion`)                                                    | tbd                              | PR2              |
| PR5          | coverage batch (`tokens`, `auth`)                                                                         | tbd                              | PR2              |
| PR6          | `opds` coverage **or** allowlist (open decision — see below)                                              | tbd                              | PR2              |
| **PR-final** | `.route(` grep-guard + allowlist; readiness-503 `ProblemDetails` reconcile + negative DB-unreachable test | tbd                              | all coverage PRs |

Batching is by DTO cohesion and route count, not fixed — adjust at planning time.

**Linear close policy (resolves the hard-rule-9 tension for a multi-PR epic):** PR1
through the last coverage PR carry **no** `Closes` line — they reference the epic as
`Part of UNK-376`. Only **PR-final** carries `Closes UNK-376`, which is the single
auto-close point. This is permitted (per CLAUDE.md: "a branch/PR does not require a
UNK issue") and avoids minting throwaway sub-issues just to satisfy the magic-word on
each intermediate PR. Trade-off: UNK-376 stays in-progress across the epic with no
per-PR Linear tracking — acceptable because GitHub PRs are the completed-work record.
**Watch item:** if PR-final's body omits `Closes UNK-376`, the epic sticks in Backlog
— it is the only PR that closes it.

## Route + DTO inventory (carried forward to PR2..N planning)

24 `/api/*` data routes across 13 modules. `/health` (2), `/auth/*` (5), `/opds/*`
(~20) stay unversioned. Source: `backend/src/routes/`.

| Module     | File:line                    | Routes                                     | Notes for migration                                                                 |
| ---------- | ---------------------------- | ------------------------------------------ | ----------------------------------------------------------------------------------- |
| library    | `routes/library/mod.rs:56`   | 4                                          | DTOs `BookListRow`, `BookDetail` (`models/library.rs`); internal `BookListResponse` |
| users      | `routes/users/mod.rs:47`     | 4                                          | DTO `User` (`models/user.rs`); internal `UserResponse`; admin-gated                 |
| shelves    | `routes/shelves/mod.rs:51`   | 6                                          | chained verbs → `routes!(a,b)`; DTO `Shelf`                                         |
| series     | `routes/series/mod.rs:30`    | 1                                          | DTO `SeriesDetail`                                                                  |
| settings   | `routes/settings/mod.rs:27`  | 1 path/2 methods                           | chained verbs; internal `SettingsResponse`/`PutSettingsResponse`                    |
| auth       | `routes/auth.rs:27`          | 5 (`/auth/*`, **unversioned**)             | login/callback public; others `CurrentUser`                                         |
| tokens     | `routes/tokens.rs:21`        | 3 (2 paths, split same-path calls → merge) | DTO in module                                                                       |
| metadata   | `routes/metadata.rs:42`      | 8                                          | `require_not_child()` on several                                                    |
| dashboard  | `routes/dashboard/mod.rs:43` | 2                                          | admin-gated                                                                         |
| enrichment | `routes/enrichment.rs:39`    | 3                                          | `require_not_child()`                                                               |
| ingestion  | `routes/ingestion.rs:22`     | 1                                          | admin-gated                                                                         |
| opds       | `routes/opds/mod.rs:28`      | ~20 (Atom XML)                             | `Option<Router>`; `BasicOnly`; open decision below                                  |

DTO convention today: `#[derive(Debug, Clone, Serialize)] #[non_exhaustive]`, none
carry `ToSchema`. Sole existing `ToSchema` = doc-only `ProblemDetails` (`openapi.rs:49`).
Four response DTOs live in route modules, not `models/`.

## Open decisions for PR2..N (require user input at those planning steps — do NOT silently resolve)

- **OPDS: migrate to `routes!` vs allowlist.** Issue scope item 1 lists `opds` among
  modules to migrate; item 3's grep-guard needs _everything_ on `routes!` OR a
  justified allowlist entry. OPDS serves standard Atom XML, not the JSON API surface —
  utoipa-annotating XML handlers is awkward, and a standard-protocol module is a
  legitimate allowlist candidate. Both defensible. Surface to user; do not quietly
  descope. (Per "no deviation without approval".)
- **Session cookie name for the `apiKey in cookie` scheme.** `lib.rs:94` sets no
  `.with_name(...)`; tower-sessions 0.15 default is `id`. **Verify empirically**
  (observe `Set-Cookie` on `/auth/login`) before hardcoding the name into the spec —
  do not bake it from memory.
- **securitySchemes introduction forces a health-pilot edit in the same PR.** Once
  global `securitySchemes` exist, every operation inherits global `security` unless
  overridden — including `/health`. The PR that introduces schemes MUST also add
  `security: []` to the health pilot, or Checkov CKV_OPENAPI_4/5 flips dirty on the
  pilot. Sequence: schemes + health `security:[]` + first data module together.
- **Research item (PR2..N):** utoipa 5.5 / utoipa-axum 0.2 `SecurityScheme` +
  `Modify`/`SecurityAddon` API, and `ToSchema` on `#[non_exhaustive]` structs.

---

# PR1 (DEEP) — `/api` → `/api/v1` mount move

## UX Design

### Before State

```text
Browser SPA / API client / OPDS reader
        │
        ▼
  same-origin fetch
        │
   ┌────┴───────────────────────────────────────────┐
   │  axum composite router (lib.rs:111-131, flat)   │
   │   /health, /health/ready   (no auth)            │
   │   /auth/*                  (public + CurrentUser)│
   │   /api/*   ── 24 data routes (CurrentUser)       │  ◄── UNVERSIONED
   │   /opds/*  ── Atom XML       (BasicOnly)         │
   │  fallback: is_reserved_prefix? → JSON404 : SPA  │
   └─────────────────────────────────────────────────┘

  Frontend: every src/api/*.ts call hardcodes "/api/..." literal.
  cover_url generated server-side as format!("/api/books/{}/cover/thumb").
  PAIN: no version boundary; a future breaking change has nowhere to live.
```

### After State

```text
Browser SPA / API client
        │
        ▼
  same-origin fetch
        │
   ┌────┴───────────────────────────────────────────┐
   │  axum composite router (flat, unchanged shape)  │
   │   /health, /health/ready   (UNVERSIONED — exempt)│
   │   /auth/*                  (UNVERSIONED — exempt)│
   │   /api/v1/*  ── 24 data routes (CurrentUser)     │  ◄── VERSIONED
   │   /opds/*    ── Atom XML       (UNVERSIONED — exempt)│
   │  fallback: is_reserved_prefix("/api/v1/x")=true  │
   │            (strip_prefix "/api" already matches) │
   │     → old /api/books now → JSON 404 (moved)      │
   └─────────────────────────────────────────────────┘

  Frontend: every src/api/*.ts call now hits "/api/v1/...".
  cover_url generated as format!("/api/v1/books/{}/cover/thumb").
  The /opds cover variant stays /opds/books/{id}/cover (exempt).
  VALUE: clean major-version boundary per ADR; spec annotations (PR2..N)
  written once at final paths.
```

### Interaction Changes

| Location                | Before                        | After                            | User Impact                                   |
| ----------------------- | ----------------------------- | -------------------------------- | --------------------------------------------- |
| Data API base           | `/api/*`                      | `/api/v1/*`                      | API clients update base path; SPA transparent |
| Old `/api/books`        | 200 JSON                      | 404 JSON Problem                 | gone-endpoint signals correctly, not SPA HTML |
| Cover URLs in responses | `/api/books/{id}/cover/thumb` | `/api/v1/books/{id}/cover/thumb` | covers still resolve via the moved api mount  |
| OPDS covers             | `/opds/books/{id}/cover`      | unchanged                        | OPDS readers unaffected                       |

## Mandatory Reading

| Priority | File                                | Lines                | Why                                                                                        |
| -------- | ----------------------------------- | -------------------- | ------------------------------------------------------------------------------------------ |
| P0       | `backend/src/lib.rs`                | 108-131              | The flat merge chain — confirm NO change needed here (each module owns its strings)        |
| P0       | `backend/src/security/headers.rs`   | 46, 236-248          | `RESERVED_PREFIXES` + `is_reserved_prefix` prefix-match — proves `/api/v1` already covered |
| P0       | `backend/src/routes/library/mod.rs` | 56-61, 228, 640, 993 | route strings + `cover_url` format! sites                                                  |
| P1       | `backend/src/routes/opds/covers.rs` | 26-75                | dual-mount: `/api` mount moves, `/opds` mount stays                                        |
| P1       | `frontend/src/api/fetch.ts`         | 70, 133-158          | `apiFetch` applies NO prefix — literal paths per call                                      |
| P1       | `backend/tests/gen_openapi.rs`      | 22-66                | drift test asserts only `/health*` — PR1 does NOT regenerate the spec                      |
| P2       | `backend/src/error/instance.rs`     | 60-72                | tests assert Problem `instance` = `/api/...` path                                          |

External docs: none — PR1 is a mechanical path rename, no new library surface.

## Patterns to Mirror

**ROUTE STRING (backend) — just the prefix changes:**

```rust
// SOURCE: backend/src/routes/library/mod.rs:56-62 (BEFORE)
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/books", get(list))
        .route("/api/books/{id}", get(detail))
        .route("/api/works/{id}", get(work_detail))
        .route("/api/search", get(search::search))
}
// AFTER: "/api/books" → "/api/v1/books", etc. Signature + handlers UNCHANGED.
```

**COVER_URL GENERATION (backend response body) — must move in lockstep:**

```rust
// SOURCE: backend/src/routes/library/mod.rs:228 (and :640, :993; series/mod.rs:124; library/search.rs:213)
format!("/api/books/{m_id}/cover/thumb")   // BEFORE
format!("/api/v1/books/{m_id}/cover/thumb") // AFTER — these are the /api (CurrentUser) covers
```

**OPDS COVER (backend) — DOES NOT MOVE (exempt):**

```rust
// SOURCE: backend/src/routes/opds/covers.rs:28-47 — opds_router(), /opds/books/{id}/cover. LEAVE AS-IS.
// Only covers.rs:55-75 api_router() (/api/books/{id}/cover) moves to /api/v1.
```

**FRONTEND CALL SITE — literal path, no central constant:**

```typescript
// SOURCE: frontend/src/api/dashboard.ts:67 (BEFORE)
const raw = await apiFetch("/api/dashboard/stats", { signal });
// AFTER: "/api/v1/dashboard/stats". Mechanical per-site rename across 9 api/*.ts files.
// csrf.ts:92 fetch("/auth/me") is UNVERSIONED — LEAVE AS-IS.
```

**BACKEND INTEGRATION TEST — mirror existing harness, just the URL changes:**

```rust
// SOURCE: backend/src/routes/library/tests.rs (canonical shape)
let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
let response = server.get("/api/v1/books").add_header(AUTHORIZATION, basic).await; // was /api/books
assert_eq!(response.status_code(), StatusCode::OK);
```

## Files to Change

| File                                                                                                                                        | Action                  | Justification                                                                                                                                                                                                                                                                                                               |
| ------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `backend/src/routes/{library,users,shelves,series,settings,tokens,metadata,dashboard,enrichment,ingestion}/...`                             | UPDATE                  | `/api/` → `/api/v1/` in `.route(...)` strings (12 `/api` route files; `auth` excluded)                                                                                                                                                                                                                                      |
| `backend/src/routes/opds/covers.rs`                                                                                                         | UPDATE                  | only `api_router()` (`/api/books/{id}/cover*`) → `/api/v1`; `opds_router()` untouched                                                                                                                                                                                                                                       |
| `backend/src/routes/library/mod.rs`, `routes/series/mod.rs`, `routes/library/search.rs`                                                     | UPDATE                  | 5 `cover_url` `format!` sites → `/api/v1`                                                                                                                                                                                                                                                                                   |
| `backend/src/lib.rs`                                                                                                                        | NO CODE CHANGE (verify) | flat merge owns no path strings; only its `/api/...` _test_ URLs (~line 715) change                                                                                                                                                                                                                                         |
| `backend/src/security/headers.rs`                                                                                                           | UPDATE (tests only)     | add test: `/api/v1/x` is reserved (no prod change — prefix-match already covers); `RESERVED_PREFIXES` UNCHANGED                                                                                                                                                                                                             |
| `backend/src/error/instance.rs`                                                                                                             | UPDATE (tests)          | `instance` assertion paths → `/api/v1`                                                                                                                                                                                                                                                                                      |
| `backend/src/routes/**/tests.rs` + inline `#[cfg(test)]`                                                                                    | UPDATE                  | ~50 `server.get("/api/...")` → `/api/v1/...`; cover_url assertions → `/api/v1`                                                                                                                                                                                                                                              |
| `frontend/src/api/{books,dashboard,metadata,search,series,shelves,users}.ts`                                                                | UPDATE                  | ~30 literal `/api/...` → `/api/v1/...` (`csrf.ts` `/auth/me` untouched)                                                                                                                                                                                                                                                     |
| `frontend/src/api/*.test.ts` (8 files)                                                                                                      | UPDATE                  | URL-string assertions → `/api/v1/...` (`csrf.test.ts` `/auth/me` untouched)                                                                                                                                                                                                                                                 |
| **Docstrings/comments** in `backend/src/{models,routes,state.rs}` + `frontend/src/{api,pages,components,lib/query/keys.ts}` + `docs/` prose | UPDATE                  | ~30+ Tier-1 docstrings name `/api/...` endpoints that 404 after the move (e.g. `models/library.rs:65,114`, `models/shelf.rs:1,45`, `routes/users/mod.rs:67,103`, `state.rs:53`, `keys.ts:51`). NOT broken-link → `cargo doc` won't catch; doc-drift only review/santa-method finds. Hard-rule-10 (docs-as-done). See Task 8 |
| `frontend/vite.config.ts`                                                                                                                   | NO CHANGE (verify)      | `:71` `/api` dev proxy prefix-matches `/api/v1` → forwards correctly; no edit. See NOT Building                                                                                                                                                                                                                             |

## NOT Building (Scope Limits)

- **No `#[utoipa::path]` / `OpenApiRouter` / `ToSchema` work** — that's PR2..N.
- **No `docs/openapi.json` regeneration** — only `/health*` is in the spec and it
  stays unversioned, so the artifact is byte-identical. (If `gen_openapi` somehow
  diffs, STOP — something unexpected entered the spec.)
- **No central `API_V1_BASE` frontend constant** — mechanical find-replace only. A
  constant helps _future_ version bumps, not this PR (speculative per simplicity-first).
  Note as a deferred option only.
- **No `RESERVED_PREFIXES` edit** — `is_reserved_prefix` prefix-match already covers
  `/api/v1`; adding/replacing would be churn (and removing `/api` would break the
  gone-endpoint 404). Confirm with a test, don't touch the constant.
- **No `/auth`, `/health`, `/opds` path changes** — ADR-exempt.
- **No `vite.config.ts` proxy edit** — `:71` `"/api": { target }` is a path-_prefix_
  key, so it already forwards `/api/v1/*` (same no-op shape as `RESERVED_PREFIXES`).
  Verify with one dev request; do not change. (Per memory, dev runs single-origin on
  the reverie-dev LXC, so this proxy may be legacy — irrelevant to the no-op.)

## Step-by-Step Tasks

Execute in order. Backend and frontend MUST move together (atomic PR) — a half-moved
app is broken.

### Task 1: Backend — rename `/api/` → `/api/v1/` route strings (12 modules)

- **ACTION**: UPDATE `.route("/api/..."` → `.route("/api/v1/..."` in all `/api` route modules
- **FILES**: `routes/{library,users,shelves,series,settings,tokens,metadata,dashboard,enrichment,ingestion}/*.rs` + `routes/opds/covers.rs::api_router()`
- **EXCLUDE**: `routes/auth.rs` (`/auth/*`), all `routes/opds/*` except `covers.rs::api_router()`
- **GOTCHA**: `opds/covers.rs` has TWO functions — move only `api_router()`, leave `opds_router()`
- **VALIDATE**: `rg -P -n '"/api/(?!v1)' backend/src/routes` returns only `opds_router` cover lines (expected) — no other bare `/api/`. **NOTE:** the `-P` (PCRE2) flag is required — ripgrep's default Rust-regex engine rejects the `(?!v1)` look-around with a parse error. Lookahead-free alternative if PCRE2 unavailable: `rg -n '\.route\("/api/[^v]' backend/src/routes`

### Task 2: Backend — move the 5 `cover_url` `format!` generation sites

- **ACTION**: UPDATE `format!("/api/books/{}/cover/thumb", ...)` → `/api/v1/books/...`
- **FILES**: `routes/library/mod.rs:228,640,993`, `routes/series/mod.rs:124`, `routes/library/search.rs:213`
- **GOTCHA**: these are response-body strings, not route registrations — easy to miss; covers break silently in the UI if skipped
- **VALIDATE**: `rg -n 'format!\("/api/books' backend/src` returns nothing

### Task 3: Backend — update integration + unit test URLs

- **ACTION**: UPDATE `server.get/post/...("/api/...")` → `/api/v1/...`; cover_url assertions → `/api/v1`
- **FILES**: `routes/**/tests.rs`, inline `#[cfg(test)]` (`ingestion.rs`, `tokens.rs`, `metadata.rs`, `enrichment.rs`), `lib.rs` (~715), `error/instance.rs` (~60-72), `security/headers.rs` tests
- **VALIDATE**: `rg -P -n '"/api/(?!v1)' backend/src --type rust` clean except intended opds cover (requires `-P`; see Task 1 note)

### Task 4: Backend — add the gone-endpoint negative test (the real TDD assertion)

- **ACTION**: ADD test asserting the _move_ (not additive): old path 404s, new path works, fallback is JSON
- **IMPLEMENT** (mirror `library/tests.rs` harness):
  - `GET /api/books` (old) → `404` with `application/problem+json` body (NOT SPA HTML)
  - `GET /api/v1/books` (new) → `200`
- **WHERE**: `routes/library/tests.rs` (has the auth + dual-pool harness already)
- **WHY**: renaming ~50 existing assertions is mechanical mirroring, not a test of the move; this is
- **VALIDATE**: `cargo test -p reverie_api routes::library` (needs dev DB up — `docker compose up -d`)

### Task 5: Backend — add reserved-prefix fallback test for `/api/v1`

- **ACTION**: ADD `assert!(is_reserved_prefix("/api/v1/__nope__"))` and an integration test that an unmatched `/api/v1/...` returns JSON Problem 404, not SPA HTML
- **WHERE**: `security/headers.rs` unit tests (`is_reserved_prefix_*`) + the composite-fallback integration tests in same module
- **GOTCHA**: confirms the no-code-change claim for `RESERVED_PREFIXES` is correct
- **VALIDATE**: `cargo test -p reverie_api security::headers`

### Task 6: Frontend — rename literal `/api/` → `/api/v1/` call sites

- **ACTION**: UPDATE every `apiFetch`/`buildUrl`/`new URL` literal in `src/api/*.ts`
- **FILES**: `api/{books,dashboard,metadata,search,series,shelves,users}.ts` (~30 sites)
- **EXCLUDE**: `api/csrf.ts:92` `fetch("/auth/me")` — unversioned
- **VALIDATE**: `rg -P -n '/api/(?!v1)' frontend/src/api` clean except `/auth/me` (requires `-P`; see Task 1 note)

### Task 7: Frontend — update test URL assertions

- **ACTION**: UPDATE `*.test.ts` URL-string expectations → `/api/v1/...`
- **FILES**: `api/{books,fetch,search,shelves,metadata,users,series}.test.ts`
- **EXCLUDE**: `csrf.test.ts` (`/auth/me`)
- **VALIDATE**: `cd frontend && npm test`

### Task 8: Update stale `/api/...` docstrings, comments, and `docs/` prose

- **ACTION**: UPDATE `/api/` → `/api/v1/` in all docstrings/comments naming a moved endpoint
- **FILES**: `backend/src/{models,routes,state.rs}` (~30 Tier-1 docstrings, e.g. `models/library.rs:65,114,246,273`, `models/shelf.rs:1,10,45,62`, `models/series.rs:1,22`, `models/settings.rs:106`, `models/user.rs:131,159`, `routes/users/mod.rs:1,21,46,67,...`, `state.rs:53`), `frontend/src/{pages,components,lib/query/keys.ts}` prose, `docs/` narrative
- **EXCLUDE**: `/auth`, `/health`, `/opds` references; `@/api/*` import aliases (module paths, NOT URLs — leave); the `/api/v1` reference already in `openapi.rs:26`
- **WHY**: these are not broken-intra-doc-links, so `cargo doc` will NOT catch them — they silently document paths that now 404. Hard-rule-10 (docs-as-done); doc-drift otherwise only caught by santa-method/human review
- **VALIDATE**: `rg -P -n '/api/(?!v1)' backend/src frontend/src docs` returns only the intended `/opds`-cover line + any genuinely-unversioned refs (requires `-P`; see Task 1 note)

### Task 9: Full backend validation

- **ACTION**: run the backend gate locally
- **VALIDATE**:
  - `cd backend && cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --locked -- -D warnings`
  - `cargo test -p reverie_api` (dev DB up)
  - confirm `cargo test --test gen_openapi` passes with **no** regeneration (spec byte-identical)

### Task 10: Repo-lint + commit

- **ACTION**: run repo-lint stack (per memory `feedback_run_repo_lint_stack_locally`), commit on branch `feat/unk-376-api-v1-mount-move`
- **VALIDATE**: `typos`, `shellcheck`/`hadolint`/`yamllint`/`markdownlint` as applicable (no shell/yaml/md changes expected → mostly `typos`)
- **PR body**: use `Part of UNK-376` — **NOT** `Closes`. Per the epic's Linear close policy (see roadmap), only PR-final closes UNK-376. This is a deliberate documented exception to hard-rule-9 for the multi-PR epic.

## Testing Strategy

### Tests to Write (new)

| Test                                   | Validates                                                     |
| -------------------------------------- | ------------------------------------------------------------- |
| `library::tests` gone-endpoint         | old `/api/books`→404 JSON, new `/api/v1/books`→200 (the move) |
| `security::headers` `/api/v1` reserved | unmatched `/api/v1/*`→JSON Problem 404, not SPA HTML          |

### Tests to Update (mechanical mirror)

~50 backend `server.get("/api/...")` + ~30 frontend URL assertions → `/api/v1`.

### Edge Cases Checklist

- [ ] Old `/api/*` path → JSON 404 (not SPA HTML, not 200)
- [ ] `/opds/books/{id}/cover` still resolves (NOT moved)
- [ ] `/api/v1/books/{id}/cover/thumb` (moved api cover) resolves
- [ ] `cover_url` in list/detail/series responses points at `/api/v1`
- [ ] `/auth/me`, `/health`, `/opds/*` unchanged
- [ ] `gen_openapi` drift test passes WITHOUT regeneration

## Validation Commands

### Level 1: STATIC

```bash
cd backend && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings
```

```bash
cd frontend && npm run lint
```

### Level 2: UNIT/INTEGRATION

```bash
cd backend && cargo test -p reverie_api
```

```bash
cd frontend && npm test
```

### Level 3: DRIFT GATE (must pass with NO regen)

```bash
cd backend && cargo test --test gen_openapi
```

EXPECT: green, `docs/openapi.json` unchanged in `git status`.

### Level 4: BROWSER (per memory — source-only review missed a P0 once)

Probe `reverie-dev.unkos.net` after Mutagen sync; `agent-browser` screenshot of the
library list + a cover image (confirms moved `cover_url` resolves). Default per
`feedback_use_browser_for_design_critique`.

## Acceptance Criteria (PR1)

- [ ] All 24 data routes served under `/api/v1/...`; `/health`,`/auth`,`/opds` unversioned
- [ ] Old `/api/*` returns JSON Problem 404 (gone-endpoint test green)
- [ ] `cover_url` response strings + moved api cover mount on `/api/v1`; `/opds` cover untouched
- [ ] Frontend reaches API at `/api/v1`; `npm test` green
- [ ] `gen_openapi` passes with no regeneration
- [ ] Level 1-3 green; browser smoke confirms covers render
- [ ] PR references UNK-376 (does NOT close it)

## Risks and Mitigations

| Risk                                                               | Likelihood | Impact | Mitigation                                                                                                                              |
| ------------------------------------------------------------------ | ---------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| Miss a `cover_url` `format!` site → broken covers, no test failure | MED        | MED    | Task 2 grep gate + browser smoke (Level 4)                                                                                              |
| Move the `/opds` cover mount by mistake                            | MED        | MED    | Task 1 explicit exclude + edge-case checklist                                                                                           |
| `RESERVED_PREFIXES` assumption wrong → SPA HTML for `/api/v1` 404  | LOW        | HIGH   | Verified prefix-match (headers.rs:241); Task 5 locks it with a test                                                                     |
| `security/headers.rs` change trips hard-rule-6 (response headers)  | —          | —      | PR1 touches it tests-only; still answer "stands up to security review?" re: fallback routing in PR summary; consult `.claude/security/` |
| Half-moved app (backend moved, frontend not)                       | LOW        | HIGH   | Single atomic PR; both in same branch                                                                                                   |

## Notes

- **Security (hard-rule-6):** PR1 touches `security/headers.rs` (tests only) and the
  fallback-routing surface. Answer in the PR summary: the gone-endpoint path correctly
  returns JSON `404` (no SPA-HTML leak for stale API clients); no auth/CSP behaviour
  changes. Consult `.claude/security/` before done.
- **Why mount-move-first:** `#[utoipa::path(path=...)]` carries the literal served
  path (no `.nest("/api")` exists). Moving first means PR2..N annotations are written
  once at `/api/v1/...` instead of being rewritten mid-migration.
- After code changes this session, run `graphify update .` to keep the graph current.
