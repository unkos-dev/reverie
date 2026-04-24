1# Plan: Design System & Visual Identity (BLUEPRINT Step 10)

> [!NOTE]
> **Unblocked 2026-04-24.** UNK-106 (CSP) shipped in PR #50 (`f070b97`) with a
> **hash-based** CSP (not nonce-based, as this plan originally assumed). The
> FOUC integration hook is already staged: `frontend/src/fouc/fouc.js` is a
> placeholder whose contents D3.13 will replace, and `frontend/vite-plugins/
> csp-hash.ts` injects the script into `index.html` at the `<!-- reverie:fouc-
> hash -->` marker and emits `dist/csp-hashes.json` on build. No backend
> templating of `index.html`; no per-request nonce. Sections touching CSP have
> been reconciled against the shipped mechanism.
>
> **Related Linear issues:** [UNK-104](https://linear.app/unkos/issue/UNK-104)
> (OIDC e2e test), [UNK-105](https://linear.app/unkos/issue/UNK-105)
> (shared-constants pipeline), [UNK-106](https://linear.app/unkos/issue/UNK-106)
> (CSP — shipped).

## Summary

Build Reverie's design foundation — a codified multi-theme token system, themed
shadcn/ui primitives, flicker-free theme switching with a DB-backed per-user
preference, and two hero screens (library grid + book detail) that prove the
system against realistic data. Scope is frontend-heavy with a single backend
sliver: one migration adding `theme_preference` to `users` plus an update to
`/auth/me` and a new `PATCH /auth/me/theme` endpoint. Design phases D1
(philosophy) and D2 (three coded directions) remain creative/iterative; this
plan gives D0 (test harness + deps), D3 (codification), D4 (hero screens), and
D5 (crosscheck review) execution-grade detail.

## User Story

As a Reverie user
I want the web UI to render with a distinctive, accessible visual identity that
remembers my Dark/Light/System preference across devices
So that every subsequent feature step inherits a consistent look-and-feel
instead of accumulating throwaway styling decisions.

## Problem → Solution

**Current state** (`frontend/src/App.tsx:1–121`, `frontend/src/index.css:1`,
`frontend/index.html:1–13`): the frontend is a default Vite scaffold — single
`App.tsx` rendering Vite/React hero logos, single-line `@import "tailwindcss"`,
no router, no tokens, no component library, no tests, no theme mechanism. Step
11+ cannot start without a design foundation.

**Desired state:** the app boots into a themed shell (Dark/Light, selected
synchronously from a cookie before React hydrates — no theme flicker),
navigates via react-router, composes from restyled shadcn primitives bound to
semantic tokens, and ships `/design/system` + `/design/hero/{library,book}`
dev-only routes that serve as the visual contract for Step 11. The design
system is canonically documented in `docs/design/PHILOSOPHY.md` +
`docs/design/VISUAL_IDENTITY.md`.

## Metadata

| Field | Value |
|---|---|
| BLUEPRINT ref | `plans/BLUEPRINT.md` lines 1708–1870 |
| Branch | `feat/design-system` |
| Depends on | Step 9 merged |
| Parallelism | Standalone; Step 11 blocks on this |
| Complexity | HIGH (multi-phase, creative + mechanical, DB + FE, crosscheck gate) |
| Estimated files | ~45–60 (1 migration up/down, ~3 backend edits, ~15 shadcn primitives, ~10 theme/provider/switcher files, ~6 hero/gallery route files, 2 docs files, 1 CI edit) |
| Model tier | Strongest (visual identity is a product pillar; errors cascade into every subsequent frontend step) |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────────┐      ┌───────────────────┐     ┌──────────────────┐     ║
║   │  Cold page load │──────│  Vite default     │──── │ React/Vite logos │     ║
║   │  (localhost:...)│      │  scaffold renders │     │  + counter demo  │     ║
║   └─────────────────┘      └───────────────────┘     └──────────────────┘     ║
║                                                                               ║
║   USER_FLOW: navigate to app → see Vite boilerplate → nothing resembling      ║
║              a real product                                                   ║
║   PAIN_POINT: no design identity, no router, no primitives, no a11y, no      ║
║               theming mechanism                                               ║
║   DATA_FLOW: no API calls; no user preference read/write                      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌───────────┐   inline   ┌───────────────────┐                               ║
║  │  Cold     │─ blocking ─│ reads reverie_    │  sets <html data-theme="…">   ║
║  │  load     │  script    │ theme cookie →    │  BEFORE React mounts          ║
║  └───────────┘            │ prefers-color-    │  → FIRST PAINT IS CORRECT     ║
║                           │ scheme fallback   │                               ║
║                           └────────┬──────────┘                               ║
║                                    ▼                                          ║
║                           ┌────────────────────┐                              ║
║                           │  React hydrates;   │                              ║
║                           │  ThemeProvider     │                              ║
║                           │  fetches /auth/me, │                              ║
║                           │  reconciles cookie │                              ║
║                           └────┬──────────┬────┘                              ║
║                                │          │                                   ║
║                 ┌──────────────┘          └────────────┐                      ║
║                 ▼                                      ▼                      ║
║  ┌──────────────────────────┐      ┌──────────────────────────────────────┐   ║
║  │ Production bundle:       │      │ Dev bundle (also dev gallery):       │   ║
║  │  App shell (react-router │      │  + /design/system (primitive gallery)│   ║
║  │  + themed primitives) —  │      │  + /design/hero/library              │   ║
║  │  /design/* tree-shaken   │      │  + /design/hero/book                 │   ║
║  │  out via dynamic import  │      │  (imported via dynamic import inside │   ║
║  │  inside `if (DEV)` block │      │   `if (import.meta.env.DEV)`)        │   ║
║  └──────────────────────────┘      └──────────────────────────────────────┘   ║
║                                                                               ║
║  USER_FLOW: cold load → correct theme first paint → app shell → browse       ║
║             (Step 11 inherits everything)                                     ║
║  VALUE_ADD: distinctive visual identity, no flicker, accessible, multi-theme  ║
║             by architecture; Step 11 inherits tokens + primitives + pattern   ║
║  DATA_FLOW: cookie(reverie_theme) ↔ inline script ↔ React provider ↔          ║
║             PATCH /auth/me/theme ↔ users.theme_preference                     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | Impact |
|---|---|---|---|
| `/` (frontend root) | Vite/React logos + counter demo | Themed app shell (Step 11 will fill in; scaffold ships with react-router, themed layout, no business content) | Foundation in place for all subsequent UI work |
| `/design/system` | 404 | Primitive gallery — every shadcn component in every state, in both themes; dev-only | Visual contract reviewable by any contributor |
| `/design/hero/library`, `/design/hero/book` | 404 | Production-fidelity reference screens against fixture data; dev-only | Step 11 mirrors these instead of designing from scratch |
| First paint on cold load | White default | `data-theme` set from cookie by blocking inline script; first paint matches stored preference | No theme flicker (FOUC) |
| `GET /auth/me` | Returns `{id, display_name, email, role, is_child}` (`backend/src/routes/auth.rs:162–177`) | Adds `theme_preference` field | Frontend reconciles cookie with server on hydrate |
| `PATCH /auth/me/theme` (new) | 404 | Accepts `{theme_preference: "system" \| "light" \| "dark"}`, updates `users.theme_preference`, refreshes `reverie_theme` cookie | Preference persists across devices |
| Session cookie (`id`, tower-sessions default, `backend/src/main.rs:27–34`) | Unchanged — stays HttpOnly | Joined by sibling `reverie_theme` cookie (not HttpOnly, SameSite=Lax, 1yr, Path=/) | JS can read the theme cookie synchronously for FOUC avoidance |
| `users` table | No theme column | Adds `theme_preference TEXT NOT NULL DEFAULT 'system'` | Per-user preference, multi-user-aware |
| `frontend/vite.config.ts` | No `server.proxy`; dev is cross-origin to backend | Proxies `/api`, `/auth`, `/opds` to `http://localhost:3000` | Same-origin dev → session + theme cookies work identically to production |
| CI (`.github/workflows/ci.yml:87–110`) | `npm ci && lint && build` | Adds `npm test -- --run`, stylelint, bundle-leak gate | Regressions on theme/primitive/gating caught in CI |

---

## Mandatory Reading

Implementation agent MUST read these before starting any task.

| Priority | File | Lines | Why |
|---|---|---|---|
| P0 | `plans/BLUEPRINT.md` | 1708–1870 | Step 10 spec — this plan operationalises it |
| P0 | `plans/DESIGN_BRIEF.md` | 1–622 | Product identity; philosophy inputs for D1 |
| P0 | `frontend/CLAUDE.md` | 1–37 | Frontend conventions (no `any`, shadcn via CLI, API calls centralised, no arbitrary hex, Vitest+RTL) |
| P0 | `frontend/index.html` | 1–14 | Contains `<!-- reverie:fouc-hash -->` marker on line 8; the Vite plugin injects `<script>${fouc.js body}</script>` here at build. Do not hand-edit the marker. `<title>frontend</title>` on line 7 still needs updating to `Reverie` as part of D3.13. |
| P0 | `frontend/src/fouc/fouc.js` | 1–5 | Placeholder set up by UNK-106; D3.13 replaces its contents with the FOUC script body (plain JS, no `<script>` tags). Any change regenerates `sha256-…` in `dist/csp-hashes.json` automatically at next build. Script body MUST NOT contain `</script>` — the plugin throws at build time if it does. |
| P0 | `frontend/vite-plugins/csp-hash.ts` | 1–80 | Reads `fouc.js`, replaces the marker, emits `dist/csp-hashes.json`. `transformIndexHtml` runs in both `serve` and `build`; sidecar is only written on `build`. Injection-safety guard enforced here. |
| P0 | `backend/src/security/dist_validation.rs` | 1–270 | Reads `dist/csp-hashes.json` at startup, validates it matches the CSP header the server will emit for HTML routes. Failure mode is fail-fast at boot — no hash drift between FE and BE can slip into a running server. |
| P0 | `backend/src/security/csp.rs` | 1–125 | Builds the differentiated CSP: HTML gets `script-src 'self' 'sha256-…'`; API gets `default-src 'none'`. Route-class dispatch lives in `backend/src/routes/spa.rs` via a single composite `.fallback`. |
| P0 | `frontend/src/main.tsx` | 1–10 | React entrypoint to wrap in `ThemeProvider` + `RouterProvider` |
| P0 | `frontend/src/index.css` | 1 | Single `@import "tailwindcss"`; `@theme` layer + theme override selectors go here |
| P0 | `frontend/vite.config.ts` | 1–7 | Will gain `server.proxy` + `test` key |
| P0 | `frontend/eslint.config.js` | 1–23 | Will gain `no-restricted-syntax` for hex literals |
| P0 | `frontend/tsconfig.app.json` | all | Types array needs `vitest/jsdom` added |
| P0 | `frontend/package.json` | 1–32 | Scripts + deps surface |
| P0 | `backend/migrations/20260414000001_add_session_version.up.sql` | 1 | **Canonical ADD COLUMN pattern** — mirror verbatim for `theme_preference` |
| P0 | `backend/migrations/20260414000001_add_session_version.down.sql` | 1 | Canonical DROP COLUMN pattern |
| P0 | `backend/migrations/20260412150002_core_tables.up.sql` | 2–18, 68–81 | `users` DDL + grants; no RLS on `users` (so handlers query `state.pool` directly, no `acquire_with_rls`) |
| P0 | `backend/src/routes/auth.rs` | 23, 133–177 | `/auth/me` GET + route registration; where PATCH handler + cookie write hook in |
| P0 | `backend/src/models/user.rs` | 7–8, 73–79, 152–186 | `USER_COLUMNS` constant (must add `theme_preference`), `find_by_id`, existing `#[sqlx::test]` pattern |
| P0 | `backend/src/main.rs` | 26–55 | `SessionManagerLayer` config (HttpOnly=true → theme cookie must be separate); router assembly |
| P0 | `backend/src/auth/middleware.rs` | 109–135 | `CurrentUser` extractor reused by new PATCH handler |
| P1 | `backend/src/db.rs` | 44–72 | `acquire_with_rls` — **NOT** used for theme handler (users has no RLS) but referenced as the codebase test harness pattern |
| P1 | `backend/src/test_support.rs` | all | `create_admin_and_basic_auth`, `server_with_real_pools`, integration test scaffolding |
| P1 | `backend/Cargo.toml` | 1–45 | `tower-http` has `cors` feature enabled but `CorsLayer` is never instantiated — same-origin via Vite proxy avoids CORS entirely. `axum-extra` is NOT currently a dep; must be added with the `cookie` feature for the `CookieJar` extractor/response pattern used by the theme cookie. |
| P1 | `.github/workflows/ci.yml` | 87–110 | Frontend CI job; adds `npm test`, stylelint, bundle-leak gate |
| P1 | `docs/astro.config.mjs` | 18–29 | Manual sidebar — new `Design` group needed to surface PHILOSOPHY + VISUAL_IDENTITY |
| P1 | `docs/src/content/docs/getting-started/introduction.md` | all | Starlight markdown pattern (frontmatter `title:`) |
| P2 | `backend/src/routes/tokens.rs` | 33–183 | Representative authenticated PATCH handler shape (JSON body + `CurrentUser` + JSON response) |
| P2 | `.claude/PRPs/plans/completed/opds-catalog.plan.md` | all | Reference PRP format for this repo |

### External Documentation

| Source | Section | Why |
|---|---|---|
| [Tailwind CSS v4 — Theme variables](https://tailwindcss.com/docs/theme) | "Why @theme instead of :root?" + "Overview" | `@theme` defines design tokens that generate utilities and **must be top-level, not nested under selectors**. Theme switching (`[data-theme="…"]`) uses regular CSS variables alongside `@theme`; `@theme inline` lets utilities reference runtime variables. |
| [shadcn/ui — Tailwind v4 guide](https://ui.shadcn.com/docs/tailwind-v4) | whole page | Canonical Tailwind v4 + React 19 setup for shadcn. Confirms `@theme inline` is the supported multi-theme path. |
| [shadcn/ui — Vite install](https://ui.shadcn.com/docs/installation/vite) | whole page | `npx shadcn@latest init` with Vite template scaffolds `components.json`, path aliases, theme CSS |
| [shadcn/ui — Dark mode (Vite)](https://ui.shadcn.com/docs/dark-mode/vite) | whole page | Reference theme provider + toggle; adapt for cookie+DB instead of localStorage-only |
| [shadcn/ui — CLI changelog (v4)](https://ui.shadcn.com/docs/changelog/2026-03-cli-v4) | whole page | `npx shadcn@latest init` now offers Vite template; includes dark-mode scaffold |
| [Vitest — Getting Started](https://vitest.dev/guide/) | "Configuring Vitest" | `test` key in `vite.config.ts` or separate `vitest.config.ts`; `environment: 'jsdom'`, `globals: true`, `setupFiles` |
| [Vitest — Environment](https://vitest.dev/guide/environment.html) | "jsdom" | `tsconfig.json` types: `["vitest/jsdom"]` required for TS recognition |
| [React Testing Library — Setup for Vitest](https://testing-library.com/docs/react-testing-library/setup) | whole page | `@testing-library/react` + `@testing-library/jest-dom` + `@testing-library/user-event`; setup file calls `cleanup` after each test |
| [Vite — `import.meta.env`](https://vite.dev/guide/env-and-mode.html) | "Built-in constants" | `import.meta.env.DEV` is replaced at build time → dead code inside `if (!DEV)` branches is tree-shaken. Dynamic `import()` inside DEV-only branches ensures the entire target module tree is eliminated. |
| [Vite — `server.proxy`](https://vite.dev/config/server-options.html#server-proxy) | "server.proxy" | Dev-time proxy to same-origin the backend; avoids CORS entirely since backend never instantiates `CorsLayer` |
| [Stylelint — `color-no-hex`](https://stylelint.io/user-guide/rules/color-no-hex/) | rule page | Built-in (no plugin); configure via `overrides` to exempt `src/styles/themes/*.css` where canonical hex tokens live |
| [ESLint — `no-restricted-syntax`](https://eslint.org/docs/latest/rules/no-restricted-syntax) | selector syntax | Use an AST selector against string literals matching `^#[0-9a-fA-F]{3,8}$` to flag hex in `.tsx` |
| [Radix UI — React 19 compatibility](https://www.radix-ui.com/) | release notes | Confirm React 19 support on every primitive added (the shadcn v4 set is fully compat as of 2026-03 CLI release) |
| [@fontsource docs](https://fontsource.org/docs/getting-started/install) | install + imports | Per-weight subpackage imports; works with Vite's asset pipeline out of the box |
| [@axe-core/cli](https://github.com/dequelabs/axe-core-npm/tree/develop/packages/cli) | README | `axe <url> --exit` exits non-zero on violations → CI gate |
| [tweakcn](https://tweakcn.com) | live tool | Browser-based token editor; exports Tailwind v4-compatible `@theme` CSS + `:root` / `[data-theme]` overrides |

---

## Patterns to Mirror

**ADD_COLUMN_MIGRATION** — the exact shape for the theme-preference migration:

```sql
-- SOURCE: backend/migrations/20260414000001_add_session_version.up.sql (1 line, full file)
ALTER TABLE users ADD COLUMN session_version INTEGER NOT NULL DEFAULT 0;

-- down.sql counterpart:
ALTER TABLE users DROP COLUMN session_version;
```

**MIRROR AS:**

```sql
-- backend/migrations/20260422000001_add_theme_preference.up.sql
ALTER TABLE users ADD COLUMN theme_preference TEXT NOT NULL DEFAULT 'system';

-- .down.sql
ALTER TABLE users DROP COLUMN theme_preference;
```

Notes: no `CHECK` constraint — application-layer validation against the allowed
set (`system`, `light`, `dark`) keeps the schema future-proof for additional
themes (per BLUEPRINT "architect for unlimited themes"). Timestamp the
migration with today's date; existing convention is `YYYYMMDD0000NN`. The
`20260422000001` timestamp shown above is illustrative — run `date +%Y%m%d000001`
at write-time to generate the real filename, not the literal value here.

**USER_MODEL_COLUMN_ADDITION** — `theme_preference` must be added in **four**
places in `backend/src/models/user.rs`. `UserRow` and `User` are distinct types
with an explicit `From<UserRow> for User` impl; missing any of these four edits
produces a compile error:

```rust
// 1. USER_COLUMNS constant (line 7) — append theme_preference to the SELECT list
const USER_COLUMNS: &str =
    "id, oidc_subject, display_name, email, role::text, is_child, \
     created_at, updated_at, session_version, theme_preference";

// 2. UserRow struct (line 11) — add the field for sqlx FromRow
struct UserRow {
    // ... existing fields ...
    theme_preference: String,
}

// 3. User public struct (line 24) — add the field
pub struct User {
    // ... existing fields ...
    pub theme_preference: String,
    // ... existing session_version_bytes: Vec<u8> synthetic field ...
}

// 4. From<UserRow> for User impl (line 39) — add the field to the constructor
impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        // ... existing field copies ...
        Self {
            // ... existing field initialisations ...
            theme_preference: row.theme_preference,
            // session_version_bytes computed as before — leave untouched
        }
    }
}
```

The rest of `find_by_id` / `upsert_from_oidc_and_maybe_promote` passes through
unchanged. `session_version_bytes` is an unrelated synthetic field — leave its
computation alone.

**AUTH_ME_RESPONSE** — current handler at `backend/src/routes/auth.rs:162–177`:

```rust
async fn me(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let u = user::find_by_id(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(serde_json::json!({
        "id": u.id,
        "display_name": u.display_name,
        "email": u.email,
        "role": u.role,
        "is_child": u.is_child,
    })))
}
```

**EXTEND AS:** add `"theme_preference": u.theme_preference` to the JSON. No RLS
transaction — `users` has no row-level policies (confirmed by exploration; the
only `ENABLE ROW LEVEL SECURITY` in the migration set is on `manifestations` at
`20260412150007_search_rls_and_reserved.up.sql:45`).

**PATCH_HANDLER_SHAPE** — follow `backend/src/routes/tokens.rs:33–183` (authed
PATCH/POST handler, JSON request/response, `CurrentUser` extractor, `state.pool`
for queries against tables without RLS). The cookie is written via
`axum_extra::extract::cookie::CookieJar`, returned as part of the response
tuple — this composes with `Redirect` for the OIDC callback site too, and
does **not** require mounting a separate `CookieManagerLayer` in the router:

```rust
// NEW: backend/src/routes/auth.rs (append to existing module)
use axum_extra::extract::cookie::CookieJar;

#[derive(serde::Deserialize)]
struct UpdateThemeRequest {
    theme_preference: String,
}

const ALLOWED_THEMES: &[&str] = &["system", "light", "dark"];

async fn update_theme(
    current_user: CurrentUser,
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<UpdateThemeRequest>,
) -> Result<(CookieJar, Json<serde_json::Value>), AppError> {
    if !ALLOWED_THEMES.contains(&body.theme_preference.as_str()) {
        // AppError::Validation maps to 422 Unprocessable Entity across this
        // API — there is no BadRequest variant. If you want 400 specifically,
        // that needs a project-wide error-taxonomy change, not a local edit.
        return Err(AppError::Validation("invalid theme_preference".into()));
    }
    sqlx::query("UPDATE users SET theme_preference = $1, updated_at = now() WHERE id = $2")
        .bind(&body.theme_preference)
        .bind(current_user.user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    // Mirror to cookie so FOUC script reads it on next cold load.
    // `set_theme_cookie` adds to the jar; returning (jar, ...) in the tuple
    // emits the Set-Cookie header alongside the JSON body.
    let jar = set_theme_cookie(jar, &body.theme_preference);
    Ok((jar, Json(serde_json::json!({ "theme_preference": body.theme_preference }))))
}

// route registration: .route("/auth/me/theme", patch(update_theme))
```

**THEME_COOKIE_WRITER** — the FOUC script reads the `reverie_theme` cookie
synchronously. The session cookie (tower-sessions default name `id`) is
`HttpOnly: true` (`backend/src/main.rs:27–34`), so a **separate** non-HttpOnly
cookie is required. The cookie name is declared as a `pub const` and
referenced, not re-literalled (see shared-constants tracker UNK-105):

```rust
// NEW: backend/src/auth/theme_cookie.rs
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use time::Duration;

/// Cookie name for the FOUC theme preference. Duplicated in:
///   - frontend/src/fouc/fouc.js (inline FOUC script body, CSP-hashed at build)
///   - frontend/src/lib/theme/cookie.ts
/// All three MUST agree. Tracked as instance 1 under UNK-105.
pub const THEME_COOKIE_NAME: &str = "reverie_theme";

pub fn set_theme_cookie(jar: CookieJar, value: &str) -> CookieJar {
    let cookie = Cookie::build((THEME_COOKIE_NAME, value.to_owned()))
        .path("/")
        .http_only(false) // JS must read it before hydration
        .same_site(SameSite::Lax)
        .max_age(Duration::days(365))
        // No `Secure` — matches session cookie behavior (plain HTTP behind TLS proxy)
        .build();
    jar.add(cookie)
}
```

Unit test (mandatory — see Testing Strategy section) asserts that the produced
cookie's name equals `"reverie_theme"` verbatim. This is the enforcement for
the cross-stack constant; if someone renames the const, this test fails and
surfaces the drift before it lands.

This helper is also called from the OIDC `callback` handler
(`backend/src/routes/auth.rs:~143`) right after `auth_session.login(&user)`
succeeds, seeding the cookie from the DB value on every login. The callback's
return type changes from `Redirect` to `(CookieJar, Redirect)` — the tuple
form composes cleanly because `CookieJar` implements `IntoResponseParts`:

```rust
// backend/src/routes/auth.rs — callback handler tail
let jar = set_theme_cookie(jar, &user.theme_preference);
Ok((jar, Redirect::temporary("/")))
```

The callback signature gains `jar: CookieJar` as an extractor alongside the
existing ones.

**SQLX_TEST_HARNESS** — migration + PATCH verification. Helper signatures
verified against `backend/src/test_support.rs` (2026-04-23):
`create_adult_and_basic_auth(&PgPool, &str) -> (Uuid, String)` and
`server_with_real_pools(&PgPool, &PgPool) -> axum_test::TestServer` — both
pools required, second is the ingestion pool:

```rust
// SOURCE: backend/src/models/user.rs:152-186 (upsert test)
//       + backend/src/routes/metadata.rs:643-674 (route test)
#[sqlx::test(migrations = "./migrations")]
async fn theme_preference_migration_applies(pool: sqlx::PgPool) {
    // Verify the column exists with correct default
    let default: String = sqlx::query_scalar(
        "SELECT theme_preference FROM users WHERE false UNION ALL SELECT 'system' LIMIT 1"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(default, "system");
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_theme_updates_user_row(pool: sqlx::PgPool) {
    use axum::http::{header::AUTHORIZATION, StatusCode};
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (user_id, basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "theme-test").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let resp = server
        .patch("/auth/me/theme")
        .add_header(AUTHORIZATION, basic)
        .json(&serde_json::json!({"theme_preference": "dark"}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    // Assert the Set-Cookie header was emitted with the canonical name
    let set_cookie = resp.header("set-cookie").to_str().unwrap().to_owned();
    assert!(set_cookie.starts_with("reverie_theme="));
    let stored: String = sqlx::query_scalar("SELECT theme_preference FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&app_pool)
        .await
        .unwrap();
    assert_eq!(stored, "dark");
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_theme_rejects_invalid_value(pool: sqlx::PgPool) {
    use axum::http::{header::AUTHORIZATION, StatusCode};
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_user_id, basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "theme-test-invalid").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let resp = server
        .patch("/auth/me/theme")
        .add_header(AUTHORIZATION, basic)
        .json(&serde_json::json!({"theme_preference": "purple"}))
        .await;
    // AppError::Validation → 422 UNPROCESSABLE_ENTITY (NOT 400). See error.rs.
    assert_eq!(resp.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}
```

**FRONTEND_TESTING_HARNESS** — no existing pattern in the repo (first frontend
test); mirror the Vitest + RTL canonical setup from the docs:

```typescript
// NEW: frontend/vitest.config.ts (or add `test` key inline to vite.config.ts)
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./tests/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
  },
});
```

```typescript
// NEW: frontend/tests/setup.ts
import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';

afterEach(() => cleanup());
```

```json
// UPDATE: frontend/tsconfig.app.json — compilerOptions.types
{
  "compilerOptions": {
    "types": ["vitest/globals", "vitest/jsdom", "@testing-library/jest-dom"]
  }
}
```

---

## New Patterns to Establish

**TAILWIND_V4_MULTI_THEME** — `@theme` declares token → utility mapping;
`@custom-variant` teaches Tailwind what the `dark:` modifier means; runtime
swap happens via regular CSS variables keyed on `[data-theme]`:

```css
/* frontend/src/index.css */
@import "tailwindcss";

/* Tell Tailwind: "dark:" variant activates when [data-theme="dark"] is on
   an ancestor (or the element itself). Required because Tailwind v4's default
   dark-mode detection is media-query based. */
@custom-variant dark (&:where([data-theme="dark"], [data-theme="dark"] *));

/* Tokens that generate utilities (bg-surface, text-ink, etc.).
   Values are runtime vars → utilities cascade with theme switch. */
@theme inline {
  --color-surface: var(--surface);
  --color-ink: var(--ink);
  --color-accent: var(--accent);
  --color-muted: var(--muted);
  --color-border: var(--border);
  /* Typography, spacing, radius, shadow tokens as theme-stable: */
  --font-display: "<D3-chosen-font>", serif;
  --font-body: "<D3-chosen-font>", sans-serif;
  --radius-sm: 0.25rem;
  --radius-md: 0.5rem;
  /* ... */
}

/* Default + explicit Light theme — runtime values live on :root, NOT inside
   @theme (which can't be nested under selectors). */
:root,
[data-theme="light"] {
  --surface: <tweakcn-export>;
  --ink: <tweakcn-export>;
  --accent: <tweakcn-export>;
  --muted: <tweakcn-export>;
  --border: <tweakcn-export>;
}

/* Dark theme override */
[data-theme="dark"] {
  --surface: <tweakcn-export>;
  --ink: <tweakcn-export>;
  --accent: <tweakcn-export>;
  --muted: <tweakcn-export>;
  --border: <tweakcn-export>;
}
```

Three load-bearing patterns:
1. `@custom-variant dark (...)` — without this, `dark:bg-surface` utilities never activate on `[data-theme="dark"]`.
2. `@theme inline` (not plain `@theme`) — the `inline` keyword is what allows tokens to reference runtime `var(--surface)` values.
3. Theme value overrides live on regular selectors (`:root`, `[data-theme="dark"]`) **outside** `@theme`. `@theme` itself cannot be nested under any selector per Tailwind v4 docs.

**FOUC_INLINE_SCRIPT** — blocking script that runs before React bundle loads.
The script **body** lives in `frontend/src/fouc/fouc.js`; the Vite plugin
`vite-plugins/csp-hash.ts` injects it as `<script>${fouc}</script>` at the
`<!-- reverie:fouc-hash -->` marker in `index.html` (during both `serve` and
`build`). On `build` the plugin also emits `dist/csp-hashes.json` with the
SHA-256 of the body, which `backend/src/security/dist_validation.rs` reads at
startup. No `<script>` tags belong in `fouc.js` itself, and its contents MUST
NOT contain `</script>` (plugin throws):

```javascript
// Contents of frontend/src/fouc/fouc.js — NO surrounding <script> tag
(function () {
  try {
    var cookie = document.cookie
      .split('; ')
      .find(function (c) { return c.startsWith('reverie_theme='); });
    var pref = cookie ? cookie.split('=')[1] : 'system';
    var effective = pref;
    if (pref === 'system') {
      effective = window.matchMedia('(prefers-color-scheme: dark)').matches
        ? 'dark'
        : 'light';
    }
    document.documentElement.dataset.theme = effective;
  } catch (e) {
    document.documentElement.dataset.theme = 'light';
  }
})();
```

Plain ES5 (no bundling needed), self-invoking, no dependencies, try/catch
fallback to `light`. Unauthenticated visitors get `prefers-color-scheme` via
the `'system'` default. Authenticated users get their server-synced preference
(cookie is written by backend on login and on PATCH).

**THEME_PROVIDER** — React context that reconciles cookie/server/OS and exposes
the setter:

```typescript
// NEW: frontend/src/lib/theme/ThemeProvider.tsx (sketch — full implementation in D3 task)
type Theme = 'system' | 'light' | 'dark';
type EffectiveTheme = 'light' | 'dark';

interface ThemeContextValue {
  preference: Theme;       // the user's stored choice
  effective: EffectiveTheme;  // what data-theme actually is
  setPreference: (t: Theme) => void; // writes cookie, PATCHes server, updates DOM
}
```

Initial state is read from `document.documentElement.dataset.theme` (set by
the inline script) to match what's already painted. On mount, the provider
fetches `/auth/me`, and if the server `theme_preference` differs from the
cookie, trusts the server and updates both cookie and DOM. Every `setPreference`
call is optimistic (writes cookie + DOM immediately) then PATCHes; on PATCH
failure it reverts both.

**Cross-tab sync via `BroadcastChannel`**: the provider subscribes to
`BroadcastChannel('reverie-theme')` on mount and posts the new value on
successful `setPreference`. On receive, the receiving tab mirrors the value
to its local state + DOM without re-PATCHing (the originating tab already
did). This eliminates the "user changes theme in tab A, switches to tab B,
sees old theme" papercut. `BroadcastChannel` is broadly supported
(~2017-vintage API, iOS Safari 15.4+, all evergreen browsers); no fallback
needed for this project.

**SHADCN_COMPONENTS_JSON** — pre-written before `npx shadcn@latest init` so
the init runs zero-prompt. Defaults chosen 2026-04-23. Path aliases must be
present in `tsconfig.app.json` (`paths: { "@/*": ["src/*"] }`) and
`vite.config.ts` (`resolve.alias: { "@": resolve(__dirname, "src") }`) before
running `init`.

```json
{
  "$schema": "https://ui.shadcn.com/schema.json",
  "style": "new-york",
  "rsc": false,
  "tsx": true,
  "tailwind": {
    "config": "",
    "css": "src/index.css",
    "baseColor": "neutral",
    "cssVariables": true,
    "prefix": ""
  },
  "aliases": {
    "components": "@/components",
    "utils": "@/lib/utils",
    "ui": "@/components/ui",
    "lib": "@/lib",
    "hooks": "@/hooks"
  },
  "iconLibrary": "lucide"
}
```

Notes on choices:
- `style: "new-york"` — since shadcn 2.5+ on Tailwind v4, `default` is no longer
  offered; only `new-york` remains. Locked regardless.
- `baseColor: "neutral"` — most neutral of the five options; tweakcn output in
  D3.7 overwrites initial colour values, so the choice is about what shadcn
  writes on init, not the final palette.
- `tailwind.config: ""` — Tailwind v4 has no JS config file; theme is CSS-side.
- `cssVariables: true` — required for the `[data-theme]` runtime swap pattern.
- `rsc: false` — this is a Vite SPA, not Next.js. Flipping later if we
  migrate to a meta-framework is the smallest part of that migration.
- `iconLibrary: "lucide"` — ~1,500 icons covering reading-app vocabulary
  (book, bookmark, library, scroll, glasses, notebook, pen, quote,
  highlighter). Radix Icons' ~300 would hit the wall in hero screens.
- Aliases match `frontend/CLAUDE.md` project structure exactly.

**DEV_ROUTE_TREE_SHAKING** — the gating mechanism for `/design/*`:

```typescript
// frontend/src/main.tsx (sketch)
import { createBrowserRouter, RouterProvider } from 'react-router';

const prodRoutes = [/* app shell — no /design/* */];

async function buildRouter() {
  const routes = [...prodRoutes];
  if (import.meta.env.DEV) {
    const { designRoutes } = await import('./routes/design');
    routes.push(...designRoutes);
  }
  return createBrowserRouter(routes);
}
```

`import.meta.env.DEV` is replaced at build time to a literal `false` in
production, and Vite's tree-shaker eliminates the whole `import('./routes/design')`
target module tree. Verified by the CI grep gate in the verification block.
Static top-level `import { designRoutes } from './routes/design'` does **not**
achieve this, even if the route list is conditionally empty.

**VITE_PROXY_FOR_SAME_ORIGIN_DEV** — removes the need for CORS:

```typescript
// frontend/vite.config.ts (extend existing)
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      '/api':  { target: 'http://localhost:3000', changeOrigin: true },
      '/auth': { target: 'http://localhost:3000', changeOrigin: true },
      '/opds': { target: 'http://localhost:3000', changeOrigin: true },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./tests/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
  },
});
```

Backend port `3000` matches `REVERIE_PORT` default (`backend/src/config.rs:103–109`).
Cookies set by the backend on `/auth/*` are automatically same-origin in dev.
No `CorsLayer` needed; matches production topology (Docker serves frontend +
backend from same origin).

---

## Files to Change

Backend (small):

| File | Action | Why |
|---|---|---|
| `backend/Cargo.toml` | UPDATE | Add `axum-extra = { version = "0.10", features = ["cookie"] }` direct dep (tower-cookies is transitive via axum-login; we use axum-extra's `CookieJar` for the typed extractor + response pattern that composes with `Redirect`) |
| `backend/migrations/{today}000001_add_theme_preference.up.sql` | CREATE | Add column, mirror `add_session_version` pattern. Filename timestamp from `date +%Y%m%d000001` at write-time. |
| `backend/migrations/{today}000001_add_theme_preference.down.sql` | CREATE | Rollback |
| `backend/src/models/user.rs` | UPDATE | `USER_COLUMNS` constant + `User` struct + `UserRow` (if separate) gain `theme_preference: String` |
| `backend/src/routes/auth.rs` | UPDATE | `me` handler adds field to JSON; new `update_theme` handler using `CookieJar` tuple return; OIDC `callback` signature gains `jar: CookieJar` and returns `(CookieJar, Redirect)` after seeding the theme cookie post-login |
| `backend/src/auth/theme_cookie.rs` | CREATE | `THEME_COOKIE_NAME` const + `set_theme_cookie(jar, value) -> CookieJar` helper |
| `backend/src/models/user.rs` or `backend/src/models/theme.rs` | UPDATE/CREATE | `ALLOWED_THEMES` constant |
| `backend/tests/...` or inline `#[sqlx::test]` | CREATE | Migration smoke + PATCH integration tests |

Frontend (substantial):

| File | Action | Why |
|---|---|---|
| `frontend/package.json` | UPDATE | Add devDeps (vitest, @testing-library/react, @testing-library/jest-dom, @testing-library/user-event, jsdom, stylelint, @axe-core/cli) + deps (react-router, lucide-react, @fontsource/<chosen>); add `test`, `test:coverage`, `stylelint` scripts |
| `frontend/vite.config.ts` | UPDATE | Add `server.proxy` + `test` key |
| `frontend/vitest.config.ts` | CREATE (optional, if not inlined) | Vitest config |
| `frontend/tests/setup.ts` | CREATE | RTL setup, jest-dom registration, cleanup |
| `frontend/tsconfig.app.json` | UPDATE | `types: ["vitest/globals", "vitest/jsdom", "@testing-library/jest-dom"]` |
| `frontend/eslint.config.js` | UPDATE | Add `no-restricted-syntax` rule banning hex literals in `.tsx` |
| `frontend/.stylelintrc.json` | CREATE | `color-no-hex` rule with `overrides` exempting `src/styles/themes/*.css` |
| `frontend/index.html` | UPDATE | Update `<title>frontend>` → `<title>Reverie</title>`; leave the `<!-- reverie:fouc-hash -->` marker untouched (injection is automated by `vite-plugins/csp-hash.ts`). |
| `frontend/src/fouc/fouc.js` | UPDATE | Replace placeholder body with FOUC_INLINE_SCRIPT contents (plain JS, no `<script>` tags; no `</script>` substrings). Build regenerates `dist/csp-hashes.json`. |
| `frontend/src/main.tsx` | UPDATE | Wrap `<App />` in `<ThemeProvider>` + `<RouterProvider>` |
| `frontend/src/App.tsx` | REPLACE | Delete Vite scaffold; replace with minimal app shell (header + `<Outlet />`) |
| `frontend/src/App.css` | DELETE | Legacy Vite scaffold CSS |
| `frontend/src/assets/{react.svg,vite.svg,hero.png}` | DELETE | Scaffold assets |
| `frontend/src/index.css` | UPDATE | Add `@theme inline` block + `[data-theme]` override selectors (values come from tweakcn exports in D3) |
| `frontend/src/styles/themes/dark.css` | CREATE | Dark theme token overrides (tweakcn export) |
| `frontend/src/styles/themes/light.css` | CREATE | Light theme token overrides |
| `frontend/src/styles/themes/index.css` | CREATE | Imports both theme files |
| `frontend/src/lib/theme/ThemeProvider.tsx` | CREATE | React context + cookie + API sync |
| `frontend/src/lib/theme/cookie.ts` | CREATE | `readThemeCookie`, `writeThemeCookie` |
| `frontend/src/lib/theme/api.ts` | CREATE | `fetchMe()`, `patchTheme(pref)` |
| `frontend/src/lib/theme/__tests__/ThemeProvider.test.tsx` | CREATE | Unit tests (initial resolution, persistence, optimistic rollback) |
| `frontend/src/lib/theme/__tests__/cookie.test.ts` | CREATE | Parse + write unit tests |
| `frontend/src/components/theme-switcher.tsx` | CREATE | UI toggle (Dark / Light / System) |
| `frontend/src/lib/utils.ts` | CREATE via shadcn init | `cn` helper (shadcn scaffolds) |
| `frontend/components.json` | CREATE via shadcn init | shadcn config |
| `frontend/src/components/ui/*.tsx` | CREATE via shadcn add | Button, Input, Label, Select, Combobox, RadioGroup, Checkbox, Switch, Card, Dialog, AlertDialog, Sheet, Table, Tabs, Toast, Tooltip, DropdownMenu, Form, Avatar, Badge, Separator, Skeleton, ScrollArea, Popover |
| `frontend/src/routes/design.tsx` | CREATE | Dev-only route tree; dynamic import target |
| `frontend/src/pages/design/system.tsx` | CREATE | Component gallery route |
| `frontend/src/pages/design/hero/library.tsx` | CREATE | Hero library-grid screen |
| `frontend/src/pages/design/hero/book.tsx` | CREATE | Hero book-detail screen |
| `frontend/src/pages/design/fixtures/` | CREATE | Realistic title/author/cover fixture data (covers from Open Library or public-domain classics) |
| `frontend/src/pages/design/explore/*` | CREATE then DELETE | Three D2 direction spikes; pruned as first step of D3 |
| `.github/workflows/ci.yml` | UPDATE | Add `npm test -- --run`, `npx stylelint`, bundle-leak grep gate to frontend job |

Docs:

| File | Action | Why |
|---|---|---|
| `docs/src/content/docs/design/philosophy.md` | CREATE | D1 deliverable — emotional target, anti-patterns, usage context |
| `docs/src/content/docs/design/visual-identity.md` | CREATE | D3 canonical spec — tokens, type scale, spacing, motion, state philosophy |
| `docs/astro.config.mjs` | UPDATE | Add `Design` sidebar group linking the two docs |

---

## NOT Building

- Frontend business routes beyond app shell + hero screens (library grid that actually queries the API, book detail with real data) — **this is Step 11**. Hero screens are fixture-driven reference only.
- Admin UI, user management, settings page, search UI — Step 11+.
- Additional themes beyond Dark + Light — architected for unlimited, shipped as two.
- Mobile-specific responsive optimisations beyond "usable on tablet"; a dedicated mobile polish pass is out of scope.
- Storybook or any third-party visual-regression tooling — `/design/system` + crosscheck review is the substitute.
- A web reader or OPDS UI — separate product surface.
- Per-component changelog or accessibility audit report documents — VISUAL_IDENTITY.md + crosscheck pass is the artefact.

---

## Step-by-Step Tasks

### Phase D0 — Testing Harness and Direct Dependencies

**Skill:** `superpowers:test-driven-development`

**Task D0.1 — Install Vitest + React Testing Library**

- **ACTION:** `cd frontend && npm install -D vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom`
- **VALIDATE:** `frontend/package.json` devDependencies includes all five; `package-lock.json` updated
- **GOTCHA:** Vitest peer-depends on Vite — already present at `^8.0.4`, compatible

**Task D0.2 — Install design-system direct deps**

- **ACTION:** `cd frontend && npm install react-router lucide-react && npm install -D stylelint @axe-core/cli`
- **NOTES:** `@fontsource/<chosen>` is deferred to D3 task 20 (font decided in D2). `react-hook-form`, `zod`, `@hookform/resolvers` added in D3 only if `Form` primitive is wired.
- **VALIDATE:** `npm run build` still succeeds; deps appear in `package.json`

**Task D0.3 — Create `frontend/vitest.config.ts` (or merge `test` key into `vite.config.ts`)**

- **ACTION:** Prefer merging into `vite.config.ts` (one config source). Add `server.proxy` in the same pass (Task D0.4).
- **IMPLEMENT:** See "FRONTEND_TESTING_HARNESS" and "VITE_PROXY_FOR_SAME_ORIGIN_DEV" patterns
- **MIRROR:** Vitest docs "Configuring Vitest" (see External Documentation)
- **VALIDATE:** `npx vitest run` exits 0 (no tests yet, but harness loads)

**Task D0.4 — Add Vite dev proxy in `vite.config.ts`**

- **ACTION:** Add `server.proxy` forwarding `/api`, `/auth`, `/opds` to `http://localhost:3000`
- **GOTCHA:** `changeOrigin: true` is required for cookie-bearing requests to appear same-origin to the backend
- **VALIDATE:** Manual — start backend at :3000, Vite at :5173, curl `http://localhost:5173/auth/me` returns backend's response

**Task D0.5 — Create `frontend/tests/setup.ts`**

- **ACTION:** Create file with the RTL setup pattern (see "FRONTEND_TESTING_HARNESS")
- **VALIDATE:** `npx vitest run` loads setup without error

**Task D0.6 — Update `frontend/tsconfig.app.json`**

- **ACTION:** Add `"vitest/globals"`, `"vitest/jsdom"`, `"@testing-library/jest-dom"` to `compilerOptions.types`
- **GOTCHA:** `types` replaces default inclusion — if the file doesn't have `types` yet, adding it narrows what's loaded. Verify `tsc -b` still passes after.
- **VALIDATE:** `cd frontend && tsc -b`

**Task D0.7 — Add scripts to `frontend/package.json`**

- **ACTION:** Add:
  - `"test": "vitest run"`
  - `"test:watch": "vitest"`
  - `"test:coverage": "vitest run --coverage"`
  - `"stylelint": "stylelint 'src/**/*.css'"`
