# Feature: Step 11 — Library REST API and React Frontend

## Summary

Build the JSON REST API surface that the production web UI consumes, and the React UI that consumes it. Replaces the dev-only hero screens (Step 10 D4) with real data, while keeping their visual contract intact (`Book` fixture shape in `frontend/src/pages/design/fixtures/books.ts` was deliberately authored to mirror the eventual API shape). Backend mirrors the existing OPDS read pattern (`backend/src/routes/opds/library.rs`) — same DB shape, same RLS seam, same cursor pagination — only the serialiser changes (Atom XML → JSON). Frontend grows production routes on top of the existing Tailwind v4 + shadcn + theme infrastructure, adding `@tanstack/react-query` and centralised `src/api/` client.

Step 11 is the largest blueprint step (20 tasks across 9 endpoints + 11 frontend features). This plan breaks it into **six sub-phases (11a–11f)**, each shippable as a standalone PR with its own validation gate. Sub-phase 11a is the foundation; 11b–11e build out functionality on the same scaffold; 11f is architecturally separable (introduces persisted settings).

## User Story

As an **adult or admin Reverie user**
I want to **browse, search, edit and curate my library from the web UI**
So that **I can manage what I own without leaving the browser, accept or reject AI/OPF metadata drafts, and maintain shelves and series**.

## Problem Statement

Today Reverie has a fully working ingestion + enrichment + writeback pipeline and an OPDS feed for e-readers, but the only browser-visible surface is `/auth/login`, `/auth/me`, `/auth/me/theme`, the cover endpoints and ingestion/enrichment trigger endpoints. There is no production frontend route, no `/api/books` JSON endpoint, no UI for accepting metadata drafts, and no admin or settings UI. Operators cannot use the library from a browser — only from an e-reader via OPDS.

## Solution Statement

Step 11 ships in **six sub-phase PRs** (11a → 11f). Each PR is independently mergeable and adds one cohesive slice of functionality:

| Sub-phase                                | Branch (suggested)           | Backend                                                                              | Frontend                                                                          | Depends on                                                               |
| ---------------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| **11a — Foundations + read list/detail** | `feat/unk-80-library-ui-11a` | `GET /api/books`, `GET /api/books/{id}`, `GET /api/works/{id}`                       | `src/api/` client, react-query, react-router data mode, library grid, book detail | Step 10 (merged)                                                         |
| **11b — Search + filters**               | `feat/unk-80-library-ui-11b` | `GET /api/search?q=`, filter params on `/api/books`                                  | Command palette (Cmd-K), shelf chips, series filter                               | 11a                                                                      |
| **11c — Manual metadata edit**           | `feat/unk-80-library-ui-11c` | `PATCH /api/books/{id}/metadata` (RFC 7396 JSON Merge Patch)                         | Accept/reject UI in book detail, manual edit form                                 | 11a, existing `/api/manifestations/{id}/metadata/{accept,reject,revert}` |
| **11d — Series + shelves CRUD**          | `feat/unk-80-library-ui-11d` | `GET /api/series/{id}`, CRUD `/api/shelves`, shelf items reorder                     | Series page, shelves sidebar with reorder/assign                                  | 11a                                                                      |
| **11e — Admin**                          | `feat/unk-80-library-ui-11e` | `GET/PUT /api/users`, `PUT /api/users/{id}/role`, `PUT /api/users/{id}/child-status` | Admin panel route (role=admin gate)                                               | 11a                                                                      |
| **11f — Settings (persisted)**           | `feat/unk-80-library-ui-11f` | New `settings` table + reload mechanism, `GET/PUT /api/settings`                     | Settings page                                                                     | 11a + ADR on persisted settings                                          |

Single committed plan file (this one). Sub-phase PRs each cite this plan plus open a follow-up Linear issue.

### Prerequisites before any sub-phase begins