- **VALIDATE:** `npm test` works (may have zero test files — should still exit 0 per Vitest behaviour with `--passWithNoTests` or one smoke test)

**Task D0.8 — Commit one smoke test**

- **ACTION:** Create `frontend/src/__tests__/smoke.test.ts`:
  ```typescript
  import { describe, it, expect } from 'vitest';
  describe('smoke', () => {
    it('harness runs', () => expect(1 + 1).toBe(2));
  });
  ```
- **VALIDATE:** `npm test` exits 0 with one passing test

**Task D0.9 — Update CI to run tests**

- **ACTION:** Edit `.github/workflows/ci.yml` frontend job (lines 87–110), add after Lint step:
  ```yaml
      - name: Test
        run: npm test
      - name: Stylelint
        run: npx stylelint 'src/**/*.css' --max-warnings 0 || true  # becomes hard gate in D3
  ```
- **NOTE:** Stylelint left non-blocking in D0 (no config yet); D3 task tightens to hard fail.
- **VALIDATE:** Push branch, CI runs test step

**Task D0.10 — Document TDD scope**

- **ACTION:** Create `docs/src/content/docs/design/testing-scope.md` (can be consolidated into philosophy.md in D1):
  ```markdown
  ---
  title: Testing Scope for the Design System
  ---

  The Step 10 design system is tested at two distinct bars:

  - **Deterministic logic (unit tests, mandatory):** theme provider
    (initial resolution from cookie/DB/prefers-color-scheme, persistence, API
    sync), cookie helpers, custom ESLint hex-literal rule fixtures, route-gating
    production-build assertion.
  - **Visual / composition work (exempt from unit tests):** verified by
    `@axe-core/cli`, Lighthouse, manual Dark/Light toggle, and the `/crosscheck`
    dual-model review gate.
  ```