1. **Linear umbrella issue** — `UNK-80` (filed 2026-04-13, status Backlog, <https://linear.app/unkos/issue/UNK-80>). Plan linked as attachment on the issue. Sub-issues filed lazily as each sub-phase starts — don't pre-file all six.
2. **Branch off main, not the current branch** — this plan file is being authored on `chore/unk-236-eslint-plugin-jsdoc-install` (the JSDoc tooling branch). All sub-phase branches must be created from `main` AFTER that JSDoc branch is merged, so the new code lints under the live jsdoc rules from commit 1.
3. **Verify Step 10 is on main** — D4 hero screens merged PR #279 (commit `19586ae`). Confirm before branching 11a; the production routes import shared components extracted from `src/pages/design/`.

## Metadata

| Field            | Value                                                                                                                             |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Type             | NEW_CAPABILITY                                                                                                                    |
| Complexity       | HIGH                                                                                                                              |
| Systems Affected | backend HTTP routes, backend services, frontend src/api, frontend routes, frontend components, eslint/jsdoc surface               |
| Dependencies     | axum 0.8.9, sqlx 0.8.6, tower-sessions, axum-login, time 0.3.47, react-router 7.15.1, react-query (NEW), shadcn radix-nova v4.7.0 |
| Estimated Tasks  | ~60 across 6 sub-phases                                                                                                           |

---

## UX Design

### Before State

```text
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌──────────────┐      ┌────────────────┐      ┌────────────────────┐        ║
║   │ Browser      │ ──►  │  GET /         │ ──►  │ App.tsx (empty     │        ║
║   │ visits root  │      │  (Vite SPA)    │      │  <main> + TODO)    │        ║
║   └──────────────┘      └────────────────┘      └────────────────────┘        ║
║                                                                               ║
║   ┌──────────────┐      ┌────────────────┐      ┌────────────────────┐        ║
║   │ E-reader     │ ──►  │ GET /opds/...  │ ──►  │ Atom XML feed       │       ║
║   │ (KOReader)   │      │ (BasicOnly)    │      │ acquisition feed    │       ║
║   └──────────────┘      └────────────────┘      └────────────────────┘        ║
║                                                                               ║
║   PAIN_POINT: Browser users have no library UI. To accept enrichment drafts,  ║
║   curate shelves, edit metadata, or administer users, they must hit the DB    ║
║   directly. OPDS only services downloads on e-reader; browser is unused.      ║
║                                                                               ║
║   DATA_FLOW: SQL → models::* → sqlx → Atom XML emitter → e-reader.            ║
║   No JSON list path exists; cover JPEGs and one-shot endpoints (auth, tokens, ║
║   enrichment trigger, accept/reject) are the only browser-callable JSON.      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```text
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌──────────────┐     ┌─────────────────┐     ┌──────────────────────┐       ║
║   │ Browser      │ ──► │ react-router    │ ──► │ /library, /b/:id,    │       ║
║   │ visits root  │     │ data mode       │     │ /series/:id, /admin, │       ║
║   └──────────────┘     │ + loaders       │     │ /settings            │       ║
║                        └─────────────────┘     └──────────┬───────────┘       ║
║                                                            │                  ║
║                                            useQuery / useMutation             ║
║                                                            │                  ║
║                                                            ▼                  ║
║                                                   ┌────────────────┐          ║
║                                                   │ src/api/*      │          ║
║                                                   │ centralised    │          ║
║                                                   │ fetch wrappers │          ║
║                                                   └────┬───────────┘          ║
║                                                        │ same-origin          ║
║                                                        │ cookie session       ║
║                                                        ▼                      ║
║   ┌────────────────────────────────────────────────────────────────────┐      ║
║   │ Axum router (build_router_with_session_store)                       │     ║
║   │   /api/books           (list + filter + sort + cursor)              │     ║
║   │   /api/books/:id       (detail w/ versions + status)                │     ║
║   │   /api/works/:id       (work + manifestations grouped)              │     ║
║   │   /api/library/search  (tsvector + trigram hybrid)                  │     ║
║   │   /api/series/:id      (work group)                                 │     ║
║   │   /api/shelves         (CRUD)                                       │     ║
║   │   /api/shelves/:id/items (reorder)                                  │     ║
║   │   /api/books/:id/metadata (PUT manual override)                     │     ║
║   │   /api/users           (admin list/edit)                            │     ║
║   │   /api/settings        (GET/PUT — persisted)                        │     ║
║   └────────────────────────────────────────────────────────────────────┘      ║
║                                                                               ║
║   VALUE_ADD: Browser parity with OPDS for reads; new metadata curation,       ║
║   shelf curation, admin and settings management.                              ║
║   DATA_FLOW: SQL → models::* → sqlx → handler::Json(struct) → react-query     ║
║   cache (keyed) → component. Mutations invalidate `['books']` etc.            ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location                 | Before         | After                                                  | User Impact                         |
| ------------------------ | -------------- | ------------------------------------------------------ | ----------------------------------- |
| `/` (browser)            | Empty `<main>` | Library grid (with cover, title, author, series badge) | Can browse library                  |
| `/library?view=list`     | n/a            | Compact sortable table                                 | Power-user dense view               |
| `/b/:id`                 | n/a            | Book detail: metadata, versions, accept/reject buttons | Curate metadata                     |
| `/series/:id`            | n/a            | Series page w/ completeness indicator                  | See gaps in series                  |
| `/admin/users`           | n/a            | User list + role + child-status                        | Admin can manage users              |
| `/settings`              | n/a            | Path template, format priority, enrichment config      | Operator config without env restart |
| `Cmd-K` anywhere         | (nothing)      | Command palette: search + jump                         | Fast nav                            |
| Backend `/api/books/...` | 404            | JSON resources, RLS-scoped, cursor pagination          | UI works; OPDS untouched            |

---

## Mandatory Reading

**Implementation agent (and each sub-phase agent) MUST read these files before starting any task:**

| Priority | File                                                                                   | Lines               | Why Read This                                                                                     |
| -------- | -------------------------------------------------------------------------------------- | ------------------- | ------------------------------------------------------------------------------------------------- |
| P0       | `/home/coder/reverie/backend/src/routes/opds/library.rs`                               | 47-356              | Canonical list/search/cursor pattern — every JSON list endpoint mirrors this                      |
| P0       | `/home/coder/reverie/backend/src/routes/enrichment.rs`                                 | 46-155              | Canonical JSON handler pattern (CurrentUser + State + Path + Query + Json)                        |
| P0       | `/home/coder/reverie/backend/src/db.rs`                                                | 98-108              | `acquire_with_rls` — mandatory on every user-scoped DB touch                                      |
| P0       | `/home/coder/reverie/backend/src/auth/middleware.rs`                                   | 50-103, 196-222     | `CurrentUser`, `require_admin`, `require_not_child`                                               |
| P0       | `/home/coder/reverie/backend/src/error.rs`                                             | 31-103              | `AppError` enum + JSON error envelope `{"error": "..."}`                                          |
| P0       | `/home/coder/reverie/backend/src/routes/metadata.rs`                                   | 188-336, 391-502    | Accept/reject/revert handlers + `apply_version` field dispatch — 11c builds on this               |
| P0       | `/home/coder/reverie/backend/src/routes/opds/cursor.rs`                                | all                 | Base64url cursor format used unchanged                                                            |
| P0       | `/home/coder/reverie/backend/src/test_support.rs`                                      | 130-141, 266-423    | `test_server`, `server_with_real_pools`, user/shelf/manifestation fixtures                        |
| P1       | `/home/coder/reverie/backend/src/routes/auth.rs`                                       | 192-199             | JSON response shape for `/auth/me` (snake_case, no rename) — convention                           |
| P1       | `/home/coder/reverie/backend/src/routes/mod.rs`                                        | 1-24                | Route module convention (`pub fn router() -> Router<AppState>`)                                   |
| P1       | `/home/coder/reverie/backend/src/lib.rs`                                               | 102-153             | Router assembly + CSP wiring (HTML CSP vs API CSP layers)                                         |
| P1       | `/home/coder/reverie/backend/migrations/20260412150007_search_rls_and_reserved.up.sql` | 1-119               | RLS policies on `manifestations` (4 policies) + trigram + GIN indexes                             |
| P1       | `/home/coder/reverie/backend/migrations/20260417000001_add_enrichment_pipeline.up.sql` | 22-144              | `metadata_versions` shape + canonical `*_version_id` pointers — "accepted" mechanic               |
| P1       | `/home/coder/reverie/backend/src/models/work.rs`                                       | 88-203              | `match_existing` (trigram find) + `upgrade_stub` (multi-step write)                               |
| P0       | `/home/coder/reverie/frontend/src/pages/design/library.tsx`                            | all                 | Visual target for `/library`; component contract                                                  |
| P0       | `/home/coder/reverie/frontend/src/pages/design/book.tsx`                               | all                 | Visual target for `/b/:id`; Tabs + sticky cover layout                                            |
| P0       | `/home/coder/reverie/frontend/src/pages/design/fixtures/books.ts`                      | all                 | `Book` interface — explicitly authored to mirror Step 11 API shape                                |
| P0       | `/home/coder/reverie/frontend/src/main.tsx`                                            | 1-30                | Router bootstrap + dev-only `designRoutes` dynamic import                                         |
| P0       | `/home/coder/reverie/frontend/src/lib/theme/api.ts`                                    | all                 | Existing fetch pattern: `credentials: "same-origin"`, no token header                             |
| P0       | `/home/coder/reverie/frontend/src/lib/theme/ThemeProvider.tsx`                         | 109-138             | 401-as-happy-path pattern (cookie fallback) — same applies to react-query 401 handler             |
| P0       | `/home/coder/reverie/frontend/vite.config.ts`                                          | 21-28, 48-51, 84-88 | Dev CSP, design-chunk gate, dev proxy `/api → :3000`                                              |
| P1       | `/home/coder/reverie/frontend/src/styles/themes/index.css`                             | 64-181              | All design tokens used by hero screens — production routes inherit identically                    |
| P1       | `/home/coder/reverie/frontend/eslint.config.js`                                        | 151-193             | jsdoc rules at `warn` — every new exported function/type/interface needs `/** */` per ADR ratchet |
| P1       | `/home/coder/reverie/frontend/components.json`                                         | all                 | `radix-nova` style, alias map; use for `npx shadcn add`                                           |
| P1       | `/home/coder/reverie/CLAUDE.md` (Tiered Comment Policy)                                | 92-101              | Tier 1 JSDoc on public exports; Tier 4 on tests; Tier 2 unlikely in Step 11                       |
| P1       | `/home/coder/reverie/backend/CLAUDE.md`                                                | (whole)             | Rust-side conventions, sqlx allowlist                                                             |
| P1       | `/home/coder/reverie/frontend/CLAUDE.md`                                               | (whole)             | Frontend conventions, `src/api/` mandate                                                          |
| P2       | `/home/coder/reverie/adr/2026-05-08-tiered-comment-policy.md`                          | all                 | Tier rationale                                                                                    |
| P2       | `/home/coder/reverie/adr/2026-05-22-frontend-docstring-tooling.md`                     | all                 | jsdoc ratchet timing                                                                              |
| P2       | `/home/coder/reverie/SDL.md`                                                           | all                 | Tool-use workflow (note: sdl-mcp connectivity can drop mid-session — fall back to Read/Grep)      |

**External Documentation:**

| Source                                                                                                                          | Section                              | Why Needed                                                                                                                |
| ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| [axum 0.8.9 — Extractors](https://docs.rs/axum/0.8.9/axum/extract/index.html)                                                   | Path/Query/Json + extractor order    | Confirm path syntax `{id}` (NOT `:id`); typed JsonRejection                                                               |
| [axum-extra Query](https://docs.rs/axum-extra/latest/axum_extra/extract/struct.Query.html)                                      | repeated-key params                  | Built-in axum `Query<T>` cannot deserialize `?tag=a&tag=b` into `Vec`; need axum-extra or serde_qs                        |
| [sqlx 0.8 — QueryBuilder](https://docs.rs/sqlx/0.8.6/sqlx/query_builder/struct.QueryBuilder.html)                               | dynamic WHERE / tuple cursor         | Dynamic filtering — `query!` macros can't do this; use `QueryBuilder` with `build_query_as`                               |
| [TanStack Query v5 — QueryCache onError](https://tanstack.com/query/v5/docs/reference/QueryCache)                               | global error handling                | v5 removed `defaultOptions.queries.onError`; correct hook is `QueryCache({ onError })` for 401 → redirect                 |
| [TanStack Query v5 — useQuery / useSuspenseQuery](https://tanstack.com/query/v5/docs/framework/react/reference/useQuery)        | hook usage + retry                   | `retry: (count, err) => err.status !== 401` to avoid retrying auth failures                                               |
| [React Router v7 — Data Mode](https://reactrouter.com/start/data/data-loading)                                                  | loaders + lazy                       | Loader pattern: `queryClient.prefetchQuery` inside loader, `useQuery` in component (Tkdodo pattern, officially supported) |
| [PostgreSQL textsearch-controls](https://www.postgresql.org/docs/current/textsearch-controls.html)                              | `websearch_to_tsquery`, `ts_rank_cd` | Use `websearch_to_tsquery` for the search input (handles quotes, `OR`, `-exclude`); not `plainto_tsquery`                 |
| [OWASP CSRF Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html) | custom header defense                | SameSite=Lax alone is insufficient per OWASP; require custom `X-Requested-With` header on mutating verbs                  |
| [MDN HTTP caching](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Caching)                                            | `private, no-cache` + ETag           | Cover Cache-Control: switch from `no-store` to `private, no-cache` + ETag (cover hash) — bandwidth win, RLS-safe          |
| [shadcn — Data Table](https://ui.shadcn.com/docs/components/data-table)                                                         | manualPagination                     | Server-side pagination requires `manualPagination: true` + skipping `getPaginationRowModel`                               |
| [shadcn — Command](https://ui.shadcn.com/docs/components/command)                                                               | CommandDialog                        | Cmd-K wraps Command in a Radix Dialog                                                                                     |

**Version constraints (from package.json / Cargo.toml):**

- backend: `axum 0.8.9`, `sqlx 0.8.6`, `tower-http 0.6.11`, `serde 1.0.228`, `time 0.3.47`, `tower-sessions ≥ 0.x` (per existing axum-login pin), `axum-login` (pre-PR #128 — see project memory, not blocking for Step 11)
- frontend: `react-router 7.15.1`, `react ^19` (per CLAUDE-pattern), `vite 7.x` (verify), `tailwindcss 4.x` (via `@tailwindcss/vite`), `shadcn 4.7.0` (radix-nova), `eslint-plugin-jsdoc ^63.0.0`
- new frontend deps to add: `@tanstack/react-query@^5`, `@tanstack/react-table@^8` (only for sub-phase 11a if list view is built immediately; otherwise defer to 11b)

**Gotchas (from research + codebase):**

- Axum 0.8 path syntax is `{id}` not `:id` (already adopted; ensure new routes match).
- `Query<T>` built-in cannot decode repeated-key params → use `axum_extra::extract::Query` or `serde_qs::axum::QsQuery`. Decision in Phase 11b.
- Compile-time `sqlx::query!` macros cannot handle conditional WHERE clauses → use `QueryBuilder::push_bind` (already used at `library.rs:236-247`). Runtime SQL is allowlist-gated per `.github/sqlx-runtime-allowlist.txt` — every new runtime query gets a line in that file.
- `time` crate, not `chrono` — RFC3339 serde format is default.
- `pg_trgm` `similarity()` threshold: keep `find_or_create` dedup at `0.6` (existing). For user-facing search use `0.3` to catch typos (separate predicate).
- shadcn `bg-black` overlay issue is tracked debt (project memory `project_bg_black_overlays_deferred.md`). Fix when first touching modal/dialog/sheet in any sub-phase (likely 11c or 11d).
- `Cache-Control: no-store` on cover endpoints is overly conservative; switch to `private, no-cache` + ETag during 11a (or carve into a separate PR if scope inflates).
- Vite dev server is **persistent** at `localhost:5173` — never `npm run dev`, never `pkill`; just probe (project memory `feedback_vite_dev_server_persistent.md`).
- npm only — no pnpm/yarn/bun (project memory `feedback_reverie_frontend_is_npm.md`).
- No parallel `cargo test` across worktrees (project memory) — dispatch sub-phase test runs sequentially when multiple worktrees are live.
- Settings persistence (11f) needs an ADR — `Config` is presently env-only with no live-reload mechanism, and changing that is an architectural decision (database table vs single-row config vs env-watch). Plan 11f gated on that ADR.

---

## Patterns to Mirror

**ROUTE_MODULE_BOILERPLATE** (backend):

```rust
// SOURCE: backend/src/routes/mod.rs:1-24 + every routes/*.rs
// COPY THIS PATTERN — every new module exports a `router()` fn:
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/books", get(list))
        .route("/api/books/{id}", get(detail))
}
// register in lib.rs::build_router_with_session_store alongside the other .merge() calls
```

**JSON_GET_HANDLER** (backend, with RFC 8288 Link header):

```rust
// SOURCE: backend/src/routes/enrichment.rs:119-155 — adapted as the canonical JSON list shape.
// Pagination signaling per RFC 8288 (Link header) + in-body `next_cursor` for JS-client convenience.
async fn list(
    current_user: CurrentUser,
    State(state): State<AppState>,
    axum_extra::extract::Query(params): axum_extra::extract::Query<ListParams>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
        "SELECT m.id, m.created_at, m.isbn_13, w.title, w.description \
         FROM manifestations m JOIN works w ON w.id = m.work_id WHERE TRUE",
    );
    if let Some(author_id) = params.author {
        qb.push(" AND EXISTS (SELECT 1 FROM work_authors wa \
                 WHERE wa.work_id = w.id AND wa.author_id = ");
        qb.push_bind(author_id);
        qb.push(")");
    }
    push_cursor_predicate(&mut qb, parse_cursor(params.cursor.as_deref(), params.sort)?.as_ref());
    push_order_by(&mut qb, params.sort);
    qb.push(" LIMIT ");
    qb.push_bind(page_size + 1);

    let rows: Vec<BookListRow> = qb.build_query_as().fetch_all(&mut *tx).await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;

    let (items, next_cursor) = paginate(rows, page_size, params.sort);

    // RFC 8288 Link header — canonical pagination signal.
    let mut headers = HeaderMap::new();
    if let Some(ref nc) = next_cursor {
        let next_url = build_next_url(&uri, nc);  // preserves all other query params
        headers.insert(LINK, HeaderValue::from_str(&format!("<{next_url}>; rel=\"next\""))
            .map_err(|e| AppError::Internal(e.into()))?);
    }

    Ok((headers, Json(BookListResponse { items, next_cursor })))
}
```

**PROBLEM_DETAILS_ERROR_ENVELOPE** (backend, RFC 7807):

```rust
// SOURCE: NEW — backend/src/error.rs IntoResponse impl after Task 1b migration.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, problem_type, title, detail) = match self {
            Self::NotFound        => (404, "not-found",        "Not Found",            "Resource not found.".into()),
            Self::Unauthorized    => (401, "unauthorized",     "Unauthorized",         "Authentication required.".into()),
            Self::Forbidden       => (403, "forbidden",        "Forbidden",            "Access denied.".into()),
            Self::Validation(msg) => (422, "validation",       "Unprocessable Entity", msg),
            Self::CsrfMissing     => (428, "csrf-missing",     "Precondition Required", "X-CSRF-Token header required.".into()),
            Self::CsrfMismatch    => (403, "csrf-mismatch",    "Forbidden",            "CSRF token invalid.".into()),
            Self::IfMatchRequired => (428, "if-match-required","Precondition Required", "If-Match header required for this operation.".into()),
            Self::IfMatchMismatch => (412, "if-match-mismatch","Precondition Failed",  "Resource changed since last read.".into()),
            Self::Internal(err)   => {
                tracing::error!(error = %err, "internal server error");
                (500, "internal", "Internal Server Error", "An internal error occurred.".into())
            }
        };
        let body = serde_json::json!({
            "type":   format!("https://reverie.example/probs/{problem_type}"),
            "title":  title,
            "status": status,
            "detail": detail,
        });
        let mut response = (StatusCode::from_u16(status).unwrap(), Json(body)).into_response();
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
```

**JSON_PUT_HANDLER_WITH_TYPED_REJECTION** (backend, new convention for 11c, 11d, 11e, 11f):

```rust
// SOURCE: standard axum 0.8 idiom, adapted to AppError envelope.
async fn update_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<Json<UpdateMetadataRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;
    let Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;
    // ... apply, return Json(BookDetail)
}
```

**RLS_TRANSACTION**:

```rust
// SOURCE: backend/src/db.rs:98-108 — call this on EVERY user-facing DB touch
let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
// ... queries against tx
tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;
```

**INTEGRATION_TEST_HARNESS**:

```rust
// SOURCE: backend/src/routes/opds/tests.rs:73-99
#[sqlx::test(migrations = "./migrations")]
async fn books_list_returns_only_visible_to_current_user(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server.get("/api/books").add_header(AUTHORIZATION, basic).await;
    assert_eq!(r.status_code(), StatusCode::OK);
    // ... assert body shape
}
```

**ERROR_ENVELOPE_CONSISTENCY**:

```rust
// SOURCE: backend/src/error.rs:101 — every error body is {"error": "..."}
// Step 11 MUST NOT introduce {"errors": [...]} or other shapes.
let body = serde_json::json!({ "error": message });
(status, axum::Json(body)).into_response()
```

**FRONTEND_API_CLIENT_MODULE** (new convention, mirrors existing fetch shape from `src/lib/theme/api.ts`):

```typescript
// NEW FILE: frontend/src/api/books.ts
// MIRRORS: frontend/src/lib/theme/api.ts — same-origin cookie, no Authorization header.

/** A book in a paginated list response. Snake_case fields mirror the Rust JSON shape. */
export interface BookListItem {
  id: string;
  work_id: string;
  title: string;
  authors: string[];
  series?: { id: string; name: string; position: number | null };
  isbn_13: string | null;
  cover_url: string;
  ingestion_status: "pending" | "staged" | "managed" | "failed";
  validation_status: "clean" | "repaired" | "degraded" | "quarantined";
  enrichment_status: "pending" | "in_progress" | "complete" | "failed" | "skipped";
}

export interface BookListResponse {
  items: BookListItem[];
  next_cursor: string | null;
}

/** Fetch a paginated list of books. Cursor is opaque base64url. Read `next_cursor` from body OR parse the RFC 8288 Link header — both carry the same information. */
export async function listBooks(
  params: {
    cursor?: string;
    author?: string;
    series?: string;
    shelf?: string;
    q?: string;
    sort?: "recent" | "title" | "author";
  },
  signal?: AbortSignal,
): Promise<BookListResponse> {
  const url = new URL("/api/books", window.location.origin);
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined) url.searchParams.set(k, v);
  }
  // GET — no CSRF token needed (safe verb, not gated by csrf middleware).
  return apiFetch(url, { method: "GET", signal }) as Promise<BookListResponse>;
}
```

**FRONTEND_API_FETCH_WRAPPER** (`src/api/fetch.ts`):

```typescript
import { getCsrfToken, refreshCsrfToken } from "@/api/csrf";
import { ApiError } from "@/api/errors";

/**
 * Centralised fetch wrapper. Injects CSRF token on mutating verbs (POST/PUT/PATCH/DELETE).
 * Parses RFC 7807 Problem Details on error responses. One retry on CSRF token rotation (403 csrf-mismatch).
 */
export async function apiFetch<T = unknown>(
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<T> {
  const method = (init?.method ?? "GET").toUpperCase();
  const headers = new Headers(init?.headers);
  headers.set("Accept", "application/json");
  if (method !== "GET" && method !== "HEAD" && method !== "OPTIONS") {
    headers.set("X-CSRF-Token", getCsrfToken());
  }
  let res = await fetch(input, { ...init, credentials: "same-origin", headers });
  if (res.status === 403) {
    const problem = await peekProblem(res);
    if (problem?.type?.endsWith("/csrf-mismatch")) {
      await refreshCsrfToken();
      headers.set("X-CSRF-Token", getCsrfToken());
      res = await fetch(input, { ...init, credentials: "same-origin", headers });
    } else {
      throw await problemFromResponse(res, problem);
    }
  }
  if (!res.ok) throw await problemFromResponse(res);
  return res.json() as Promise<T>;
}

async function problemFromResponse(
  res: Response,
  cached?: ProblemDetails | null,
): Promise<ApiError> {
  const problem = cached ?? (await peekProblem(res));
  return new ApiError(
    res.status,
    problem?.type ?? null,
    problem?.title ?? res.statusText,
    problem?.detail ?? "",
  );
}
```

**FRONTEND_API_ERRORS** (`src/api/errors.ts`):

```typescript
/** RFC 7807 Problem Details parsed into a typed error. */
export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly type: string | null, // problem-type URI, e.g. "...probs/rls-hidden"
    public readonly title: string,
    public readonly detail: string,
  ) {
    super(`${status} ${title}: ${detail}`);
    this.name = "ApiError";
  }

  /** Convenience: extract the problem slug (last path segment of the type URI). */
  get problemSlug(): string | null {
    if (!this.type) return null;
    const i = this.type.lastIndexOf("/");
    return i >= 0 ? this.type.slice(i + 1) : this.type;
  }
}
```

**REACT_QUERY_SETUP** (new convention):

```typescript
// NEW FILE: frontend/src/lib/query/client.ts
import { QueryClient, QueryCache } from "@tanstack/react-query";
import { ApiError } from "@/api/errors";

/** Global QueryClient configured for same-origin cookie auth. */
export function makeQueryClient(onUnauthenticated: () => void): QueryClient {
  return new QueryClient({
    queryCache: new QueryCache({
      onError: (err) => {
        if (err instanceof ApiError && err.status === 401) onUnauthenticated();
      },
    }),
    defaultOptions: {
      queries: {
        retry: (count, err) =>
          !(err instanceof ApiError && (err.status === 401 || err.status === 403)) && count < 2,
        staleTime: 30_000,
      },
    },
  });
}
```

**ROUTE_LOADER_PREFETCH** (new convention):

```typescript
// NEW FILE: frontend/src/routes/library.tsx
import { listBooks } from "@/api/books";
import { queryClient } from "@/lib/query/client";

/** Route loader: prefetch the library list so `useQuery` is hot on render. */
export async function loader({ request }: LoaderFunctionArgs) {
  const url = new URL(request.url);
  const params = paramsFromSearch(url.searchParams);
  await queryClient.prefetchQuery({
    queryKey: ["books", params],
    queryFn: ({ signal }) => listBooks(params, signal),
  });
  return null;
}
```

**COMPONENT_USES_USEQUERY**:

```typescript
// NEW: frontend/src/pages/library/LibraryPage.tsx — mirrors the dev HeroLibraryPage layout
const { data } = useSuspenseQuery({
  queryKey: ["books", params],
  queryFn: ({ signal }) => listBooks(params, signal),
});
// render data.items via the same grid markup as src/pages/design/library.tsx
```

**JSDOC_ON_PUBLIC_EXPORTS** (per ESLint rules):

```typescript
/**
 * Fetch the detail of a single book by manifestation id. Returns 404 when the user lacks RLS visibility.
 *
 * Reflects the Tier 1 comment policy: WHY non-obvious behaviour (RLS gating returns 404 not 403)
 * is captured in this docstring per [adr/2026-05-08-tiered-comment-policy.md].
 */
export async function getBook(id: string, signal?: AbortSignal): Promise<BookDetail> {
  /* ... */
}
```

**RUST_DOCSTRING_ON_PUBLIC_FN** (per `#![deny(missing_docs)]`):

```rust
/// List manifestations visible to the current user, with optional filters and cursor pagination.
///
/// # Errors
/// - `AppError::Validation` if the cursor is malformed.
/// - `AppError::Internal` on database errors.
pub async fn list(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, AppError> { /* ... */ }
```

---

## Files to Change (Aggregate Across All Sub-phases)

### Backend — new files

| File                                            | Action                           | Justification                                                              |
| ----------------------------------------------- | -------------------------------- | -------------------------------------------------------------------------- |
| `backend/src/error/problems.rs`                 | CREATE (11a Task 1b)             | RFC 7807 problem-type URI constants + helpers                              |
| `backend/src/security/csrf.rs`                  | CREATE (11a Task 1c)             | OWASP synchronizer token + tower middleware + session integration          |
| `backend/src/routes/library.rs`                 | CREATE                           | All `/api/books`, `/api/works/{id}`, `/api/search` handlers                |
| `backend/src/routes/series.rs`                  | CREATE                           | `GET /api/series/{id}`                                                     |
| `backend/src/routes/shelves.rs`                 | CREATE                           | CRUD `/api/shelves`, `/api/shelves/{id}/items`                             |
| `backend/src/routes/users.rs`                   | CREATE                           | Admin `/api/users`                                                         |
| `backend/src/routes/settings.rs`                | CREATE                           | `GET/PUT /api/settings` (11f)                                              |
| `backend/src/routes/manifestations.rs`          | CREATE (or extend `metadata.rs`) | `PATCH /api/books/{id}/metadata` (11c, RFC 7396 JSON Merge Patch)          |
| `backend/src/services/library.rs`               | CREATE                           | List/detail/search service functions for multi-step orchestration          |
| `backend/src/services/shelves.rs`               | CREATE                           | Shelf items reorder transaction                                            |
| `backend/src/services/settings.rs`              | CREATE (11f)                     | Persisted settings load/save + reload propagation                          |
| `backend/src/models/series.rs`                  | CREATE                           | `Series`, `SeriesWork` types + queries                                     |
| `backend/src/models/shelf.rs`                   | CREATE                           | `Shelf`, `ShelfItem` types + queries                                       |
| `backend/src/models/library.rs`                 | CREATE                           | `BookListRow`, `BookDetail`, `WorkDetail` response DTOs                    |
| `backend/src/models/settings.rs`                | CREATE (11f)                     | `Settings` struct + DB encoding                                            |
| `backend/migrations/{ts}_search_indexes.up.sql` | CREATE (11b if needed)           | Verify trigram indexes on author + manifestation full-text; add if missing |
| `backend/migrations/{ts}_settings.up.sql`       | CREATE (11f)                     | `settings` table                                                           |
| `backend/tests/api_books.rs` and friends        | CREATE                           | Integration tests for each new route group                                 |
| `.github/sqlx-runtime-allowlist.txt`            | UPDATE                           | Add new dynamic-SQL call sites                                             |

### Backend — modified files

| File                          | Action               | Justification                                                                                                                                        |
| ----------------------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `backend/src/routes/mod.rs`   | UPDATE               | Register new modules                                                                                                                                 |
| `backend/src/lib.rs`          | UPDATE               | Wire new routers + CSRF layer into `build_router_with_session_store`                                                                                 |
| `backend/src/error.rs`        | UPDATE (11a Task 1b) | Rewrite `IntoResponse` to emit RFC 7807 `application/problem+json`; add `CsrfMissing`, `CsrfMismatch`, `IfMatchRequired`, `IfMatchMismatch` variants |
| `backend/src/routes/auth.rs`  | UPDATE (11a Task 1c) | Generate CSRF token on session creation; expose via `/auth/me`                                                                                       |
| `backend/src/models/user.rs`  | UPDATE (11a Task 1c) | `/auth/me` response shape includes `csrf_token: String` field                                                                                        |
| `backend/src/test_support.rs` | UPDATE               | Add `assert_problem(response, type_slug, status)` helper; existing tests update to use it                                                            |
| `backend/Cargo.toml`          | UPDATE (11a Task 4)  | Add `axum-extra = { version = "0.10", features = ["query"] }`; add `subtle = "2"` for constant-time CSRF compare                                     |

### Frontend — new files