- **VALIDATE:** Docs site build (`cd docs && npm run build`) succeeds after sidebar update in D3

**Task D0.11 — Verify cookie middleware integration end-to-end**

- **ACTION:** Add `axum-extra = { version = "0.10", features = ["cookie"] }` as a direct dep in `backend/Cargo.toml`.
- **ACTION:** Write two throwaway test handlers in a temporary branch of the router (or as `#[sqlx::test]` harness fixtures):
  1. `GET /_test/cookie-ok` — returns `(jar.add(Cookie::new("test_ok", "1")), StatusCode::OK)`
  2. `GET /_test/cookie-redirect` — returns `(jar.add(Cookie::new("test_redirect", "1")), Redirect::temporary("/"))`
- **ACTION:** Integration tests via `axum_test::TestServer` assert BOTH responses include the expected `Set-Cookie` header.
- **RATIONALE:** The entire design-system backend sliver (theme PATCH + OIDC callback cookie write + FOUC seed) depends on `CookieJar` working as an extractor and returnable type. Verify the contract works for both OK and Redirect responses BEFORE starting D3.5. Failing fast here is the whole point of D0.
- **VALIDATE:** Both tests pass. Remove the throwaway routes before exiting D0 (or roll them into a `#[cfg(test)]` module that never registers them outside tests).