| File                                               | Action       | Justification                                                                                                                                                                                                                            |
| -------------------------------------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `frontend/src/api/index.ts`                        | CREATE       | Public surface of the API client module                                                                                                                                                                                                  |
| `frontend/src/api/errors.ts`                       | CREATE       | `ApiError` class (RFC 7807 fields: status/type/title/detail)                                                                                                                                                                             |
| `frontend/src/api/csrf.ts`                         | CREATE       | CSRF token state + refresh from `/auth/me`                                                                                                                                                                                               |
| `frontend/src/api/fetch.ts`                        | CREATE       | `apiFetch` wrapper (credentials, RFC 7807 parsing, CSRF token injection + rotation-retry)                                                                                                                                                |
| `frontend/src/api/books.ts`                        | CREATE       | `listBooks`, `getBook`, `getWork`, `updateBookMetadata`                                                                                                                                                                                  |
| `frontend/src/api/search.ts`                       | CREATE (11b) | `searchLibrary` — calls `GET /api/search`                                                                                                                                                                                                |
| `frontend/src/api/series.ts`                       | CREATE (11d) | `getSeries`                                                                                                                                                                                                                              |
| `frontend/src/api/shelves.ts`                      | CREATE (11d) | CRUD                                                                                                                                                                                                                                     |
| `frontend/src/api/users.ts`                        | CREATE (11e) | Admin                                                                                                                                                                                                                                    |
| `frontend/src/api/settings.ts`                     | CREATE (11f) | Settings                                                                                                                                                                                                                                 |
| `frontend/src/lib/query/client.ts`                 | CREATE       | `makeQueryClient`                                                                                                                                                                                                                        |
| `frontend/src/lib/query/keys.ts`                   | CREATE       | Centralised `queryKey` factory: `books.list(params)`, `books.detail(id)`, etc. (mirrors the Tkdodo "query key factory" pattern)                                                                                                          |
| `frontend/src/hooks/useUnauthenticatedRedirect.ts` | CREATE       | Wires `QueryCache.onError` 401 → `navigate('/login')`                                                                                                                                                                                    |
| `frontend/src/routes/library.tsx`                  | CREATE       | Loader + lazy component for `/library` (and `/`)                                                                                                                                                                                         |
| `frontend/src/routes/book.tsx`                     | CREATE       | Loader + lazy component for `/b/:id`                                                                                                                                                                                                     |
| `frontend/src/routes/series.tsx`                   | CREATE (11d) | `/series/:id`                                                                                                                                                                                                                            |
| `frontend/src/routes/admin.tsx`                    | CREATE (11e) | `/admin/users`                                                                                                                                                                                                                           |
| `frontend/src/routes/settings.tsx`                 | CREATE (11f) | `/settings`                                                                                                                                                                                                                              |
| `frontend/src/pages/library/LibraryPage.tsx`       | CREATE       | Production grid+list. Move and de-fixture-ise component pieces from `pages/design/library.tsx`                                                                                                                                           |
| `frontend/src/pages/library/BookGrid.tsx`          | CREATE       | Extracted grid (production)                                                                                                                                                                                                              |
| `frontend/src/pages/library/BookList.tsx`          | CREATE       | Extracted list (production)                                                                                                                                                                                                              |
| `frontend/src/pages/book/BookPage.tsx`             | CREATE       | Production detail (Tabs: Overview / Versions / Activity)                                                                                                                                                                                 |
| `frontend/src/pages/book/VersionsTab.tsx`          | CREATE (11c) | Accept/reject UI per field                                                                                                                                                                                                               |
| `frontend/src/pages/series/SeriesPage.tsx`         | CREATE (11d) |                                                                                                                                                                                                                                          |
| `frontend/src/pages/admin/UsersPage.tsx`           | CREATE (11e) |                                                                                                                                                                                                                                          |
| `frontend/src/pages/settings/SettingsPage.tsx`     | CREATE (11f) |                                                                                                                                                                                                                                          |
| `frontend/src/components/CoverImage.tsx`           | CREATE       | `<img>` wrapping `/api/books/:id/cover/thumb` with `loading="lazy"`, `decoding="async"`, dimension hints, fallback to `CoverArtwork` (use existing dev component but produce a production variant that doesn't ship the fixture palette) |
| `frontend/src/components/CommandPalette.tsx`       | CREATE (11b) | Cmd-K wrapping shadcn `Command` + react-router navigate                                                                                                                                                                                  |
| `frontend/tests/api/*.test.ts`                     | CREATE       | API client tests with `vi.fn()` fetch mocks                                                                                                                                                                                              |
| `frontend/tests/pages/*.test.tsx`                  | CREATE       | Page-level RTL tests using `MemoryRouter` + a test QueryClient                                                                                                                                                                           |

### Frontend — modified files

| File                        | Action      | Justification                                                                                                           |
| --------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------- |
| `frontend/src/App.tsx`      | UPDATE      | Replace empty `<main>` with `<Outlet />` inside the production layout (sidebar + main)                                  |
| `frontend/src/main.tsx`     | UPDATE      | Bootstrap `QueryClientProvider` + production routes; dev-only `designRoutes` import remains gated                       |
| `frontend/package.json`     | UPDATE      | Add `@tanstack/react-query`, `@tanstack/react-query-devtools` (dev), optionally `@tanstack/react-table` (11b list view) |
| `frontend/vite.config.ts`   | UPDATE      | Add `'design'` chunk rule already present; no change. New `manualChunks` for vendor isolation optional                  |
| `frontend/eslint.config.js` | (NO change) | Existing jsdoc warns apply to new exports                                                                               |
| `frontend/components.json`  | (NO change) | Add components via `npx shadcn add data-table command sheet sonner-toast` only if not already present                   |

### Out of scope for Step 11 (explicit)

See "NOT Building" section below.

---

## NOT Building (Scope Limits)

Step 11's blueprint exit criteria are reads + curation + admin + settings. The following are explicitly **out of scope** and must not be added:

- **Reader / book player UI** — Step 11 links to a download. The in-browser reading experience (epub.js or similar) is a future step (not in blueprint).
- **Library health dashboard** — that's Step 12. Step 11 must not pre-build aggregate health queries.
- **Webhook notifications** — Step 13.
- **Real-time updates (SSE/WebSocket)** — All UI uses react-query polling/refetch. No live channel.
- **Multi-user collaboration / sharing** — Each user sees their own library per RLS. No sharing UI.
- **OPDS changes** — Step 9 OPDS is untouched. Step 11 ADDS a parallel JSON surface; it must NOT modify, deprecate, or rename any OPDS route.
- **Mobile native apps** — UI is responsive web only. iPad/tablet tested; phone usable but not the target.
- **Reading state UI beyond display** — `reading_state` table exists but progress writes are owned by the future reader. Step 11 reads `reading_state` for display only.
- **CSV/JSON export, opds-pse extensions, calibre-like plugin system** — None of these.
- **Background job UI** — Enrichment trigger remains an endpoint, no full job dashboard. (Health dashboard in Step 12 will cover.)
- **Token management UI redesign** — Existing `/api/tokens` endpoints stay; UI can be deferred to a sub-phase or another step.
- **i18n** — English only for Step 11; tokens already chosen with monolingual content.
- **`PUT /api/users/:id/password`** — Auth is OIDC-driven; passwords are out of scope.
- **Settings — env override semantics** (11f): when an env var is set, decide whether the persisted setting wins or the env wins. Document explicitly in 11f ADR — do NOT silently merge.

---

## Sub-phase 11a — Foundations + Read List/Detail

**Goal:** Production `/library` and `/b/:id` routes wired to real DB through three new JSON endpoints, with the full frontend data-layer scaffold (`src/api/`, react-query, react-router data mode) in place for subsequent sub-phases to reuse.

### Step-by-Step Tasks (11a)

**TDD discipline (CLAUDE.md hard rule #5):** every endpoint task is `red → green → refactor`. Test task lands the failing test; the next task makes it pass. Do not commit a handler before its test exists.

#### Task 1: ADR — JSON API conventions (industry-standard default per [[feedback_industry_standard_default]])

- **ACTION**: CREATE `adr/{date}-json-api-conventions.md` using the `adr` skill.
- **GOVERNING PRINCIPLE**: Every convention below defaults to an IETF / OWASP / W3C standard. Where Reverie deviates, the ADR captures the deviation explicitly with measurable justification.
- **CAPTURE** (standard → Reverie choice → rationale):

  | Convention               | Industry standard                                                                    | Reverie choice                                                                                               | Rationale                                                                                                                                                                                    |
  | ------------------------ | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
  | Field naming             | RFC 8259 (no opinion) + de-facto JSON-API/Google style: snake_case                   | snake_case                                                                                                   | Matches existing `User` struct (`auth.rs:192-199`) and zero-config serde                                                                                                                     |
  | Date format              | RFC 3339                                                                             | `time::OffsetDateTime` serializing RFC 3339                                                                  | Default serde format for the `time` crate; matches OPDS Atom feeds                                                                                                                           |
  | **Error envelope**       | **RFC 7807 `application/problem+json`**                                              | **RFC 7807** (CHANGED from `{"error": "..."}`)                                                               | See Task 1b below — full migration                                                                                                                                                           |
  | Null shape               | Implicit (Option<T> → null)                                                          | `Option<T>::None` → JSON `null`, NEVER `skip_serializing_if`                                                 | TS contract reads `field: T \| null`, not `field?: T`                                                                                                                                        |
  | Pagination model         | NOT IETF-specified; modern consensus (GitHub v4, Stripe, Slack, Twitter v2) = cursor | Cursor with sort-aware `CursorKey` enum (see 11a Task 4)                                                     | Stable under concurrent inserts (Reverie's enrichment pipeline writes asynchronously); O(log N) per page at scale; offset would degrade at the 50K+ library size we target                   |
  | **Pagination signaling** | **RFC 8288 `Link` header with `rel="next"`, `rel="prev"`, `rel="first"`**            | **RFC 8288 Link header + in-body `next_cursor` field for JS convenience**                                    | Link header is the IETF-canonical signal (matches OPDS Atom `<link rel="next">`, GitHub, Stripe). Body field is a JS-convenience belt-and-braces — `fetch()` doesn't auto-parse Link headers |
  | **CSRF defense**         | **OWASP synchronizer token pattern**                                                 | **Synchronizer token (CHANGED from `X-Requested-With` alone)**                                               | See Task 1c below                                                                                                                                                                            |
  | Existence-not-leaked     | OWASP defense-in-depth                                                               | 404 (not 403) when RLS hides a row                                                                           | Don't leak existence of resources the user can't access                                                                                                                                      |
  | Mutating-verb body shape | RFC 7396 (JSON Merge Patch) for sparse update                                        | RFC 7396 on PATCH endpoints                                                                                  | Standard sparse-update semantics, library support across languages                                                                                                                           |
  | HTTP precondition        | RFC 9110 §13.1 `If-Match` / 412 / 428                                                | `If-Match` on optimistic-concurrency endpoints (e.g. shelf reorder)                                          | See 11d                                                                                                                                                                                      |
  | Content negotiation      | RFC 9110 §12                                                                         | `application/json` default; `application/problem+json` on errors; `application/atom+xml` on OPDS (unchanged) | —                                                                                                                                                                                            |

  Each row in the table that says "CHANGED" represents a deliberate move TOWARDS the industry standard from a previously-inherited divergence (per the principle: deviations require justification; inherited divergence without justification is debt).

- **VALIDATE**: `adr` skill checklist passes; entry added to `adr/README.md` index.

#### Task 1b: Backend — adopt RFC 7807 Problem Details for error envelope

- **ACTION**: REWRITE `backend/src/error.rs::IntoResponse` impl to emit `application/problem+json` per RFC 7807.
- **RESPONSE_SHAPE**:

  ```json
  {
      "type": "https://reverie.example/probs/<problem-slug>",
      "title": "<short human-readable summary>",
      "status": <int>,
      "detail": "<longer human-readable explanation, may include instance-specific info>",
      "instance": "/api/books/abc-123"
  }
  ```

  - `type` URI: stable per error variant (`rls-hidden`, `validation`, `csrf-missing`, `if-match-required`, `if-match-mismatch`, `internal`, `unauthorized`, `forbidden`). Use a Reverie-owned URI prefix — `https://reverie.example/probs/<slug>` placeholder; actual hostname decided in the ADR. URI does NOT need to dereference at first — it's an identifier per RFC 7807 §3.
  - `title` and `status` mirror the status code.
  - `detail` is the caller-visible message (was the old `"error"` field).
  - `instance` is the request path.
  - Content-Type header: `application/problem+json` (not `application/json`).

- **MIGRATE_EXISTING**: every existing `(StatusCode, Json(json!({"error": ...})))` call site in handlers needs to emit the new shape. Centralise via `AppError::into_response()` so handlers continue returning `Err(AppError::...)` and the conversion is one place. Audit grep: `rg '"error":' backend/src` to find any handler bypassing the central path.
- **PROBLEM TYPE REGISTRY**: CREATE `backend/src/error/problems.rs` (or inline in `error.rs`) — one `const` URI per variant. Document each in the ADR with: when it fires, what the `detail` should include, whether `instance` is included.
- **OPDS UNCHANGED**: OPDS routes serve Atom XML; RFC 7807 applies to JSON API only. OPDS error responses (mostly 401/403 with empty body + `WWW-Authenticate`) stay as-is.
- **TESTS**: every existing test that asserts `body["error"]` updates to assert `body["title"]`, `body["status"]`, `body["type"]`. Use a small `assert_problem(response, type_slug, status)` helper in `test_support.rs` to keep test diffs small.
- **VALIDATE**: `cargo test` green across all existing route modules (auth, ingestion, enrichment, metadata, tokens) after migration.

#### Task 1c: Backend — CSRF synchronizer token pattern

- **ACTION**: CREATE `backend/src/security/csrf.rs` implementing the OWASP synchronizer token pattern:
  - On session creation (`auth_session.login(&user)` in `routes/auth.rs::callback`), generate a 32-byte random token via `rand::rngs::OsRng`, base64url-encode, and store in the session under the key `csrf_token`.
  - Expose a way for the frontend to read the token: ADD `csrf_token` field to the `/auth/me` JSON response (or — alternative — return it via a dedicated `GET /api/csrf-token` endpoint; ADR picks one). Recommend folding into `/auth/me` since the frontend already calls it on mount via `ThemeProvider`.
  - Add tower middleware layer `csrf_required` applied to every non-safe-verb (POST/PUT/PATCH/DELETE) request under `/api/*`. Layer reads `X-CSRF-Token` header; if absent → 428 `{"type": ".../csrf-missing", "status": 428, ...}`; if present but doesn't match `session.get::<String>("csrf_token")` → 403 `{"type": ".../csrf-mismatch", ...}`. Constant-time string compare (`subtle::ConstantTimeEq` or `ring::constant_time::verify_slices_are_equal`).
  - On `POST /auth/logout`, the token is destroyed with the session.
  - Token rotation: rotate on privilege change (role bump) by regenerating the token when `session_version` increments.
- **FRONTEND**: extend `src/lib/theme/api.ts` to also read `csrf_token` from `/auth/me` response; cache in module-level state; inject `X-CSRF-Token: <token>` header on every mutating-verb request via `apiFetch`. On 403-csrf-mismatch response, re-fetch `/auth/me` to get the rotated token, retry once.
- **DEFENSE-IN-DEPTH**: keep SameSite=Lax cookie attribute (already set); keep CSP API layer; the synchronizer token is the PRIMARY CSRF defense, the others are belt-and-braces.
- **WHITELIST**: `POST /auth/logout` is exempt (no session yet for newly-logged-out users; pre-logout the request would have a valid token; documented in ADR). All OIDC callback paths are GET-only — no exemption needed.
- **WIRE**: CSRF layer goes in `backend/src/lib.rs::build_router_with_session_store` between the `/api` route group and the API CSP layer.
- **TESTS**: required matrix —
  - happy: POST /api/\* with valid `X-CSRF-Token` → 200
  - negative: POST /api/\* with no header → 428
  - negative: POST /api/\* with wrong token → 403 (constant-time-checked path)
  - negative: GET /api/books WITHOUT token → 200 (safe verb, not gated)
  - edge: token rotates on role change — old token returns 403 after rotation
  - edge: logout destroys token — POST after logout returns 401 (auth layer fires first)
- **VALIDATE**: `cargo test -p reverie csrf` GREEN.

#### Task 2: Backend — `BookListRow`, `BookDetail`, `WorkDetail` response models

- **ACTION**: CREATE `backend/src/models/library.rs`.
- **IMPLEMENT**: Three serde-deriving structs. Mirror existing `Serialize` patterns (`models/user.rs:41-68`). All fields snake_case, no `rename_all`. Nullable fields use `Option<T>` with no `skip_serializing_if` (matches `User` pattern).

  ```rust
  #[derive(Debug, Serialize, sqlx::FromRow)]
  pub struct BookListRow {
      pub id: Uuid,
      pub work_id: Uuid,
      pub title: String,
      pub created_at: OffsetDateTime,
      pub isbn_13: Option<String>,
      // ... plus serializable status enums + authors loaded separately
  }
  ```

- **GOTCHA**: `authors: Vec<String>` cannot come from a single `sqlx::query!` join (one-to-many); batch-load separately via `ANY($1::uuid[])` like `routes/opds/library.rs:707-754`.
- **VALIDATE**: `cargo check -p reverie` from `backend/`.

#### Task 3 (RED): Backend — failing integration test for `GET /api/books`

- **ACTION**: CREATE `backend/src/routes/library/tests.rs` (module-local per existing OPDS convention) with the **list-endpoint** tests. Tests reference the not-yet-existing handler and must compile-fail first (or use `#[ignore]` until step 4 lands — prefer compile-fail to keep the red light obvious).
- **IMPLEMENT**:
  - happy: admin lists books they own (envelope shape, item count, cursor present when overflow)
  - happy: adult lists, sees only RLS-visible manifestations
  - happy: child lists, sees only manifestations shelved on their own shelves (`create_child_user_and_basic_auth` + `create_shelf` + `add_to_shelf`)
  - negative: unauthenticated → 401 `{"error": "unauthorized"}`
  - negative: malformed cursor → 422 `{"error": "invalid cursor"}`
  - edge: `?sort=title` orders alphabetically
- **MIRROR**: `backend/src/routes/opds/tests.rs:73-99`.
- **VALIDATE**: `cargo test -p reverie list_endpoint` — RED.

#### Task 4 (GREEN): Backend — sort-aware cursor + `GET /api/books` handler

- **ACTION_A**: CREATE migration `backend/migrations/{ts}_idx_works_sort_title.up.sql` (+ corresponding `.down.sql`):

  ```sql
  CREATE INDEX IF NOT EXISTS idx_works_sort_title_id
    ON works (sort_title, id);
  ```

  Required to make `sort=title` cursor pagination index-friendly.

- **ACTION_B**: EXTEND `backend/src/routes/opds/cursor.rs` (or fork as `backend/src/routes/cursor.rs` if OPDS module-locality is enforced) to support a tagged enum:

  ```rust
  pub enum CursorKey {
      Recent { created_at: OffsetDateTime, id: Uuid },
      Title  { sort_title: String,         id: Uuid },
      Author { sort_name: String,          id: Uuid },
  }
  ```

  Base64url payload encodes the variant tag as the first byte (`r`/`t`/`a`) followed by the key bytes. Reject mismatched variant (e.g. `?sort=title&cursor=<recent-tagged>`) with `AppError::Validation("cursor sort mismatch")`. The OPDS path keeps using only the `Recent` variant — backwards-compat via tag byte. Author sort: defined as `sort_name` of `work_authors.position = 0` (i.e. the first author); join via subquery.

- **ACTION_C**: CREATE `backend/src/routes/library.rs` (or `routes/library/mod.rs`) with `pub fn router() -> Router<AppState>` exposing `GET /api/books`.
- **IMPLEMENT**: `list` handler per JSON_GET_HANDLER pattern. `Query<ListParams>` with optional `cursor`, `sort` (`recent` default, `title`, `author`). `QueryBuilder` switches ORDER BY + cursor predicate per sort mode:
  - `recent` → `ORDER BY m.created_at DESC, m.id DESC` + cursor predicate `(m.created_at, m.id) < ($1, $2)`
  - `title` → `ORDER BY w.sort_title ASC, w.id ASC` + cursor predicate `(w.sort_title, w.id) > ($1, $2)`
  - `author` → join `LATERAL (SELECT a.sort_name FROM work_authors wa JOIN authors a ON a.id = wa.author_id WHERE wa.work_id = w.id ORDER BY wa.position ASC LIMIT 1) first_author` + `ORDER BY first_author.sort_name ASC, w.id ASC` + cursor predicate `(first_author.sort_name, w.id) > ($1, $2)`
- **MIRROR**: `backend/src/routes/opds/library.rs:263-356` (`emit_new`) for the RLS tx + batch-load envelope.
- **DEP**: Use `axum_extra::extract::Query` (NOT built-in `axum::Query`) for `ListParams` to make 11b's multi-value filter params (`?tag=a&tag=b`) drop in without rewriting handlers. Add `axum-extra = { version = "0.10", features = ["query"] }` (verify against axum 0.8.9) to `backend/Cargo.toml`.
- **PAGE_SIZE**: reuse `state.config.opds.page_size`; add `REVERIE_API_PAGE_SIZE` env var in a follow-up if divergence wanted.
- **DOCSTRING**: `///` with `# Errors` section per Tier 1.
- **REGISTER**: in `backend/src/routes/mod.rs` and merge into router at `backend/src/lib.rs`.
- **RUNTIME_SQL_ALLOWLIST**: add the new `QueryBuilder::build_query_as` call sites to `.github/sqlx-runtime-allowlist.txt`.
- **VALIDATE**: `cargo test -p reverie list_endpoint` — GREEN. Add cursor-variant unit tests (encode → decode round-trip per variant; mismatched-variant rejection).

#### Task 5 (RED): Backend — failing test for `GET /api/books/{id}`

- **ACTION**: ADD `detail_endpoint` tests in `routes/library/tests.rs`:
  - happy: returns book with version_summary
  - edge: hidden id → 404 (RLS, not 403 — existence not leaked)
  - negative: malformed UUID in path → 400 (axum default Path rejection)
- **VALIDATE**: `cargo test -p reverie detail_endpoint` — RED.

#### Task 6 (GREEN): Backend — `GET /api/books/{id}` handler

- **ACTION**: ADD `detail` handler in `routes/library.rs`.
- **IMPLEMENT**: Single-row fetch via `sqlx::query_as!` (no dynamic clauses); include metadata version count via subquery; return 404 when 0 rows.
- **RESPONSE**: `BookDetail { manifestation fields, work fields, authors, series, tags, ingestion_status, validation_status, enrichment_status, metadata_version_summary: { pending: u32, accepted: u32 } }`.
- **VALIDATE**: `cargo test -p reverie detail_endpoint` — GREEN.

#### Task 7 (RED → GREEN): Backend — `GET /api/works/{id}` (with RLS-existence gate)

- **RED**: ADD `work_endpoint` tests:
  - happy: returns work + all its manifestations grouped (admin/adult who can see at least one manifestation)
  - edge: hidden work id (nonexistent) → 404
  - **SECURITY (must-pass):** child user requests a work whose manifestations aren't on the child's shelves → 404 (NOT 200 with empty manifestations array). Existence-not-leaked invariant — `works` table has no RLS so handler MUST gate explicitly.
- **GREEN**: ADD `work_detail` handler. Under the RLS transaction, first run:

  ```sql
  SELECT EXISTS (SELECT 1 FROM manifestations WHERE work_id = $1) AS visible
  ```

  This query is RLS-filtered (RLS lives on `manifestations`), so it returns `false` for users who can't see any manifestation of the work. If `visible = false` → return 404 immediately, BEFORE fetching work metadata. If `visible = true` → fetch the work row (no RLS on `works`, but visibility already established), fetch manifestations (RLS-filtered), batch-load authors via `ANY`.

- **VALIDATE**: `cargo test -p reverie work_endpoint` GREEN, full `cargo test` green.

#### Task 8: Backend — cargo fmt / clippy / sqlx round-trip

- **VALIDATE**: `cargo fmt --check && cargo clippy -p reverie -- -D warnings && cargo sqlx prepare --workspace --check` (per project memory `feedback_run_cargo_fmt_check.md`).

#### Task 9: Frontend — add `@tanstack/react-query` + devtools

- **ACTION**: `npm --prefix frontend install @tanstack/react-query@^5 && npm --prefix frontend install --save-dev @tanstack/react-query-devtools@^5`.
- **VERIFY**: `frontend/package-lock.json` updated, npm only (per memory). No `pnpm-lock.yaml` etc.

#### Task 10: Frontend — failing test for `apiFetch` + `listBooks` (RED)

- **ACTION**: CREATE `frontend/tests/api/books.test.ts` mocking `global.fetch`. Assertions: includes `credentials: 'same-origin'`, `X-Requested-With: XMLHttpRequest` header, throws `ApiError(401)` on 401 response, throws `ApiError(500)` on 500.
- **VALIDATE**: `npm test api/books` — fails because `src/api/*` not yet present.

#### Task 11: Frontend — `src/api/` scaffold (GREEN)

- **ACTION**: CREATE `src/api/errors.ts`, `src/api/fetch.ts`, `src/api/books.ts`, `src/api/index.ts`.
- **IMPLEMENT**:
  - `ApiError` class with `status: number` and `body: unknown`.
  - `apiFetch(input, init?)` wrapping `fetch` with `credentials: 'same-origin'`, `'Accept': 'application/json'`, `'X-Requested-With': 'XMLHttpRequest'`. Throws `ApiError` on non-2xx.
  - `listBooks(params, signal?)`, `getBook(id, signal?)`, `getWork(id, signal?)`.
- **DOCSTRING**: JSDoc on every exported function/type per ESLint `jsdoc/require-jsdoc` warn → ratchet to error (Stage D).
- **VALIDATE**: `cd frontend && npm run test -- api/books` GREEN, plus `npm run lint`.

#### Task 12: Frontend — `QueryClient` + `QueryClientProvider`

- **ACTION**: CREATE `src/lib/query/client.ts` (`makeQueryClient`), `src/lib/query/keys.ts` (key factory), modify `src/main.tsx` to wrap the router in `<QueryClientProvider>`.
- **PATTERN**: `QueryCache({ onError })` for 401 redirect (per research). 401 redirect target = `/login` (= `/auth/login` proxy passthrough; the navigation triggers the existing OIDC flow).
- **DEVTOOLS**: dev-only via `import.meta.env.DEV` dynamic import — same pattern as `designRoutes`.
- **VALIDATE**: `npm run build` succeeds; type-check passes.

#### Task 13: Frontend — react-router data mode + production routes

- **ACTION**: REPLACE `RouteObject[]` in `src/main.tsx` with `createBrowserRouter(...)`. Add production layout shell and child routes.
- **ROUTES (11a only)**:
  - `/` → redirect to `/library`
  - `/library` → `LibraryPage` (loader prefetches `books.list({})`)
  - `/b/:id` → `BookPage` (loader prefetches `books.detail(id)`)
- **LAZY**: use `lazy: { loader, Component }` form per react-router v7.15 to allow loader to start before component bundle arrives.
- **DEV_ONLY_DESIGN_ROUTES**: keep the existing `if (import.meta.env.DEV)` block — designRoutes get pushed into the router children, not replaced.
- **VALIDATE**: `npm run build` succeeds; design routes still tree-shaken out of prod bundle (verify `dist/assets/*` does not contain `HeroLibraryPage`).

#### Task 14: Frontend — `LibraryPage` (RTL test red → component green)

- **ACTION**: CREATE `src/pages/library/LibraryPage.tsx`. Extract reusable bits (`BookCard`, `BookRow`, header, search input) into `src/pages/library/components/`. Move shared pieces from `src/pages/design/library.tsx`'s component tree into `src/pages/library/components/` as the new home; the dev page imports from there with the fixture data.
- **IMPLEMENT**: `useSuspenseQuery` keyed on `['books', 'list', params]`. Render grid (default) and list view (`?view=list`). Empty state. Skeleton via shadcn `<Skeleton>` for `<Suspense>` fallback. Pagination via "Load more" button using react-query infinite-query or manual cursor state — recommend infinite-query for simplicity (`useInfiniteQuery`).
- **COVER IMAGES**: `<img src={`/api/books/${book.id}/cover/thumb`} loading="lazy" decoding="async" />`. Fallback to a typographic placeholder when 404 (`onError` swap). Production `CoverImage` component handles this.
- **PRESERVE_VISUAL_TARGET**: the layout, spacing, fonts, tokens all stay identical to the dev hero. The diff is data-source, not pixels.
- **ROUTE_PARAMS**: `?view=grid|list`, `?sort=recent|title|author`, `?cursor=...`. URL is source of truth; component state mirrors URL via `useSearchParams`.
- **TDD ORDER**: write `frontend/tests/pages/library/LibraryPage.test.tsx` first (render inside a `MemoryRouter` with a stubbed `QueryClient` seeded with fixture data, assert grid renders 3 covers and "Load more" appears when `next_cursor` is set) — RED. Then implement the component — GREEN.
- **VALIDATE**: `npm test`; visit `localhost:5173/library` against the dev backend, screenshot via `agent-browser` and visually compare against `/design/hero/library`.

#### Task 15: Frontend — `BookPage` (RTL test red → component green)

- **TDD ORDER**: RTL test first — render with seeded QueryClient, assert title + author + Tabs (Overview / Versions / Activity) — RED. Then implement: mirror `src/pages/design/book.tsx` layout (sticky cover aside + Tabs).
- **VERSIONS_TAB**: 11a renders read-only metadata version list (rows from `metadata_versions` joined to canonical pointers); accept/reject buttons land in **11c**.
- **BROWSER_QA**: agent-browser screenshot vs hero (per project memory `feedback_use_browser_for_design_critique.md`).
- **VALIDATE**: `npm test`; `npm run build`; `npm run lint`.

#### Task 16: Frontend — auth boundary (login redirect on 401)

- **ACTION**: Wire `makeQueryClient` to a `useNavigate`-based redirect using `react-router`. Because `QueryClient` is module-scoped, expose an `onUnauthenticated` setter that `App` sets once `useNavigate` is available.
- **TEST**: assert `QueryCache.onError` of an `ApiError` 401 calls the provided callback.
- **VALIDATE**: `npm test`.

### Validation Commands (11a)

```bash
# Backend
cd backend && cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo sqlx prepare --workspace --check

# Frontend
cd frontend && npm run lint && npm run test && npm run build

# Browser QA (project memory: use browser before declaring UI critique done)
# Probe http://localhost:5173 (persistent), screenshot /library and /b/:id, compare to /design/hero/{library,book}
```

### Exit Criteria (11a)

- All four list/detail tests pass under `cargo test`.
- `localhost:5173/library` renders real books from the dev DB with covers (verify with agent-browser screenshot).
- `localhost:5173/b/:id` shows real metadata + version count.
- No regressions in OPDS feed tests.
- ADR landed.
- New API endpoint count: 3 (`GET /api/books`, `GET /api/books/{id}`, `GET /api/works/{id}`).

---

## Sub-phase 11b — Search + Filters

**Goal:** Add full-text search and filterable list params. UI: Cmd-K command palette + filter chips on library page.

### Task overview (11b)

1. ADD optional filter params to `ListParams` on `/api/books`: `author: Option<Uuid>`, `series: Option<Uuid>`, `shelf: Option<Uuid>`, `tag: Option<Vec<String>>` (multi-value: `?tag=a&tag=b`). Extractor is `axum_extra::extract::Query` (committed in 11a Task 4 — no decision needed here).
2. ADD `GET /api/search?q=` per Blueprint task 7. Mirror `routes/opds/library.rs:626-703` (`emit_search`) for the RLS tx + envelope. Use `websearch_to_tsquery` (NOT `plainto_tsquery`) for the user input — better operator handling (`"phrase"`, `-exclude`, `OR`).
3. **Hybrid search strategy (index-friendly, NOT a single OR predicate):**

   ```sql
   WITH ts_hits AS (
       SELECT w.id, ts_rank_cd(w.search_vector, q, 32) AS rank, 'ts' AS hit_kind
       FROM works w, websearch_to_tsquery('english', $1) q
       WHERE w.search_vector @@ q
   ), trgm_hits AS (
       SELECT w.id, similarity(w.title, $1) * 0.5 AS rank, 'trgm' AS hit_kind
       FROM works w
       WHERE w.title % $1                       -- uses GIST trigram index
         AND similarity(w.title, $1) > 0.3
   ), merged AS (
       SELECT id, MAX(rank) AS rank FROM (SELECT * FROM ts_hits UNION ALL SELECT * FROM trgm_hits) u GROUP BY id
   )
   SELECT m.id, m.created_at, ..., merged.rank
   FROM merged JOIN works w ON w.id = merged.id JOIN manifestations m ON m.work_id = w.id
   ORDER BY merged.rank DESC, m.id ASC
   LIMIT $2;
   ```

   Each CTE uses its native index (GIN on tsvector, GIST on trigram). Single-OR predicate would force the planner to bitmap-or scan; UNION ALL keeps each plan index-friendly. The `%` operator triggers the trigram index when `pg_trgm.similarity_threshold` ≥ 0.3 (default 0.3 — verify session setting or use `SET pg_trgm.similarity_threshold = 0.3` per-tx if needed).

4. CONFIRM tsvector GIN index + trigram GIST indexes exist (migration `20260412150007:1-7`). Trigram index on `works.title` exists; if `authors.name` and `series.name` trigram indexes already exist (per migration 20260412150007:5-6), the hybrid extends to author/series search trivially in a follow-up.
5. ADD `ts_headline` for snippet highlighting; pass non-HTML delimiters (`StartSel=‹, StopSel=›`) and render via component, NOT `dangerouslySetInnerHTML`.
6. FRONTEND: `src/components/CommandPalette.tsx` using shadcn `CommandDialog`. Global Cmd-K binding via `useEffect`. Inside the dialog: query `/api/search?q=` debounced 200ms; results group by Books / Authors / Series; on select, `navigate('/b/:id')` or `/series/:id`.
7. FRONTEND: shelf chips and series filter on `LibraryPage` — toggle `?shelf=` and `?series=` params, react-query refetches on key change. `aria-pressed` Buttons (mirrors hero pattern).
8. **TEST MATRIX (RED → GREEN per endpoint, mirrors 11a discipline):**
   - happy: exact-match search returns ranked results (most-relevant first)
   - happy: typo-tolerant — search "Hemingwy" finds "Hemingway"
   - happy: `websearch` operators — `"war and peace"` (quoted phrase), `tolstoy -anna` (exclude) work as expected
   - happy: filter `?author=<id>` returns only that author's books
   - happy: multi-tag `?tag=scifi&tag=hugo` AND-matches (or OR — decide and document)
   - negative: empty `q` → 422 `{"error": "query required"}`
   - negative: oversized `q` (>200 chars) → 422
   - negative: SQL-injection probe (`'); DROP TABLE works;--`) safely escaped via parameterised query — assert table still exists post-test
   - edge: child-account search returns only manifestations on their shelves (RLS join)
   - edge: empty result set → empty `items` array, `next_cursor: null`
   - **PERF GATE:** add `backend/tests/perf_search.rs` (or `#[ignore]`d test) that seeds 10K books and asserts p50 < 200ms. Run in CI nightly, not on every PR — flag in 11b PR description.

### Exit Criteria (11b)

- Search results returned within 200ms median for the dev library (10K books fixture).
- Cmd-K works on every route.
- All shelf/series/author filter params produce correct RLS-respecting results.
- `bg-black` overlay fix landed (since CommandDialog uses a Radix Dialog overlay — pick up the deferred fix here per project memory).

---

## Sub-phase 11c — Manual Metadata Edit + Accept/Reject UI

**Goal:** Surface metadata version review to operators.

### Task overview (11c)

1. ADD `PATCH /api/books/{id}/metadata` accepting `Json<UpdateMetadataRequest>` shaped per **RFC 7396 JSON Merge Patch**:
   - Missing key in body → field unchanged.
   - Key present with non-null value → field set to that value (INSERT new `metadata_versions` row with `source = 'manual'`, update canonical pointer).
   - Key present with `null` → field cleared (delegate to existing `clear_field` in `routes/metadata.rs`).
   - Encode in Rust via `serde_with::rust::double_option` (or hand-rolled `Option<Option<T>>` deserializer): `None` = absent, `Some(None)` = clear, `Some(Some(v))` = set.
   - Frontend TS contract: `{ fields: { [key: string]: string | null } }`. Omit keys you don't want to touch; pass `null` to clear.
2. Field-dispatch identical to `apply_version` (`routes/metadata.rs:391-502`).
3. REUSE existing `POST /api/manifestations/{id}/metadata/{accept,reject,revert}` for per-row buttons.
4. ADD `GET /api/manifestations/{id}/metadata` is already there; expose pending rows in `BookDetail.metadata_versions` for the Versions tab.
5. FRONTEND: `VersionsTab.tsx` — per-field rows: current canonical | pending alternatives | accept/reject/revert buttons. Use react-query mutations with optimistic update + invalidation of `['books', 'detail', id]`.
6. FRONTEND: edit form (modal sheet) — form fields per metadata field. Submit calls `PATCH /api/books/{id}/metadata` with only the touched fields. Confirm with shadcn `<AlertDialog>` before applying when the change would clear a previously-set field. Form state tracks `touched` flag per field so the request body only contains user-modified keys.
7. TESTS: PATCH with `{ fields: { title: "New" } }` updates only title; PATCH with `{ fields: { description: null } }` clears description and creates an audit-trail `metadata_versions` row with `new_value = null`; PATCH with empty body is 422 (`{"error": "no fields"}`); child accounts blocked (403); writeback job enqueued on every accept/manual edit; manual edit on a field with a pending AI draft accepts the manual value and leaves the AI draft `status = 'pending'` (don't auto-reject — operator may later revert to it).

### Exit Criteria (11c)

- Operators can accept, reject, revert, and manually edit metadata from `/b/:id`.
- `manifestations.title_version_id` (and friends) reflect manual edits.
- `writeback_jobs` receives an entry on every accept/manual edit.
- No regression: existing `routes/metadata.rs` tests still pass.

---

## Sub-phase 11d — Series + Shelves CRUD

**Goal:** Series page + shelf management.

### Task overview (11d)

1. ADD `GET /api/series/{id}` returning the series + ordered list of works/manifestations (`series_works.position`). Apply the same RLS-existence gate as `GET /api/works/{id}` (return 404 if no manifestation across all works in the series is visible to the current user — prevents existence-leak for child accounts).
2. ADD migration `{ts}_shelves_updated_at.up.sql` (if `shelves.updated_at` column doesn't already exist — verify; many tables already have it): `ALTER TABLE shelves ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now()` + a `BEFORE UPDATE` trigger setting it to `now()`.
3. ADD CRUD for shelves: `GET /api/shelves` (list user's shelves), `POST /api/shelves` (create), `PATCH /api/shelves/{id}` (rename), `DELETE /api/shelves/{id}` (cannot delete if `is_system = true`).
4. ADD shelf items:
   - `POST /api/shelves/{id}/items` (add manifestation — appends at `max(position) + 1`)
   - `DELETE /api/shelves/{id}/items/{manifestation_id}` (remove)
   - `PUT /api/shelves/{id}/items` (reorder — accepts full ordered array, transactionally rewrites `shelf_items.position`). **Optimistic-concurrency gate:** require `If-Match: "<shelves.updated_at as RFC3339>"` header. Inside the transaction: `SELECT updated_at FROM shelves WHERE id = $1 FOR UPDATE`; if it doesn't match the `If-Match` value, return 412 Precondition Failed `{"error": "shelf modified"}`. On success, `UPDATE shelves SET updated_at = now() WHERE id = $1` and return the new ETag in the response.
   - Response on every shelf read endpoint includes `ETag: "<updated_at as RFC3339>"` so frontend can capture and pass back on PUT.
5. FRONTEND: `SeriesPage.tsx` — ordered list with completeness indicator (own/total).
6. FRONTEND: shelves sidebar with create/rename/delete; drag-and-drop reorder via `@dnd-kit/sortable` (small additional dep). Optimistic update via react-query mutation. On 412, react-query rollback + toast "Shelf changed on another device — refresh and retry."
7. **TEST MATRIX (RED → GREEN, mirrors 11a discipline):**
   - happy: GET /api/shelves returns user's shelves
   - happy: POST + PATCH + DELETE round-trip
   - happy: PUT items with correct `If-Match` reorders successfully and returns new ETag
   - negative: DELETE on `is_system = true` shelf → 409 `{"error": "system shelf"}`
   - negative: ownership boundary — adult A cannot mutate adult B's shelf (404, NOT 403, existence-not-leaked)
   - negative: child cannot CREATE/DELETE shelves (403)
   - negative: child CAN view their own shelves (200)
   - **CONCURRENCY:** two parallel PUT reorders with the same `If-Match` — exactly one succeeds (200), one returns 412
   - edge: PUT items with stale `If-Match` → 412
   - edge: PUT items without `If-Match` header → 428 Precondition Required `{"error": "if_match_required"}` (RFC 6585)

### Exit Criteria (11d)

- All shelf CRUD works from the UI.
- Drag-to-reorder persists across refresh.
- Series page shows correct completeness.
- Child accounts can VIEW their own shelves (system shelves + any assigned) but cannot CREATE/DELETE.

---

## Sub-phase 11e — Admin

**Goal:** User management UI gated on `role = admin`.

### Task overview (11e)

1. ADD `GET /api/users` (admin only — `current_user.require_admin()?`) returning all users.
2. ADD `PUT /api/users/{id}/role` (admin only). **Last-admin protection (TOCTOU-safe):** inside the transaction, BEFORE the UPDATE, run `SELECT id FROM users WHERE role = 'admin' FOR UPDATE`. Recount the locked rows; reject (422 `{"error": "would leave zero admins"}`) if the demotion would drop the admin count to zero. Under READ COMMITTED, two concurrent demotions serialize on the lock — second transaction sees the updated count after the first commits. Bumping a user's role bumps `users.session_version` (in the same transaction) to invalidate their existing sessions. Test must include a concurrency case: two admins simultaneously POST demote-the-other; assert exactly one succeeds and one returns 422.
3. ADD `PUT /api/users/{id}/child-status` (admin only). When toggling `is_child` ON, the `chk_child_role_sync` CHECK constraint requires `role = 'child'`; do both updates in one transaction.
4. ADD `PATCH /api/users/{id}` (admin only) for display_name + email edits.
5. FRONTEND: `src/routes/admin.tsx` — guard: render redirect if `useQuery(['auth', 'me']).data.role !== 'admin'`. Otherwise show table.
6. FRONTEND: `UsersPage.tsx` — shadcn data table; role dropdown per row; child-status toggle; audit log link (deferred — track in debt).
7. **TEST MATRIX (RED → GREEN, mirrors 11a discipline):**
   - happy: admin GETs /api/users, sees full list with role + is_child
   - happy: admin promotes adult → admin (session_version bump invalidates target's existing session — assert by making a request with the target's old session cookie and expecting 401)
   - happy: admin demotes admin → adult when ≥2 admins exist
   - happy: admin toggles is_child ON (also sets role='child' in same transaction)
   - happy: PATCH /api/users/{id} updates display_name and email (admin only)
   - negative: adult GETs /api/users → 403
   - negative: child GETs /api/users → 403
   - negative: non-admin PUT /api/users/{id}/role → 403
   - **SECURITY (last-admin):** single concurrent test: spawn two parallel demote-the-other transactions; assert exactly one succeeds, one returns 422 `{"error": "would leave zero admins"}`
   - negative: admin demoting self when sole admin → 422
   - negative: PATCH with email already in use by another user → 422
   - edge: `chk_child_role_sync` enforced — PUT role='adult' on a `is_child = true` user without first toggling child off → 422 surfacing the CHECK violation
   - edge: PATCH /api/users/{nonexistent-uuid} → 404

### Exit Criteria (11e)

- Admins can change roles and toggle child status.
- Last-admin-protection holds under concurrency.
- Demoting a user bumps their session_version (kicks them out next request).
- Non-admins cannot see the `/admin` route or hit the endpoints.

---

## Sub-phase 11f — Settings (Persisted)

**Goal:** Operator-tunable settings persisted to DB + live-reload to running app.

**Gated on:** ADR for persisted settings (precedence between env vars and DB-persisted values; reload mechanism: SIGHUP, internal channel, or per-request lookup).

### Task overview (11f)

1. **ADR** — write `adr/{date}-persisted-settings.md`. Decisions to make:
   - Storage: single-row table (`settings` with one row) vs key-value table.
   - Precedence: env var beats DB beats default, or DB beats env? Recommend env beats DB (env stays the deploy override; DB is the runtime knob). Document.
   - Reload: per-request `SELECT FROM settings` vs in-process cache invalidated by a tokio broadcast channel + listen/notify.
2. ADD migration `{ts}_settings.up.sql` — single-row table with a `singleton CHECK (id = TRUE)` invariant.
3. ADD `models/settings.rs` + `services/settings.rs` (load, save, reload).
4. ADD `GET /api/settings` (admin only) returning effective config (env + DB resolved, with provenance per field).
5. ADD `PUT /api/settings` (admin only). Validate each field against existing config-validation rules in `config.rs`; reject on invalid.
6. WIRE reload: per the ADR's choice. Simplest is per-request `SELECT` cached in `AppState` behind a `tokio::sync::RwLock`; complexity acceptable for OSS deploy. Avoid LISTEN/NOTIFY for the MVP.
7. FRONTEND: `SettingsPage.tsx` — form with shadcn `Field` + `Input`. Show provenance: "(env override)" badge on fields that env wins.
8. **TEST MATRIX (RED → GREEN, mirrors 11a discipline):**
   - happy: admin GET /api/settings returns effective config with per-field provenance
   - happy: admin PUT /api/settings updates one field; subsequent GET returns updated value; running app reflects new value (per the ADR's reload mechanism — e.g. next request hits the new value in `AppState`)
   - happy: PUT with valid `format_priority` array reorders and persists
   - negative: non-admin GET → 403
   - negative: non-admin PUT → 403
   - negative: PUT with invalid value (e.g. `concurrency = -1`) → 422 with field-level diagnostic (per the ADR's decision on single-error vs field-level envelope shape — defaults to single-error per existing pattern unless ADR overrides)
   - edge: env-set field PUT — behavior per ADR (env-wins → 200 returns `effective_value` from env, `db_value` reflects stored attempt; OR rejection 409 `{"error": "field locked by env"}`). Test asserts whichever the ADR chose
   - edge: PUT empty body → 422
   - edge: ADR-mandated reload semantics — for live-reloadable fields, assert effect visible within N ms; for restart-required fields, assert response includes `"restart_required": true` flag

### Exit Criteria (11f)

- Admin can change path template, format priority, enrichment config, cover config from the UI.
- Changes take effect without restart for fields that are runtime-reloadable; other fields surface a "restart required" badge in the UI.
- env-set fields are visibly marked.
- ADR landed before merge.

---

## Testing Strategy

### Test layers per sub-phase

| Layer                          | What                                                                                        | Tooling                                                                                  |
| ------------------------------ | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Backend unit (model functions) | `match_existing`-style helpers                                                              | `#[sqlx::test(migrations = "./migrations")]`                                             |
| Backend integration (HTTP)     | Each endpoint, RLS variants, error envelopes                                                | `#[sqlx::test]` + `axum_test::TestServer` via `test_support::db::server_with_real_pools` |
| Frontend unit (api client)     | `apiFetch`, `listBooks`, etc. — happy + 4xx + 5xx                                           | `vitest` + `vi.fn()` `fetch` mocks                                                       |
| Frontend unit (hooks)          | `useUnauthenticatedRedirect`                                                                | `vitest` + RTL `renderHook`                                                              |
| Frontend component             | Pages render with seeded `QueryClient`; snapshot via inline assertions, not toMatchSnapshot | `vitest` + RTL + `MemoryRouter`                                                          |
| Browser QA                     | Real-DB visual check vs hero                                                                | `agent-browser` screenshots + `localhost:5173` (persistent)                              |

### Edge cases (must cover at least once across sub-phases)

- [ ] Cursor base64 corruption → 422 with `{"error": "invalid cursor"}`
- [ ] Empty result page → empty `items` array, `next_cursor: null`
- [ ] Same `(created_at, id)` boundary (no overlap)
- [ ] RLS: adult cannot see admin's `manifestations` not on a shared shelf (works by default; verify)
- [ ] RLS: child sees only manifestations on their own shelves
- [ ] Search: empty query string → 422 (per OPDS pattern at `library.rs:201-208`)
- [ ] Search: SQL-injection-style input quoted by `plainto_tsquery` — verify not exploitable
- [ ] Metadata PUT with non-existent field name → 422
- [ ] Demote last admin → 422
- [ ] Non-admin hits `/api/users` → 403 envelope
- [ ] CSRF: mutating verb without `X-Requested-With` header → ADR decision (could be 403 or accept-but-warn — document in 11a ADR)
- [ ] 11f: env-overridden field PUT → 200 with `effective_value` from env (per ADR)

---

## Validation Commands

### Level 1: Static Analysis

```bash
# Backend
cd backend && cargo fmt --check && cargo clippy -- -D warnings

# Frontend
cd frontend && npm run lint && npm run stylelint && npm run detect
```

**EXPECT**: exit 0, no warnings.

### Level 2: Unit + Integration Tests

```bash
# Backend
cd backend && cargo test
# Frontend
cd frontend && npm test
```

**EXPECT**: all pass.

### Level 3: Full Suite + Build

```bash
cd backend && cargo build --release && cargo sqlx prepare --workspace --check
cd frontend && npm run build
```

**EXPECT**: builds succeed; `cargo sqlx prepare --workspace --check` reports no drift.

### Level 4: Database Validation

For each sub-phase that adds migrations:

- [ ] migration up/down round-trips clean (`sqlx migrate revert` then `sqlx migrate run`)
- [ ] new indexes used by `EXPLAIN ANALYZE` on representative queries
- [ ] RLS policies still cover new query patterns (write a focused test that runs `SET ROLE reverie_app` + `SET LOCAL app.current_user_id`)

### Level 5: Browser Validation

```text
# Vite dev server is persistent at :5173 — do not start/stop.
# Capture screenshots via agent-browser and compare to dev hero screens:
- /library  vs  /design/hero/library
- /b/:id    vs  /design/hero/book

Failures to check (project memory `feedback_use_browser_for_design_critique.md`):
- shadcn Tabs render correctly with brand tokens
- bg-black overlay deferred fix is applied (sub-phase 11b at latest)
- Custom fonts loaded (Author / Satoshi visible)
- Console clean (no errors)
```

### Level 6: Manual Validation

Per sub-phase exit criteria above. For 11c, walk through accept → writeback enqueue → check `writeback_jobs` row. For 11e, log in as a non-admin and verify the admin route 403s.

---

## Acceptance Criteria

- [ ] All blueprint Step 11 exit criteria met:
  - [ ] All CRUD operations work through the UI
  - [ ] Search returns results within 200ms median for 10K-book library
  - [ ] Metadata drafts visible and actionable (accept/reject/revert)
  - [ ] RLS enforced (verified by tests with multiple users)
  - [ ] Child account admin controls functional
  - [ ] UI responsive and doesn't look like a default template (hero screens are the visual baseline)
  - [ ] No console errors
- [ ] Every new public Rust item has `///` with `# Errors` where applicable.
- [ ] Every new exported TS function/type has JSDoc (jsdoc-warn passes).
- [ ] No `// removed`-style backwards-compat shims (per CLAUDE.md).
- [ ] No regression in: OPDS feed tests, auth tests, ingestion tests, enrichment tests, writeback tests, cover tests.
- [ ] CodeQL workflow clean.
- [ ] Greptile + CodeRabbit + multi-agent review surface no P0/P1 findings post-fix.

---

## Completion Checklist

- [ ] All 6 sub-phase PRs merged (or fewer if scope adjusts; track in Linear umbrella issue).
- [ ] Linear umbrella issue (UNK-XX) closed with links to each sub-phase PR.
- [ ] Each sub-phase tagged with sqlx-runtime-allowlist updates if any.
- [ ] ADR for JSON API conventions landed (sub-phase 11a).
- [ ] ADR for persisted settings landed (sub-phase 11f).
- [ ] `debt/` updated if any workaround taken (e.g. last-admin protection done at app layer not DB constraint — flag in debt with lift condition).
- [ ] `bg-black` overlay debt resolved (project memory tracker).
- [ ] graphify-out updated: `graphify update .` after each sub-phase.

---

## Risks and Mitigations

| Risk                                                                                                                  | Likelihood | Impact | Mitigation                                                                                                                                                                                                                              |
| --------------------------------------------------------------------------------------------------------------------- | ---------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scope creep — Step 11 absorbs UNK-\* work that belongs in Step 12 (health dashboard)                                  | HIGH       | MEDIUM | Hard scope gate in "NOT Building" section; reviewers reject any aggregate health query in Step 11 PRs                                                                                                                                   |
| RLS regressions when adding filter joins in 11b                                                                       | MEDIUM     | HIGH   | Every new filter query gets a child-account RLS test; review the resulting EXPLAIN for any plan that bypasses the `manifestations_select_*` policies                                                                                    |
| Cursor pagination breaks under `?sort=title` (sort key ≠ cursor key)                                                  | LOW        | LOW    | Resolved 2026-05-22: sort-aware `CursorKey` enum landed in 11a Task 4. Migration adds `idx_works_sort_title_id`. Author sort defined as `sort_name` of `work_authors.position = 0`. Cursor variant-tag prevents cross-sort cursor reuse |
| 11f settings ADR concludes settings cannot be safely persisted                                                        | MEDIUM     | MEDIUM | Step 11 exit criterion for Blueprint task 9 downgrades to `GET /api/settings` (read-only). Document downgrade in 11f PR description; update Blueprint Step 11 task list if downgrade lands. Do not block 11a–11e on this                |
| `manualPagination: true` + react-query infinite-query mismatch                                                        | LOW        | LOW    | Build a small POC inside the 11a PR before committing to `@tanstack/react-table` for the list view; if friction, defer to a follow-up sub-phase                                                                                         |
| Settings reload (11f) becomes an architectural rabbit hole                                                            | MEDIUM     | HIGH   | 11f gated on ADR; if ADR proves complex, ship 11f as read-only `GET /api/settings` only and defer `PUT` to a separate Linear issue                                                                                                      |
| New runtime SQL queries (`QueryBuilder`) leak into the wild without allowlist                                         | MEDIUM     | MEDIUM | Pre-commit check or CI check that grep'd new `build_query_as` call sites have corresponding allowlist entries (manual review in PR; consider script later)                                                                              |
| Multi-agent / Greptile / CR all miss visual regression on production routes (prior incident PR #279)                  | MEDIUM     | MEDIUM | Mandatory agent-browser screenshot diff in every sub-phase PR description per project memory `feedback_use_browser_for_design_critique.md`                                                                                              |
| `axum-login` block (project memory: `project_axum_login_tower_sessions_block.md`) re-emerges if tower-sessions bumped | LOW        | LOW    | Don't bump tower-sessions during Step 11; track separately                                                                                                                                                                              |
| Drag-to-reorder UX (11d) snags on touch devices                                                                       | MEDIUM     | LOW    | Use `@dnd-kit` which supports pointer + keyboard + screen reader; not blocker for 11d exit                                                                                                                                              |

---

## Follow-up PRs unblocked by this plan (NOT in Step 11 scope)

These are improvements identified during plan-writing that touch Step 9 or earlier surfaces. Each lands as its own PR against `main`, not bundled into any Step 11 sub-phase:

| Follow-up                            | Surface                                      | Scope summary                                                                                                                                                                                                                        |
| ------------------------------------ | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `perf(covers): private/etag caching` | Step 9 — `backend/src/routes/opds/covers.rs` | Replace `Cache-Control: no-store` with `private, no-cache` + `ETag: "{first 16 chars of current_file_hash}"`. Honor `If-None-Match` → 304. Integration test asserting 304 with no body. Add `Accept-Ranges: bytes` for large covers. |

Tracked in: file a Linear issue per follow-up when picked up; do not pre-file (priorities may shift after sub-phases land).

---

## Notes

### Pagination — cursor model + RFC 8288 Link header signaling

Industry-standard-default principle ([[feedback_industry_standard_default]]) applied to pagination split into two orthogonal axes:

- **Pagination model**: NOT IETF-specified. Modern consensus (GitHub v4, Stripe, Slack, Twitter v2) is cursor for stability under concurrent writes + O(log N) per page. Reverie's enrichment pipeline writes asynchronously, so offset pagination is unstable; we ship cursor.
- **Pagination signaling**: RFC 8288 Link header (`Link: <next-url>; rel="next"`) is the IETF-canonical signal. Reverie's OPDS feeds already emit `<link rel="next">` Atom elements (the XML equivalent). We ship RFC 8288 Link headers AND an in-body `next_cursor` convenience field (because `fetch()` doesn't auto-parse Link headers; JS clients shouldn't need a header-parser dep).

This is the strict-superset shape: cursor pagination model + RFC 8288 signaling + body field. No tradeoff between standard and performance.

### Why this plan ships in 6 sub-phases, not 1

The blueprint pre-empts this with "Recommended sub-phase breakdown (a–f)". The risk in a single 20-task PR is review fatigue + visual regression escaping notice (PR #279 lesson per project memory). Six PRs of ~3-4 tasks each preserve review quality and let each merge demonstrate end-to-end progress.

### Why `axum-extra::Query` over `serde_qs` (recommendation, decide in 11b)

`axum-extra` is already an axum-ecosystem crate; `serde_qs` is independent. `axum-extra::extract::Query` handles `?tag=a&tag=b` into `Vec<String>` natively via `serde_html_form`. Smaller dep footprint, smaller serde-feature pressure. `serde_qs` is overkill for our filter shape (no nested objects). Document this choice in the 11b PR.

### Why `Cache-Control: private, no-cache` + ETag over `private, max-age=N` for covers

Covers can change (re-enrichment downloads a higher-quality cover; writeback rewrites the EPUB). `max-age=N` would serve stale covers for N seconds; ETag forces revalidation on every request but with body suppression on 304. Round-trip overhead is cheap (HEAD-equivalent) and correctness is preserved. If profiling later shows the revalidation RTT dominating, switch to short `max-age` per-cover; for now, ETag wins.

### React Query devtools dev-only

Mirror the `designRoutes` pattern: `if (import.meta.env.DEV) { const { ReactQueryDevtools } = await import('@tanstack/react-query-devtools'); ... }`. Don't ship to prod bundle.

### Settings (11f) is architecturally separable — could be split to a Step 11.5

If 11f's ADR uncovers heavy lifting (per-process reload coordination across worker pools, etc.), it can defer to a Step 11.5 standalone plan without breaking the Step 11 exit criteria — Blueprint task 9 ("GET /api/settings, PUT /api/settings") is the only Step 11 task that depends on persistence. Decide before starting 11f.

### Comment policy is enforced incrementally via the jsdoc ratchet

This branch (`chore/unk-236-eslint-plugin-jsdoc-install`) is currently warn-only. By the time Step 11 ships, the ratchet may have flipped to error in Stage D. Write every new export with JSDoc from the first commit — backfilling later is more work than authoring upfront.

### Graphify keeps up with the change

After each sub-phase PR merge, run `graphify update .` in the repo root to keep the AST graph aligned. Step 11 introduces many new symbols + cross-stack edges; the graph is the cheaper read-path for subsequent agents than scanning the new files cold.

### Security stance check ("will this stand up to security review?")

Per CLAUDE.md hard rule #6:

- **User input**: every JSON body validated by serde + explicit field checks; `Json` extractor rejects malformed UTF-8.
- **Auth**: every endpoint uses `CurrentUser`. Admin routes use `require_admin`. Child guard via `require_not_child` for metadata edit.
- **Sessions**: unchanged from Step 3 (axum-login + tower-sessions PostgresStore).
- **Secrets**: no new secret material introduced.
- **File I/O**: cover handler is the only new file-touching surface; existing pattern reused unchanged.
- **XML parsing**: no new XML; OPDS path untouched.
- **Outbound HTTP**: none. (Step 11 is internal-API + frontend only.)
- **Response headers**: CSP HTML and CSP API layers cover all new routes since they all live under `/api/*` (API CSP) or under `/` (HTML CSP). Cover endpoints' Cache-Control change reviewed above. SameSite=Lax + custom-header CSRF defense documented in 11a ADR.

**Verdict:** Plan stands up to security review pending the 11a ADR landing the CSRF custom-header decision in writing.

### Confidence Score for One-Pass Implementation

Post adversarial review (2026-05-22): plan patched against 6 HIGH and 7 MEDIUM findings. All HIGH-severity items now have spec-level fixes (last-admin TOCTOU `FOR UPDATE`, CSRF tower layer, RLS-existence gate on works, sort-aware cursor, JSON Merge Patch metadata edit, axum-extra committed in 11a). Confidence revised upward accordingly.

**Sub-phase 11a confidence: 9 / 10** — backend list pattern mirrors `emit_new` directly; cursor module is a small isolated change; CSRF layer is ~30 LOC. Residual risk: frontend data-layer scaffold introduces 4-5 new patterns at once (api/, query/, router data mode, lazy loaders, suspense). Mitigated by Task 9–14 RED→GREEN TDD ordering.

**Sub-phase 11b confidence: 8.5 / 10** — UNION ALL hybrid search lands once, well-trodden patterns thereafter. Perf gate as nightly-CI keeps the 200ms SLO honest without per-PR cost.

**Sub-phase 11c confidence: 8.5 / 10** — RFC 7396 JSON Merge Patch is standard; `serde_with::double_option` reduces boilerplate. Existing `apply_version` dispatcher does the heavy lifting.

**Sub-phase 11d confidence: 8 / 10** — If-Match concurrency is conventional; `@dnd-kit` mature.

**Sub-phase 11e confidence: 8.5 / 10** — last-admin protection now correctness-safe via row locks; admin gating uses existing `require_admin`.

**Sub-phase 11f confidence: 6.5 / 10** — pending settings-persistence ADR. Fallback path (downgrade to read-only) documented in Risks table.

**Overall plan confidence: 8.5 / 10** — strong patterns to mirror, clear scope gates, all known security holes from adversarial review patched, sub-phases isolated, Linear umbrella linked (UNK-80).