**D0 Exit Gate:** `npm test` green, `cargo test` green (including the cookie-middleware verification), deps visible in package.json + Cargo.toml, TDD scope documented.

---

### Phase D1 — Conceptual Foundation

**Skill:** `superpowers:brainstorming`

**Creative phase — no file-level task breakdown.** Per BLUEPRINT lines
1749–1758:

1. Brainstorm-driven exploration of Reverie's emotional target (private library
   vs reading sanctuary vs exploration space vs other)
2. Identify core tensions (contemplative vs efficient, ornate vs minimal,
   ambient vs energetic)
3. Enumerate explicit anti-patterns — what the product is NOT
4. Capture usage context — when, where, how long, what mood the user is in
5. Theme strategy — which themes are must-have at launch, which are deferred polish

**Deliverable:** `docs/src/content/docs/design/philosophy.md` (1–2 Starlight
pages with frontmatter `title:`). Fold the D0.10 testing-scope note into this
if convenient.

**D1 Exit Gate:** Document human-reviewed; design direction concrete enough to
drive D2 variations.

---

### Phase D2 — Visual Exploration

**Skills:** `frontend-design`, `ui-ux-pro-max`, tweakcn browser tool.

**Creative phase — no file-level task breakdown.** Per BLUEPRINT lines
1760–1768:

1. Generate three *genuinely distinct* coded directions — not variations of one
   palette
2. Each direction produces: full token set (colours × Dark + Light minimum,
   type scale, spacing, motion), applied to ~3 representative screens (library
   grid, book detail, search) against realistic fixture data
3. Live-browseable at `/design/explore/[name-a]`, `/design/explore/[name-b]`,
   `/design/explore/[name-c]`
4. Use tweakcn to generate token exports per direction; commit as
   `frontend/src/design/explore/[name]/tokens.css`
5. Route these under the same dynamic-import dev gate established later
   (D3 task D3.12) — fine to use a provisional static import in D2 and convert
   during D3.1 pruning

**Deliverable:** Three working `/design/explore/*` routes with distinct visual
directions, each themeable.

**D2 Exit Gate:** Subjective taste review — one direction (or a synthesis of
two) clearly wins. **Record the decision in a short note at the top of
`philosophy.md` or as a committed changelog entry.**

---

### Phase D3 — Codify Design System

**Skills:** `design-system`, `accessibility`.

**Task D3.1 — Prune D2 exploration artefacts (first action in D3)**

- **ACTION:** Delete `frontend/src/pages/design/explore/*` (all three directions), delete `frontend/src/design/explore/*` token files. Keep only the winning direction's tokens as the seed for the canonical theme CSS (Task D3.7).
- **RATIONALE:** Working on top of three stale trees muddies every D3 review.
- **VALIDATE:** `rg "design/explore" frontend/src/` returns nothing

**Task D3.2 — Create the theme-preference migration**

- **ACTION:** Generate the migration timestamp at write-time: `STAMP=$(date +%Y%m%d000001)`. Create `backend/migrations/${STAMP}_add_theme_preference.up.sql` with `ALTER TABLE users ADD COLUMN theme_preference TEXT NOT NULL DEFAULT 'system';`
- **ACTION:** Create `${STAMP}_add_theme_preference.down.sql` with `ALTER TABLE users DROP COLUMN theme_preference;`
- **MIRROR:** `backend/migrations/20260414000001_add_session_version.*` verbatim
- **VALIDATE:** `cd backend && sqlx migrate run` succeeds against a fresh DB; `sqlx migrate revert` cleanly reverts

**Task D3.3 — Extend the user model**

- **ACTION:** Make all four edits per the `USER_MODEL_COLUMN_ADDITION` pattern:
  1. Append `theme_preference` to `USER_COLUMNS` constant (`backend/src/models/user.rs:7-8`)
  2. Add `theme_preference: String` field to `UserRow` struct (line 11)
  3. Add `pub theme_preference: String` field to `User` struct (line 24)
  4. Add `theme_preference: row.theme_preference,` to `From<UserRow> for User` impl (line 39)
- **ACTION:** Add `const ALLOWED_THEMES: &[&str] = &["system", "light", "dark"];` (location: `backend/src/auth/theme_cookie.rs` alongside `THEME_COOKIE_NAME`, or near the PATCH handler in `routes/auth.rs` — pick one location, don't duplicate)
- **VALIDATE:** `cargo build -p reverie-api` succeeds; existing user tests still pass. Missing any of the four edits produces a clear compile error.

**Task D3.4 — Update `/auth/me` response**

- **ACTION:** In `backend/src/routes/auth.rs:162–177`, add `"theme_preference": u.theme_preference` to the JSON
- **VALIDATE:** Add `#[sqlx::test]` that hits GET `/auth/me` and asserts the field present with default `"system"`

**Task D3.5 — Implement `PATCH /auth/me/theme`**

- **ACTION:** Create `backend/src/auth/theme_cookie.rs` with `THEME_COOKIE_NAME` const and `set_theme_cookie(jar, value) -> CookieJar` helper per "THEME_COOKIE_WRITER" pattern. `axum-extra` dep was added in D0.11.
- **ACTION:** Append `update_theme` handler to `backend/src/routes/auth.rs` per "PATCH_HANDLER_SHAPE" pattern — signature takes `jar: CookieJar`, returns `(CookieJar, Json<_>)`, validates against `ALLOWED_THEMES`, uses `AppError::Validation` (not `BadRequest` — see the pattern's inline comment).
- **ACTION:** Register route in `routes::auth::router()`: `.route("/auth/me/theme", patch(update_theme))`
- **ACTION:** Update the OIDC `callback` handler (`backend/src/routes/auth.rs:68–152`):
  - Extractor list gains `jar: CookieJar`
  - Return type changes from `impl IntoResponse` to `(CookieJar, Redirect)`
  - Immediately after `auth_session.login(&user)` succeeds, call `let jar = set_theme_cookie(jar, &user.theme_preference);`
  - Final return becomes `Ok((jar, Redirect::temporary("/")))`
- **ACTION:** Unit-test `set_theme_cookie` in isolation — given a fresh `CookieJar` and the value `"dark"`, assert the returned jar's cookie has name `THEME_COOKIE_NAME` (string-compared to `"reverie_theme"` so a rename of the const fails the test), `http_only = false`, `same_site = Lax`, `path = "/"`, `max_age` equals one year. This test is the enforcement for cookie-name drift across the three locations tracked in UNK-105.
- **VALIDATE:** Integration test (see `SQLX_TEST_HARNESS` pattern — two test cases provided there covering the happy path and invalid-value rejection):
  1. Happy path: PATCH with `{"theme_preference": "dark"}` returns 200, column is updated, `Set-Cookie: reverie_theme=dark` present in response
  2. Rejection: PATCH with `{"theme_preference": "purple"}` returns **422** (`AppError::Validation`), no row modified
- **NOTE:** End-to-end OIDC callback success-path test (including "Set-Cookie includes reverie_theme") is tracked separately under [UNK-104](https://linear.app/unkos/issue/UNK-104) — requires `wiremock` + signed-ID-token scaffolding that doesn't yet exist in the project. Don't bundle that work into this PR.

**Task D3.6 — Init shadcn/ui via CLI (zero-prompt)**

- **ACTION (path aliases, prerequisite):** Before running `shadcn init`, add path aliases:
  - `frontend/tsconfig.app.json`: add `"baseUrl": "."` and `"paths": { "@/*": ["src/*"] }` to `compilerOptions`
  - `frontend/vite.config.ts`: add `resolve: { alias: { "@": path.resolve(__dirname, "src") } }` (import `path` from `node:path`)
- **ACTION (pre-write `components.json`):** Write `frontend/components.json` with the contents from the `SHADCN_COMPONENTS_JSON` pattern BEFORE running init. Pre-writing the config means shadcn's init is zero-prompt (no stdin interaction).
- **ACTION:** `cd frontend && npx shadcn@latest init --yes` — picks up the pre-written `components.json`. Init generates `src/lib/utils.ts` (the `cn` helper) and updates `src/index.css`.
- **GOTCHA (Feb 2026 unified package):** the current shadcn CLI generates components that import from the unified `radix-ui` package rather than individual `@radix-ui/react-*` modules. Expect one big `radix-ui` dep in `package.json` instead of many `@radix-ui/react-*` entries — this is correct, not a bug.
- **FALLBACK:** If `--yes` does not skip all prompts in the installed CLI version, run `npx shadcn@latest init --help` first and capture the current non-interactive flag set; adjust accordingly. Do NOT run interactive init — it blocks on stdin in CI/agent contexts.
- **VALIDATE:** `npm run build` succeeds; `npm run lint` passes; `@/components/...` and `@/lib/utils` imports resolve correctly in a test file.

**Task D3.7 — Commit Tailwind v4 multi-theme CSS**

- **ACTION:** Replace `frontend/src/index.css` contents with the "TAILWIND_V4_MULTI_THEME" pattern; fill the `<tweakcn-export>` placeholders with the winning D2 direction's tokens (Dark + Light from tweakcn)
- **ACTION:** Create `frontend/src/styles/themes/{dark,light,index}.css` if you prefer split files (import the index from `index.css`)
- **VALIDATE:** `/design/system` (built in D3.11) shows visible theme swap when `data-theme` flips on `<html>`

**Task D3.8 — Add shadcn primitives**

- **ACTION:** Install the Step 11 primitive set via CLI non-interactively. `combobox` is **not** a standalone shadcn primitive — it is a composed pattern built from `command` + `popover` + `cmdk`. Pass `--yes` to skip confirmation prompts:
  ```bash
  npx shadcn@latest add --yes \
    button input label select command popover \
    radio-group checkbox switch card dialog alert-dialog sheet table tabs \
    sonner tooltip dropdown-menu form avatar badge separator skeleton \
    scroll-area
  ```
  (Notes: `sonner` is the Toast primitive in current shadcn; `command` + `popover` compose into Combobox — see [shadcn Combobox docs](https://ui.shadcn.com/docs/components/combobox) for the composition pattern.)
- **ACTION:** If `--yes` does not auto-accept peer-dep installation prompts (`react-hook-form`, `zod`, `@hookform/resolvers` for Form; `cmdk` for command), manually `npm install react-hook-form zod @hookform/resolvers cmdk` first and re-run `add --yes`.
- **VALIDATE:** All files appear under `frontend/src/components/ui/`; `npm run build` succeeds

**Task D3.9 — Restyle every primitive against the token system**

- **ACTION:** Go through each `frontend/src/components/ui/*.tsx` and replace default spacing/radius/colour utility classes with token-bound equivalents. Example: `bg-white` → `bg-surface`; `rounded-md` → `rounded-[var(--radius-md)]` or a token-backed utility class if `@theme` declares one. Kill shadcn's stock visual DNA.
- **ACTION:** Extract repeated class string groups into a `cva` composition if they appear in ≥3 primitives (shadcn already uses `cva` under the hood — extend, don't parallel it)
- **VALIDATE:** `/design/system` (D3.11) renders every primitive without any hardcoded hex; lint + stylelint pass (see D3.14 hex bans)

**Task D3.10 — Theme provider + switcher + API client**

- **ACTION:** Create `frontend/src/lib/theme/{ThemeProvider.tsx,cookie.ts,api.ts}` per "THEME_PROVIDER" pattern
- **ACTION:** Create `frontend/src/components/theme-switcher.tsx` — uses `DropdownMenu` primitive with System / Light / Dark options
- **ACTION:** Mount `<ThemeProvider>` in `frontend/src/main.tsx` wrapping `<RouterProvider>`
- **ACTION (cross-tab sync, C3):** Inside `ThemeProvider`, create a `BroadcastChannel('reverie-theme')` in a `useEffect` on mount (close on unmount). On successful `setPreference` (after the PATCH resolves), post the new preference to the channel. On receive, mirror the value to local state + DOM + cookie WITHOUT triggering another PATCH (the originating tab already did). This eliminates the cross-tab-drift papercut.
- **ACTION:** TDD — write these tests FIRST per D0 TDD scope (see Testing Strategy section):
  - `cookie.test.ts`: round-trip parse/write, malformed cookie handling
  - `ThemeProvider.test.tsx`: initial resolution from `document.documentElement.dataset.theme`; reconciliation with server value on mount; optimistic update + rollback on PATCH failure; `system` preference reacts to `prefers-color-scheme` media query change; **BroadcastChannel message from another tab updates state without triggering a PATCH**
- **VALIDATE:** `npm test` all green; `/design/system` theme-switcher cycles through states; manually open two tabs, change theme in one, verify the other updates without reload

**Task D3.11 — Component gallery at `/design/system`**

- **ACTION:** Create `frontend/src/pages/design/system.tsx` — for every primitive, render it in every state (default, hover, focus, active, disabled, error, loading) in both themes (switcher at top of page)
- **ACTION:** Wire the route via the dynamic-import pattern (Task D3.12)
- **VALIDATE:** `npm run dev`, navigate to `/design/system`, manually toggle theme, every primitive renders correctly in both

**Task D3.12 — Dev-only route tree + dynamic gating + structural bundle gate**

- **ACTION:** Create `frontend/src/routes/design.tsx` exporting `designRoutes` (array of `RouteObject`)
- **ACTION:** In `main.tsx` (or a `routeTree.ts`), gate via:
  ```typescript
  const routes = [...prodRoutes];
  if (import.meta.env.DEV) {
    const { designRoutes } = await import('./routes/design');
    routes.push(...designRoutes);
  }
  ```
- **ACTION (structural bundle gate):** In `frontend/vite.config.ts`, configure `build.rollupOptions.output.manualChunks` to route all design-tree modules into a dedicated `design` chunk:
  ```typescript
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('/src/routes/design/') ||
              id.includes('/src/pages/design/')) {
            return 'design';
          }
        },
      },
    },
  },
  ```
  In production mode, Vite tree-shakes the entire `design-*` chunk because `import.meta.env.DEV` is replaced with the literal `false` and the `if`-branch is dead code. No `design-*.js` is emitted. In dev mode the chunk is emitted because the branch executes.
- **VALIDATE (structural, not substring):** `npm run build && test -z "$(ls frontend/dist/assets/design-*.js 2>/dev/null)"` exits zero (no chunk emitted). Substring grep against minified output is unreliable (Vite mangles names); this check is against the build manifest structure.

**Task D3.13 — Replace placeholder contents of `frontend/src/fouc/fouc.js`**

- **ACTION:** Replace the current 5-line placeholder in `frontend/src/fouc/fouc.js` with the FOUC_INLINE_SCRIPT body shown above (JS only — no surrounding `<script>` tags; the Vite plugin wraps it). Do **not** touch the `<!-- reverie:fouc-hash -->` marker or its location in `index.html`.
- **ACTION:** Separately, update `<title>frontend</title>` → `<title>Reverie</title>` on line 7 of `frontend/index.html`.
- **CONSTRAINT:** Script body must not contain the literal `</script>` (case-insensitive). `vite-plugins/csp-hash.ts` throws at build time if present, because a raw `</script>` in inline script content would escape the element and render as HTML.
- **VALIDATE (build regen):** `npm run build` succeeds and `dist/csp-hashes.json` contains a single `sha256-...` entry whose value matches `openssl dgst -sha256 -binary frontend/src/fouc/fouc.js | base64`. Backend dist-validation picks this up on its next start.
- **VALIDATE (happy path):** Set `reverie_theme=dark` cookie, hard-reload, open devtools, confirm `<html data-theme="dark">` is set before any React mount event.
- **VALIDATE (catch-block path):** Set `reverie_theme=<malformed>` (e.g. `reverie_theme=` with a control character, or `reverie_theme=javascript:alert(1)`), hard-reload, confirm `<html data-theme="light">` (the try/catch fallback). The catch branch handles malformed-cookie cases at runtime; JS-disabled is out of scope (the entire app is React; unstyled no-JS rendering is not a supported configuration).

**Task D3.14 — ESLint + Stylelint hex bans**

- **ACTION:** Edit `frontend/eslint.config.js` — add to the existing `files: ['**/*.{ts,tsx}']` block:
  ```javascript
  rules: {
    'no-restricted-syntax': ['error', {
      selector: "Literal[value=/^#[0-9a-fA-F]{3,8}$/]",
      message: 'No raw hex codes in .tsx. Use semantic tokens (bg-surface, text-ink, etc.).',
    }],
  },
  ```
- **ACTION (Stylelint, first-party only):** `npm install -D stylelint stylelint-config-standard`. Do NOT install any third-party Tailwind-aware Stylelint config — the false-positives on Tailwind v4 at-rules are resolved via the built-in `at-rule-no-unknown` ignore list.
- **ACTION:** Create `frontend/.stylelintrc.json`:
  ```json
  {
    "extends": ["stylelint-config-standard"],
    "rules": {
      "at-rule-no-unknown": [true, {
        "ignoreAtRules": [
          "theme", "custom-variant", "layer", "utility",
          "apply", "config", "tailwind", "source", "variant"
        ]
      }]
    },
    "overrides": [
      {
        "files": ["src/**/*.css", "!src/styles/themes/**/*.css"],
        "rules": { "color-no-hex": true }
      }
    ]
  }
  ```
  The negated glob exempts theme token files where canonical hex values live; `color-no-hex` is built-in to Stylelint 16. If Tailwind adds a new at-rule in a future release, append it to `ignoreAtRules` — one-line change per Tailwind release (rare).
- **ACTION (rule-correctness test, in-process):** Test the ESLint hex-ban rule via ESLint's own `RuleTester` — no subprocess spawn, no fixture files:
  ```typescript
  // frontend/src/__tests__/hex-ban.test.ts
  import { RuleTester } from 'eslint';
  import rule from '../../eslint-rules/no-restricted-syntax'; // or import the config rule set
  const tester = new RuleTester({ languageOptions: { ecmaVersion: 2022, sourceType: 'module' } });
  tester.run('hex-ban', rule, {
    valid: [
      { code: 'const c = "hello";' },
      { code: 'const c = bgSurface;' },
    ],
    invalid: [
      { code: 'const c = "#abc123";', errors: 1 },
      { code: 'const c = "#fff";', errors: 1 },
    ],
  });
  ```
  In-process, millisecond runtime, deterministic cross-platform. No `.fixture.tsx` files, no `spawn('eslint', …)`.
- **ACTION:** Tighten CI (D0.9): remove `|| true` on the stylelint step; add `npx eslint src --max-warnings 0` if not already covered by `npm run lint`
- **VALIDATE:** `npx stylelint 'src/**/*.css' --max-warnings 0` and `npm run lint` both exit 0; deliberately introduce a hex literal in a non-theme file — both fail as expected; revert. `npm test` runs the `RuleTester`-based hex-ban test in under 100ms.

**Task D3.15 — Motion + state tokens**

- **ACTION:** Extend `@theme inline` with motion tokens (`--duration-fast`, `--duration-slow`, `--ease-standard`, `--ease-emphasised`). Extend with empty/loading/error state philosophy — specifically which primitives have `Skeleton` treatment, whether loading states show shimmer or just pulse.
- **ACTION:** Document in `visual-identity.md` (see Task D3.18)
- **VALIDATE:** No code validation — reviewed in D5 crosscheck

**Task D3.16 — Self-hosted fonts via `@fontsource`**

- **ACTION:** `npm install @fontsource/<display-font> @fontsource/<body-font>` — versions tracked in package.json
- **ACTION:** Import weights + subsets from `main.tsx`: `import '@fontsource/<body>/400.css'; import '@fontsource/<body>/600.css';` etc.
- **ACTION:** Update `@theme inline` `--font-display` and `--font-body` to reference the font family names registered by fontsource
- **VALIDATE:** Network panel in devtools shows font files loading from `/node_modules/@fontsource/…` via Vite; no external font requests

**Task D3.17 — Accessibility pass**

- **ACTION:** For every primitive in `/design/system`, verify:
  - Visible focus indicator in both themes (ring utility or outline token)
  - Full keyboard navigation (tab / shift-tab / enter / space / arrow)
  - WCAG 2.2 AA contrast for all text over backgrounds
- **ACTION:** Run `npx @axe-core/cli http://localhost:5173/design/system --exit` (dev server running) — fix any violations. **The `--exit` flag is mandatory for CI gating**; without it `@axe-core/cli` always exits 0 regardless of violations.
- **ACTION:** Document allowed focus-ring style in `visual-identity.md`
- **VALIDATE:** axe-core exits 0

**Task D3.18 — Canonicalise in `docs/design/visual-identity.md`**

- **ACTION:** Create `docs/src/content/docs/design/visual-identity.md` with sections: Tokens (full list), Type Scale, Spacing, Motion, State Philosophy (empty/loading/error), Theme Architecture
- **ACTION (Theme Architecture content):** Include explicit notes:
  - "Cookie name `reverie_theme` is referenced in three places: `backend/src/auth/theme_cookie.rs` (`THEME_COOKIE_NAME` const), `frontend/src/fouc/fouc.js` (inline FOUC script body), `frontend/src/lib/theme/cookie.ts`. All three MUST change together. The backend unit test on `set_theme_cookie` enforces the backend side; cross-stack drift is tracked in [UNK-105](https://linear.app/unkos/issue/UNK-105)."
  - "FOUC avoidance relies on a blocking inline `<script>` injected into `index.html` at the `<!-- reverie:fouc-hash -->` marker by `frontend/vite-plugins/csp-hash.ts`. The script body lives in `frontend/src/fouc/fouc.js`; on `vite build` the plugin emits `dist/csp-hashes.json` containing the SHA-256 of the body, and `backend/src/security/dist_validation.rs` reads that at startup to bake the hash into the HTML-route CSP header. The CSP itself is hash-based — there is no per-request nonce and no backend templating of `index.html`. Any change to `fouc.js` regenerates the hash automatically; hash drift between frontend and backend is fail-fast at boot."
- **ACTION:** Update `docs/astro.config.mjs` sidebar:
  ```javascript
  {
    label: 'Design',
    items: [
      { label: 'Philosophy', slug: 'design/philosophy' },
      { label: 'Visual Identity', slug: 'design/visual-identity' },
    ],
  },
  ```
- **VALIDATE:** `cd docs && npm run build` succeeds; both pages reachable in the built site

**Task D3.19 — Smoke-test an extra theme**

- **ACTION:** Add a throwaway third theme file (e.g. `sepia.css`) with minimally-plausible values; confirm adding `[data-theme="sepia"]` in the switcher + the extra CSS file works end-to-end with no architectural change
- **ACTION:** Delete the throwaway file before commit (or keep as a docs example in `visual-identity.md`)
- **VALIDATE:** Toggle to `sepia` in devtools, `data-theme="sepia"` on `<html>`, tokens apply — architecture confirmed theme-unlimited

**D3 Exit Gate:** Gallery complete; both themes pass WCAG AA; a11y clean;
no primitive shows stock shadcn DNA; production bundle free of `/design` code
(structural manualChunks gate passes — no `design-*.js` in `dist/assets/`).

---

### Phase D4 — Hero Screens

**Task D4.1 — Library grid hero (`/design/hero/library`)**

- **ACTION:** Create `frontend/src/pages/design/hero/library.tsx`
- **ACTION:** Create `frontend/src/pages/design/fixtures/books.ts` with ~30 realistic entries: real titles, real authors, real-looking covers (public-domain classics via Open Library cover URLs or `/public/fixtures/*.jpg`), varied series membership, long/short title edge cases
- **ACTION:** Render a production-fidelity grid: cover, title, author, series badge, responsive breakpoints (desktop 4-col, tablet 3-col, mobile 2-col), empty/loading/error treatments
- **VALIDATE:** Dark + Light both render; `npx @axe-core/cli http://localhost:5173/design/hero/library` exits 0; Lighthouse > 90 on Performance, Accessibility, Best Practices

**Task D4.2 — Book detail hero (`/design/hero/book`)**

- **ACTION:** Create `frontend/src/pages/design/hero/book.tsx`
- **ACTION:** Production-fidelity book detail: hero cover, metadata (title, author, series, ISBN, publisher, language), description block, version history placeholder (static fixture), action buttons (Download, Accept Draft, Edit — all fixture-bound), tabs for Metadata/Versions/Shelves/Health
- **VALIDATE:** Dark + Light both render; axe-core exits 0; Lighthouse > 90

**Task D4.3 — Responsive validation**

- **ACTION:** Validate both hero routes at 1440×900, 1024×768, 375×812 breakpoints
- **ACTION:** Fix any layout collapses or overflow; document responsive behaviour in `visual-identity.md` breakpoint section
- **VALIDATE:** Manual screenshot pass in both themes × three breakpoints

**D4 Exit Gate:** Both hero routes render at production fidelity; both themes;
responsive; Lighthouse > 90; axe-core clean.

---

### Phase D5 — Review Gate

**Task D5.1 — Run `/crosscheck`**

- **ACTION:** Invoke `/crosscheck` skill against the design artefacts: `docs/design/*.md`, `frontend/src/styles/themes/*`, `frontend/src/components/ui/*`, `frontend/src/pages/design/*`
- **ACTION:** If either Opus or Gemini reviewer flags significant issues, loop back to D3 or D4 and iterate
- **VALIDATE:** Both reviewers pass

**D5 Exit Gate:** Crosscheck green. Step 11 unblocked.

---

## Testing Strategy

### Unit Tests (mandatory — D0 TDD scope)

| Test file | Test cases | Validates |
|---|---|---|
| `frontend/src/lib/theme/__tests__/cookie.test.ts` | parse missing / malformed / well-formed; write; round-trip | Cookie helper correctness |
| `frontend/src/lib/theme/__tests__/ThemeProvider.test.tsx` | initial resolution from `data-theme` attribute; fetch-me reconciliation; optimistic setter + PATCH success; optimistic setter + PATCH failure (rollback); `system` preference reacts to `matchMedia` change; **BroadcastChannel message from another tab updates state without triggering a PATCH** | Theme state machine + cross-tab sync |
| `frontend/src/components/__tests__/theme-switcher.test.tsx` | renders three options; selecting calls `setPreference`; disabled state when mutation pending | UI behaviour |
| `frontend/src/__tests__/hex-ban.test.ts` | ESLint `RuleTester` (in-process, no subprocess): valid cases pass, hex-literal cases fail with the expected message | Lint rule correctness |
| `backend/src/auth/theme_cookie.rs` unit test | `set_theme_cookie(jar, "dark")` produces a cookie with name `"reverie_theme"` (verbatim string compare — enforces UNK-105 cross-stack const), `http_only = false`, `same_site = Lax`, `path = "/"`, `max_age = 365 days` | Cookie helper correctness + cross-stack name drift guard |
| `backend/src/routes/auth.rs` tests (inline `#[sqlx::test]`) | migration adds column with default `'system'`; `GET /auth/me` includes the field; `PATCH /auth/me/theme` with valid body returns 200, updates row, emits `Set-Cookie: reverie_theme=…`; invalid body returns **422** (`AppError::Validation`) and does not modify the row | Backend contract |
| `backend/_test/cookie-{ok,redirect}` integration (D0.11; throwaway routes) | `CookieJar` extractor + tuple return emits `Set-Cookie` for both `OK` and `Redirect` responses | Cookie middleware integration verified BEFORE D3.5 |

OIDC callback successful-flow test (asserting `Set-Cookie: reverie_theme=…`
after login) is tracked separately under [UNK-104](https://linear.app/unkos/issue/UNK-104).

### Integration Tests (in D3 scope)

- Production build structural gate (CI): `npm run build && test -z "$(ls frontend/dist/assets/design-*.js 2>/dev/null)"` exits zero (no `design-*` chunk emitted in production)
- axe-core on `/design/system` + both hero routes
- Lighthouse (manual) on `/design/hero/library`

### Edge Cases Checklist

- [ ] Empty cookie string (no `reverie_theme=`) → falls back to `prefers-color-scheme`
- [ ] Malformed cookie value (e.g. `reverie_theme=bogus`) → FOUC script's catch falls back to `light`
- [ ] `system` preference + OS theme change mid-session → effective theme updates without reload
- [ ] Logged-out visitor → no `/auth/me` call fails provider init (provider detects 401 and stays on cookie value)
- [ ] Two tabs open, theme changed in one → BroadcastChannel propagates the change to the other tab in real time (no reload required)
- [ ] Logout → session cookie cleared; `reverie_theme` cookie persists (user's device preference, not session state)
- [ ] Invalid theme in PATCH body → 422 (`AppError::Validation`), no row modified
- [ ] Revert migration mid-development → row data loss (acceptable pre-release per repo memory)

---

## Validation Commands

See BLUEPRINT.md Step 10 § Verification (lines 1822–1856) — already updated by
this plan's adversarial-review pass to include:

- `cargo test` (includes new `#[sqlx::test]`s + the `set_theme_cookie` unit test + the D0.11 cookie-middleware verification)
- `cargo clippy -- -D warnings`
- `npm run build && npm run lint && npm test -- --run`
- `npx @axe-core/cli` against `/design/system`, `/design/hero/library`, `/design/hero/book`
- `npx eslint frontend/src --max-warnings 0` + `npx stylelint "frontend/src/**/*.css" --max-warnings 0`
- Production bundle structural gate: `npm run build && test -z "$(ls frontend/dist/assets/design-*.js 2>/dev/null)"` exits zero (no `design-*` chunk in production output)
- Manual cold-load FOUC check (happy path + malformed-cookie path) + Lighthouse audit
- Manual two-tab cross-tab theme sync check (BroadcastChannel)

---

## Acceptance Criteria

Mirrors BLUEPRINT Step 10 Exit Criteria (lines 1859–1870):

- [ ] `docs/design/philosophy.md` captures emotional target, anti-patterns, usage context
- [ ] `docs/design/visual-identity.md` is the canonical spec: tokens, type scale, spacing, motion, state philosophy, theme architecture
- [ ] Dark + Light themes implemented as CSS variable overrides under `[data-theme]`; theme switcher works; preference persists across reload and across devices (DB + cookie)
- [ ] Cross-tab theme changes propagate in real time via `BroadcastChannel('reverie-theme')`
- [ ] shadcn primitives installed and restyled — none show stock shadcn visual DNA
- [ ] `/design/system` route shows every primitive in every state; both themes
- [ ] `/design/hero/library` and `/design/hero/book` render at production fidelity
- [ ] WCAG 2.2 AA contrast in both themes (axe-core + manual)
- [ ] ESLint blocks raw hex literals in `.tsx` (verified by in-process `RuleTester` test); Stylelint blocks raw hex in `.css` outside `src/styles/themes/**`
- [ ] Crosscheck (Opus + Gemini) passes on design artefacts and hero screens
- [ ] Architecture supports unlimited themes (proven via D3.19 smoke test)
- [ ] First paint on cold load matches stored theme preference — no FOUC; malformed-cookie path falls back to `light`
- [ ] CI structural bundle gate: no `design-*.js` chunk in `frontend/dist/assets/` in production builds
- [ ] `set_theme_cookie` unit test enforces the canonical `"reverie_theme"` cookie name (UNK-105 cross-stack drift guard)

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| shadcn restyle work blows up scope (23 primitives × many states) | MED | MED | Restyle in batches; set a bounded-ambition rule: kill stock shadcn *visual* DNA (spacing/radius/colour), keep structural classes; if a primitive needs invasive rework, log it and defer to Step 11 |
| Tailwind v4 `@theme inline` semantics differ subtly across minor versions | LOW | MED | `package-lock.json` is committed and CI uses `npm ci`, so builds are version-reproducible. Renovate (already running on this repo) opens reviewable PRs for any Tailwind minor/major bump — semantic changes to `@theme inline` would surface there before merging. Verify utility generation by eyeballing `/design/system` after each batch. |
| FOUC script breaks on older browsers | LOW | LOW | Plain ES5, try/catch fallback to `light`; no modern APIs required |
| `@theme inline` prevents Tailwind from generating some utilities that reference unresolved runtime values | LOW | MED | If discovered during D3.9, fall back to split utilities (stable tokens in `@theme`, runtime-swapped values in component classes via `var()`) — documented in shadcn Tailwind v4 guide |
| Vite dev proxy misconfigures cookie domain | LOW | HIGH | `changeOrigin: true` is load-bearing; test by inspecting `document.cookie` after login — if session cookie appears, theme cookie will too. Dev topology unchanged post-CSP: Vite still serves `index.html` in dev (the `csp-hash.ts` plugin participates in `serve` as well as `build`). Prod topology: backend serves `/` and SPA fallback via `backend/src/routes/spa.rs`, with startup dist-validation gating on the FE/BE hash match. |
| `fouc.js` content contains `</script>` | LOW | HIGH | `vite-plugins/csp-hash.ts` throws at build time if the body contains `</script>` (case-insensitive). Keep FOUC body pure ES5; never build script literals with `</script>` substrings. Test fixture in `vite-plugins/__tests__/csp-hash.test.ts` covers this. |
| FOUC edit lands without hash regen in deploy | LOW | HIGH | Cannot happen in practice: the plugin runs on every `vite build`, `csp-hashes.json` ships in `dist/`, and `backend/src/security/dist_validation.rs` fails the server boot if the frontend hash doesn't match the CSP the backend would emit. Manual sanity check: `openssl dgst -sha256 -binary frontend/src/fouc/fouc.js \| base64` should equal the value in `dist/csp-hashes.json` after build. |
| Crosscheck fails at D5 on a high-cost iteration loop | MED | HIGH | Don't run crosscheck on a broken build — walk the exit gates at D3 and D4 manually first; iterate D3/D4 tightly before invoking D5 |
| Migration revert in production loses user theme preferences | LOW (pre-release) | LOW | Acknowledged in BLUEPRINT rollback; pre-release schema is mutable per repo memory |
| Third-party font licensing overlooked during D2/D3 font selection | LOW | HIGH | Constrain font choice to SIL OFL / Apache 2.0 / `@fontsource` catalogue (all bundled fonts are explicitly licensed) |

---

## Rollback

Per BLUEPRINT line 1872: revert branch. Frontend returns to default Vite
scaffold. DB migration reverts with `sqlx migrate revert` (drops the
`theme_preference` column; pre-release data loss acceptable). Step 11 stays
blocked.

---

## Notes

- **The BLUEPRINT step is the spec.** This plan does not duplicate BLUEPRINT prose; it operationalises it into file-level tasks with patterns to mirror and gotchas discovered during codebase exploration. If BLUEPRINT and this plan conflict, BLUEPRINT wins and this plan should be amended.
- **No existing frontend patterns to mirror.** The frontend is a zero-test, zero-pattern Vite scaffold. "Patterns to Mirror" borrows from the backend for the single backend sliver, and from external docs (Vitest, shadcn, Tailwind v4) for the frontend. First frontend PRs set the patterns that Step 11+ will mirror.
- **Dev cross-origin vs prod same-origin** is a genuine production/dev parity concern. Vite proxy (D0.4) resolves this; without it, the session cookie set at `:3000` is invisible at `:5173`. Revisit in Step 11 if the production topology changes.
- **`users` has no RLS.** Verified by exhaustive migration search — the only `ENABLE ROW LEVEL SECURITY` is on `manifestations`. Handlers against `users` query `state.pool` directly; no `acquire_with_rls` wrapper.

---

## Confidence Score

**8/10** for one-pass implementation success (post-2026-04-23 adversarial
review; post-2026-04-24 CSP reconciliation after UNK-106 shipped).

**Rationale for 8:**

- **Confident** on the backend sliver (migration, four-point `User` model edit, `/auth/me` extension, PATCH handler with `axum-extra::CookieJar` tuple return) — direct mirror of an established pattern, no RLS complication, existing test harness, helper signatures verified.
- **Confident** on the infrastructure (Vitest harness, Vite proxy, CI updates, ESLint `RuleTester` + Stylelint built-in at-rule list, pre-written `components.json` for zero-prompt shadcn init, Tailwind v4 multi-theme structure, structural manualChunks bundle gate) — well-documented external patterns, all major load-bearing decisions verified during review.
- **Medium confidence** on D3 primitive restyling — the task list is concrete but the *volume* of primitives × states is significant and design quality is subjective. Crosscheck at D5 is the safety net.
- **Medium confidence** on D1/D2 creative phases — these are deliberately open-ended. The plan cannot drive them to a single answer; exit gates rely on human review.
- **Verified load-bearing assumptions** (during 2026-04-23 review): `axum-extra::CookieJar` is the correct mechanism (no `CookieManagerLayer` mounting question, composes with `Redirect` via tuple return); helper signatures `create_adult_and_basic_auth(pool, name)` and `server_with_real_pools(app_pool, ingestion_pool)` confirmed in `backend/src/test_support.rs`; `AppError::Validation` (422) confirmed as the project's chosen error variant for input validation (no `BadRequest`).
- **Known unknowns:** tweakcn export format compatibility with `@theme inline` (docs cite both but I haven't hand-verified a tweakcn export running through Tailwind v4); shadcn's latest Form primitive may pull in `react-hook-form` + `zod` whose versions need pinning.
- **CSP dependency (resolved):** UNK-106 shipped 2026-04-24 (PR #50, `f070b97`) with a **hash-based** CSP rather than the nonce+templating shape this plan originally anticipated. Reconciliation: D3.13 now edits `frontend/src/fouc/fouc.js` instead of `index.html` directly; the Vite plugin at `frontend/vite-plugins/csp-hash.ts` handles injection and `dist/csp-hashes.json` emission; `backend/src/security/dist_validation.rs` gates server boot on FE/BE hash match. Vite dev topology is unchanged (Vite still serves `index.html` via the plugin); prod serves from `backend/src/routes/spa.rs`. A fresh adversarial review should still run before implementation starts — the 2026-04-23 review predates this reconciliation pass.
