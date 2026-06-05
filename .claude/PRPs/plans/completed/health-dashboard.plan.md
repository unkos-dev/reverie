# Feature: Library Health Dashboard (Step 12 / UNK-81)

## Summary

Add an **admin-only** health dashboard to Reverie: two read-only Axum JSON endpoints
that aggregate library-wide operator metrics (total works/manifestations, storage
usage, validation-status breakdown, metadata coverage, enrichment-status breakdown,
recent ingestion-batch activity) plus a React dashboard page under `/admin/dashboard`
that renders them with existing shadcn/ui primitives. The dashboard is pure
aggregation over existing tables — **no schema changes, no new write paths**. It
mirrors the Step 11 library-API + admin-page patterns exactly and reuses the existing
`require_admin()` gate and `acquire_with_rls()` pool path.

## User Story

As a **Reverie administrator (self-hosting operator)**
I want to **see library-wide health metrics on one page** (how many books, how much
disk, how many files failed validation/enrichment, what recently got ingested)
So that **I can spot ingestion problems, storage growth, and metadata gaps without
running SQL by hand.**

## Problem Statement

Today there is no operator-facing view of library health. The only admin page is
`/admin/users`. To answer "how many books do I have, how much disk are they using,
did the last scan fail anything, how many files lack metadata?" an operator must query
Postgres directly. There is no endpoint that returns any aggregate — every existing
`manifestations` read path is paginated and per-row (`GET /api/books`), and
`ingestion_jobs`/`writeback_jobs` are not surfaced via HTTP at all.

This is testable: before, `GET /api/dashboard/stats` and `GET /api/dashboard/activity`
return 404 (routes don't exist). After, an admin gets 200 + a documented JSON shape;
a non-admin gets 403.

## Solution Statement

A new route module `backend/src/routes/dashboard/` exposes two admin-gated GET
endpoints. Both acquire a transaction via `db::acquire_with_rls(&state.pool,
current_user.user_id)` **after** `current_user.require_admin()?`. Because the caller is
an admin, the `manifestations_select_adult` RLS policy grants full-library visibility,
and every other table the dashboard reads (`works`, `ingestion_jobs`,
`writeback_jobs`) has **no RLS** and is `SELECT`-granted to `reverie_app` — so a single
pool + single transaction returns global aggregates with zero new infrastructure.

The frontend adds `src/api/dashboard.ts` (Zod-validated client), a `dashboard` query-key
family, a `DashboardPage.tsx` under `pages/admin/`, and a lazy route `admin/dashboard`
wired through `production.ts` + `main.tsx`, guarded with the existing `useAuthMe()` →
`<Navigate>` admin pattern. Charts are **out of scope** — metrics render with `Card`,
`Table`, `Badge`, `Progress`, and Lucide icons (no new dependency, no ADR needed).

## Metadata

| Field            | Value                                                                 |
| ---------------- | --------------------------------------------------------------------- |
| Type             | NEW_CAPABILITY                                                        |
| Complexity       | MEDIUM                                                                |
| Systems Affected | backend (routes), frontend (pages, api, routes, query-keys)          |
| Dependencies     | No new deps. axum 0.8.9, sqlx 0.9.0, @tanstack/react-query ^5.100.14, zod ^4.4.3, react-router ^7.16.0, lucide-react ^1.17.0 |
| Estimated Tasks  | 16 (incl. Task 0 nav pre-flight + Task 12b API-doc decision)          |
| Schema changes   | NONE (read-only aggregation over existing tables)                    |
| Branch           | `feature/unk-81-step-12-library-health-dashboard`                     |
| Closes           | UNK-81                                                                |

---

## UX Design

### Before State

```text
╔══════════════════════════════════════════════════════════════════════╗
║                            BEFORE STATE                                ║
╠══════════════════════════════════════════════════════════════════════╣
║                                                                        ║
║   ┌───────────┐      ┌───────────────┐      ┌────────────────────┐    ║
║   │  Admin    │ ───► │  /admin/users │ ───► │  User list table   │    ║
║   │  user     │      │  (only admin  │      │  (the ONLY admin   │    ║
║   └───────────┘      │   page)       │      │   surface)         │    ║
║                      └───────────────┘      └────────────────────┘    ║
║                                                                        ║
║   USER_FLOW: Admin can manage users. No library-health visibility.     ║
║   PAIN_POINT: "How many books / how much disk / did the scan fail?"    ║
║               answerable only via raw psql.                            ║
║   DATA_FLOW: No endpoint returns any aggregate over manifestations,    ║
║              ingestion_jobs, or works.                                 ║
║                                                                        ║
╚══════════════════════════════════════════════════════════════════════╝
```

### After State

```text
╔══════════════════════════════════════════════════════════════════════╗
║                            AFTER STATE                                 ║
╠══════════════════════════════════════════════════════════════════════╣
║                                                                        ║
║  ┌────────┐   ┌──────────────────┐   ┌──────────────────────────────┐ ║
║  │ Admin  │──►│ /admin/dashboard │──►│  GET /api/dashboard/stats     │ ║
║  │ user   │   │  (NEW page)      │   │  GET /api/dashboard/activity  │ ║
║  └────────┘   └──────────────────┘   └──────────────┬───────────────┘ ║
║                                                      │ require_admin() ║
║                                                      ▼ acquire_with_rls║
║                                       ┌──────────────────────────────┐ ║
║                                       │ Aggregate queries (1 tx):     │ ║
║                                       │  • COUNT manifestations/works │ ║
║                                       │  • SUM file_size_bytes        │ ║
║                                       │  • GROUP BY validation_status │ ║
║                                       │  • GROUP BY enrichment_status │ ║
║                                       │  • coverage FILTER counts     │ ║
║                                       │  • recent batches (jobs)      │ ║
║                                       └──────────────────────────────┘ ║
║                                                                        ║
║   Page layout (shadcn Card grid + Table, NO charts):                   ║
║   ┌──────────┬──────────┬──────────┬──────────┐                        ║
║   │ Books    │ Works    │ Storage  │ Failed   │  ← stat Cards          ║
║   │  1,204   │   980    │  3.4 GB  │   7      │                        ║
║   └──────────┴──────────┴──────────┴──────────┘                        ║
║   ┌─────────────────────┐  ┌──────────────────────┐                    ║
║   │ Validation breakdown│  │ Metadata coverage    │  ← Badge counts /  ║
║   │ clean 1190 ▓▓▓▓▓▓░  │  │ cover 92% ▓▓▓▓▓▓▓▓░  │    Progress bars   ║
║   │ repaired 7  degraded│  │ isbn  78% ▓▓▓▓▓▓░░░  │                    ║
║   └─────────────────────┘  └──────────────────────┘                    ║
║   ┌────────────────────────────────────────────┐                       ║
║   │ Recent ingestion batches (Table)           │  ← activity feed      ║
║   │ started  files  ok  failed  skipped  in-prog│                      ║
║   └────────────────────────────────────────────┘                       ║
║                                                                        ║
║   USER_FLOW: Admin opens /admin/dashboard → sees all metrics at a       ║
║              glance, loading skeletons → data, 403-redirect if not admin║
║   VALUE_ADD: Operator health visibility with zero psql.                 ║
║   DATA_FLOW: page → react-query → /api/dashboard/* → 1 admin tx → JSON. ║
║                                                                        ║
╚══════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location                       | Before              | After                                   | User Impact                          |
| ------------------------------ | ------------------- | --------------------------------------- | ------------------------------------ |
| `/admin/dashboard` (route)     | 404 (no route)      | Admin health dashboard page             | Operator sees library metrics        |
| `GET /api/dashboard/stats`     | 404                 | 200 JSON aggregate (admin) / 403 (other)| Programmatic health stats            |
| `GET /api/dashboard/activity`  | 404                 | 200 recent ingestion batches            | Recent scan outcomes visible         |
| Admin nav                      | "Users" only        | "Users" + "Dashboard" link              | Discoverable navigation              |

---

## Mandatory Reading

**Implementation agent MUST read these before starting any task.**

| Priority | File | Lines | Why Read This |
| -------- | ---- | ----- | ------------- |
| P0 | `backend/src/routes/settings/mod.rs` | 33-72 | Admin-gated GET handler + inline `Serialize` DTO pattern to MIRROR (calls `require_admin()`, returns `Json`) |
| P0 | `backend/src/routes/library/mod.rs` | 56-62, 130-204, 548-571 | `router()` shape, `acquire_with_rls` acquisition, `sqlx::query!` + `QueryBuilder` usage, enum `::text`/`"col!: Type"` casts |
| P0 | `backend/src/routes/ingestion.rs` | 22-41 | Smallest admin single-route module template (mod with `router()` + one handler) |
| P0 | `backend/src/error/mod.rs` | 56-137 | `AppError` variants + `IntoResponse` (RFC 7807). Use `AppError::Internal(e.into())` on sqlx errors |
| P0 | `backend/src/auth/middleware.rs` | 47-76, 188-247 | `CurrentUser` extractor + `require_admin()` → `AppError::Forbidden` |
| P0 | `backend/src/db.rs` | 108-118 | `acquire_with_rls(&pool, user_id)` — sets `app.current_user_id` GUC tx-locally |
| P1 | `backend/src/routes/library/tests.rs` | 76-79, 103-144 | `#[sqlx::test]` integration test pattern, `test_support::db` helpers, seeding via ingestion pool, admin/basic auth fixtures |
| P1 | `backend/src/routes/settings/tests.rs` | all | Admin-403 negative test shape (`require_admin` path) |
| P1 | `backend/src/models/validation_status.rs` | 49-63 | `ValidationStatus` enum (`Pending/Clean/Repaired/Degraded`) for `"col!: ValidationStatus"` cast |
| P1 | `backend/src/models/enrichment_status.rs` | 23-39 | `EnrichmentStatus` enum (snake_case rename) for cast |
| P0 | `frontend/src/pages/admin/UsersPage.tsx` | 36-120 | Admin page MIRROR: `useAuthMe()` gate, four render states, `<Navigate>` redirect, `useQuery` |
| P0 | `frontend/src/routes/admin.tsx` | 20-32 | Route `loader` + `Component` export + admin prefetch guard |
| P0 | `frontend/src/routes/production.ts` | 18-69 | Lazy route registration pattern (`adminUsersRoute`) |
| P0 | `frontend/src/api/users.ts` | 1-40 | Simple GET API module: `apiFetch` + `Schema.parse(raw)` at boundary |
| P0 | `frontend/src/api/books.ts` | 27-73, 184-198 | Zod enum + object schema definitions, `signal?: AbortSignal` param |
| P0 | `frontend/src/lib/query/keys.ts` | 27-66 | `queryKeys` factory — add a `dashboard` family here |
| P1 | `frontend/src/pages/library/LibraryPage.test.tsx` | 37-188 | Frontend test MIRROR: pre-seed `QueryClient` cache + `createMemoryRouter`, `screen.findByRole` |
| P1 | `frontend/src/components/ui/card.tsx` / `table.tsx` / `badge.tsx` | all | Primitives the page composes from |

**External Documentation:** None required. Feature uses only already-present libraries
and established in-repo patterns. (If a `progress` primitive is desired and not present,
it is added via `npx shadcn@latest add progress` — see Task 11 note.)

---

## Patterns to Mirror

**ADMIN_GATED_GET_HANDLER + INLINE DTO** (the core backend shape):

```rust
// SOURCE: backend/src/routes/settings/mod.rs:33-72
#[derive(serde::Serialize)]
struct SettingsResponse {
    #[serde(flatten)]
    settings: Settings,
    restart_required_fields: &'static [&'static str],
    #[serde(with = "time::serde::rfc3339::option")]
    last_successful_reload_at: Option<OffsetDateTime>,
}

async fn get_settings(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_admin()?;          // ← gate FIRST, before any DB work
    let settings = crate::services::settings::load(&state.pool).await?;
    Ok(axum::Json(SettingsResponse { /* ... */ }))
}
```

**ROUTER REGISTRATION** (one `pub fn router()` per module is the sole public API):

```rust
// SOURCE: backend/src/routes/ingestion.rs:22-41 (smallest admin module)
pub fn router() -> Router<AppState> {
    Router::new().route("/api/ingestion/scan", post(scan))
}
```

**RLS ACQUISITION** (required so the admin sees all manifestations rows):

```rust
// SOURCE: backend/src/routes/library/mod.rs (acquisition) + db.rs:108-118
let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
// ... run every aggregate query on &mut *tx ...
tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;
```

**AGGREGATE QUERY WITH ENUM CAST** (group-by with typed enum column):

```rust
// PATTERN derived from library/mod.rs:699-744 enum cast syntax.
// validation breakdown:
let rows = sqlx::query!(
    r#"SELECT validation_status AS "status!: ValidationStatus", COUNT(*) AS "count!"
       FROM manifestations
       GROUP BY validation_status"#,
)
.fetch_all(&mut *tx)
.await
.map_err(|e| AppError::Internal(e.into()))?;
```

**RESPONSE DTO CONVENTIONS** (response-only, in models when reused):

```rust
// SOURCE: backend/src/models/library.rs:47-80
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct BookListRow { /* pub fields, snake_case, Option<T> for nullable */ }
```
> For dashboard: DTOs are response-only and route-local, so define them as **private
> `#[derive(serde::Serialize)]` structs inside `routes/dashboard/mod.rs`** (mirror the
> settings `SettingsResponse` choice), NOT in `models/`. Only promote to `models/` if a
> second handler reuses them.

**BACKEND TEST** (admin-sees-all integration test):

```rust
// SOURCE: backend/src/routes/library/tests.rs:103-144
#[sqlx::test(migrations = "./migrations")]
async fn stats_endpoint_admin_gets_totals(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // seed manifestations via ingestion_pool (bypasses RLS)
    // ...
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server.get("/api/dashboard/stats").add_header(AUTHORIZATION, admin_auth).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total_manifestations"], 2);
}
```

**FRONTEND API MODULE** (Zod-at-boundary GET):

```typescript
// SOURCE: frontend/src/api/users.ts:29-32
async function listUsers(signal?: AbortSignal): Promise<User[]> {
  const raw = await apiFetch("/api/users", { signal });
  return UsersListSchema.parse(raw);
}
```

**FRONTEND ADMIN PAGE GUARD** (four states + redirect):

```typescript
// SOURCE: frontend/src/pages/admin/UsersPage.tsx:36-48
const { data: me, isLoading: meLoading, isError: meError } = useAuthMe();
const { data, isLoading, error } = useQuery({
  queryKey: queryKeys.dashboard.stats(),
  queryFn: ({ signal }) => getDashboardStats(signal),
  enabled: me?.role === "admin",
});
// meLoading → <Skeleton/>; meError → error <p>; me?.role !== "admin" → <Navigate to="/library"/>; success → render
```

**FRONTEND ROUTE MODULE**:

```typescript
// SOURCE: frontend/src/routes/admin.tsx:20-32
export async function loader(): Promise<null> {
  const me = queryClient.getQueryData<AuthMe | null>(queryKeys.auth.me());
  if (me?.role === "admin") {
    await queryClient.prefetchQuery({ queryKey: queryKeys.dashboard.stats(), queryFn: ({ signal }) => getDashboardStats(signal) });
  }
  return null;
}
export const Component = DashboardPage;
```

**FRONTEND TEST** (pre-seeded cache + memory router):

```typescript
// SOURCE: frontend/src/pages/library/LibraryPage.test.tsx:37-64
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
client.setQueryData(queryKeys.dashboard.stats(), statsFixture());
// wrap in QueryClientProvider + createMemoryRouter, assert with screen.findByRole
```

---

## Architecture & Key Decisions

**APPROACH_CHOSEN: Admin-only, global aggregates, single `state.pool` + `acquire_with_rls(admin_id)`, no chart library.**

**RATIONALE (pool):** Verified against `migrations/20260526000000_initial_schema.up.sql`:
- RLS is enabled on only `manifestations`, `reading_state`, `reading_sessions`,
  `webhooks`, `webhook_deliveries` (lines 788, 819, 823, 827, 831).
- `works` (940), `ingestion_jobs` (874), `writeback_jobs` (942), `work_authors` (934),
  `metadata_versions` (890) have **no RLS** and `reverie_app` holds `SELECT` — readable
  directly.
- `manifestations_select_adult` (line 800) grants `reverie_app` full SELECT when the
  GUC-resolved user has role `admin`/`adult`. After `require_admin()`, the caller IS
  admin, so `acquire_with_rls(&state.pool, admin_id)` makes `COUNT(*) FROM
  manifestations` return the **global** total. No `ingestion_pool` exposure, no new
  grant, no GUC dance needed. `acquire_with_rls` is **required** (not optional): without
  the `app.current_user_id` GUC set, the manifestations SELECT policies match no user
  and return zero rows.

**ALTERNATIVES_REJECTED:**
- *Use `state.ingestion_pool` for aggregates* (the explorer agent's sample): rejected —
  `ingestion_pool` is not wired to HTTP handlers today, and is unnecessary since the
  admin RLS path already yields global visibility. Adds surface area for no gain.
- *Add a new `reverie_readonly` HTTP pool + GUC bypass* (analyst's floated option):
  rejected — solves a problem that doesn't exist; `reverie_app` already reaches every
  needed table.
- *Per-user (non-admin) dashboard*: rejected — storage usage, ingestion-job internals,
  and validation/enrichment health are operator concerns, not per-reader data. Framing
  is "library health" = admin. (Children would also see near-empty aggregates under
  `manifestations_select_child`.)
- *Add a charting library (recharts/visx)*: rejected for first pass — a new runtime dep
  requires an ADR (CLAUDE.md), and the metrics read fine as Card stats + Badge counts +
  `Progress`/CSS bars + a Table. Charts can be a follow-up with its own ADR.
- *Fix UNK-313 here*: rejected — out of scope (see below); we **document** the caveat in
  the response shape instead.

**NOT_BUILDING (explicit scope limits):**
- **No schema migration.** Zero new tables/columns. Pure read aggregation.
- **No fix for UNK-313.** Non-EPUB files carry `validation_status='clean'` despite never
  being structurally validated (`ingestion/orchestrator.rs:386`). The stats endpoint
  therefore additionally returns `clean_non_epub_count` so the UI can footnote the
  `clean` bucket honestly ("N of these are non-EPUB formats not structurally
  validated"). The underlying bug stays tracked on UNK-313.
- **No charts / graphing library.**
- **No real-time updates / websockets / polling.** Plain react-query fetch on page load
  (default staleTime). Manual refresh via react-query is enough.
- **No CSV/PDF export.**
- **No date-range filtering** on activity — a simple `?limit=N` (default 20, cap 100).
- **No writeback/enrichment job history endpoints** beyond the enrichment-status
  *count* in stats. Recent activity = recent ingestion batches only.

**SECURITY (answer required by CLAUDE.md Hard Rule 6 — touches auth + response shape):**
Both endpoints gate with `require_admin()` *before* any DB access (403 for
non-admins, 401 for unauthenticated via the `CurrentUser` extractor). No user input is
interpolated into SQL — the only parameter is `limit` (bound, clamped to 1..=100). No
secrets, file I/O, XML, or outbound HTTP. Responses contain only aggregate counts/bytes
— no per-user data, no file paths, no PII. RFC 7807 error bodies reuse the existing
`AppError` `IntoResponse` (internal cause never leaked). **Will stand up to security
review:** yes — it is an admin-gated, parameterless-except-clamped-limit, read-only
aggregate over non-sensitive counts.

---

## Files to Change

| File | Action | Justification |
| ---- | ------ | ------------- |
| `backend/src/routes/dashboard/mod.rs` | CREATE | `router()` + 2 handlers (`stats`, `activity`) + private response DTOs |
| `backend/src/routes/dashboard/tests.rs` | CREATE | `#[sqlx::test]` integration tests (admin happy path + 403 + UNK-313 caveat) |
| `backend/src/routes/mod.rs` | UPDATE | Add `pub mod dashboard;` |
| `backend/src/lib.rs` | UPDATE | Merge `dashboard::router()` into `build_router_with_session_store` |
| `backend/.sqlx/*` | CREATE (generated) | `cargo sqlx prepare -- --tests` offline cache for new `query!` calls |
| `frontend/src/api/dashboard.ts` | CREATE | Zod schemas + `getDashboardStats` / `getDashboardActivity` |
| `frontend/src/lib/query/keys.ts` | UPDATE | Add `dashboard` query-key family |
| `frontend/src/pages/admin/DashboardPage.tsx` | CREATE | Admin-guarded dashboard page |
| `frontend/src/pages/admin/DashboardPage.test.tsx` | CREATE | Vitest page tests (states + admin redirect) |
| `frontend/src/routes/dashboard.tsx` | CREATE | `loader` + `Component` route module |
| `frontend/src/routes/production.ts` | UPDATE | Register `adminDashboardRoute` (lazy) |
| `frontend/src/main.tsx` | UPDATE | Push route into `children` array |
| `frontend/src/components/ui/progress.tsx` | CREATE (optional) | Only if a `Progress` bar is used and not already present (`npx shadcn@latest add progress`) |
| (admin nav host) | UPDATE (conditional) | Add admin-guarded "Dashboard" link — **only if Task 0 finds a nav host**; else descoped |
| (Starlight `docs/`) | UPDATE (conditional) | Endpoint docs — **only if Task 12b finds an existing per-endpoint API-doc convention**; else waived |

---

## API Contract (target response shapes)

`GET /api/dashboard/stats` → 200 (admin) / 403 / 401:

```jsonc
{
  "total_manifestations": 1204,        // COUNT(*) manifestations
  "total_works": 980,                  // COUNT(DISTINCT work_id)
  "storage_total_bytes": 3650000000,   // COALESCE(SUM(file_size_bytes),0)
  "storage_cover_bytes": 41000000,     // COALESCE(SUM(cover_size_bytes),0)
  "storage_by_format": [               // query B: GROUP BY format
    { "format": "epub", "count": 1100, "bytes": 3200000000 },
    { "format": "pdf",  "count": 104,  "bytes": 450000000 }
  ],
  "validation_breakdown": [            // GROUP BY validation_status
    { "status": "clean", "count": 1190 },
    { "status": "repaired", "count": 7 },
    { "status": "degraded", "count": 5 },
    { "status": "pending", "count": 2 }
  ],
  "clean_non_epub_count": 104,         // UNK-313 caveat qualifier
  "enrichment_breakdown": [            // GROUP BY enrichment_status
    { "status": "complete", "count": 1180 },
    { "status": "failed", "count": 9 }
  ],
  "metadata_coverage": {               // FILTER counts; frontend derives %
    "total": 1204,
    "has_description": 1100,
    "has_language": 1190,
    "has_isbn_13": 940,
    "has_cover": 1112
  }
}
```

`GET /api/dashboard/activity?limit=20` → 200 (admin):

```jsonc
{
  "batches": [                         // GROUP BY batch_id, recent first
    {
      "batch_id": "…uuid…",
      "started_at": "2026-06-05T10:00:00Z",
      "ended_at": "2026-06-05T10:02:11Z",   // nullable (null while batch in-flight)
      "total": 42, "completed": 40, "failed": 1, "skipped": 1, "in_progress": 0
      // invariant: completed + failed + skipped + in_progress == total
    }
  ]
}
```

**Reference SQL (compile-checked `query!`).** PERF (addresses adversarial D2): the
manifestation-side metrics are folded into **one** scan via conditional aggregation
(`COUNT(*) FILTER (WHERE …)`) rather than six separate `GROUP BY` scans. Only two scans
of `manifestations` remain: the combined aggregate (query A) and the per-format
breakdown (query B, genuinely a `GROUP BY format`). Validation/enrichment buckets are
reassembled into the breakdown arrays in Rust from the fixed FILTER columns (the enum
value sets are closed: 4 validation, 5 enrichment).

```sql
-- A. Combined manifestation+works aggregate (ONE scan, joined).
--    Coverage denominator is per-manifestation (total_manifestations), consistent
--    with a work that has multiple manifestations counting its metadata once per file.
SELECT
  COUNT(*)                  AS "total_manifestations!",
  COUNT(DISTINCT m.work_id) AS "total_works!",
  COALESCE(SUM(m.file_size_bytes), 0) AS "storage_total_bytes!",
  COALESCE(SUM(m.cover_size_bytes), 0) AS "storage_cover_bytes!",
  -- validation buckets (enum job set is closed)
  COUNT(*) FILTER (WHERE m.validation_status = 'pending')  AS "val_pending!",
  COUNT(*) FILTER (WHERE m.validation_status = 'clean')    AS "val_clean!",
  COUNT(*) FILTER (WHERE m.validation_status = 'repaired') AS "val_repaired!",
  COUNT(*) FILTER (WHERE m.validation_status = 'degraded') AS "val_degraded!",
  -- UNK-313 qualifier: clean rows that are non-EPUB (never structurally validated)
  COUNT(*) FILTER (WHERE m.validation_status = 'clean' AND m.format <> 'epub') AS "clean_non_epub!",
  -- enrichment buckets
  COUNT(*) FILTER (WHERE m.enrichment_status = 'pending')     AS "enr_pending!",
  COUNT(*) FILTER (WHERE m.enrichment_status = 'in_progress') AS "enr_in_progress!",
  COUNT(*) FILTER (WHERE m.enrichment_status = 'complete')    AS "enr_complete!",
  COUNT(*) FILTER (WHERE m.enrichment_status = 'failed')      AS "enr_failed!",
  COUNT(*) FILTER (WHERE m.enrichment_status = 'skipped')     AS "enr_skipped!",
  -- coverage
  COUNT(*) FILTER (WHERE w.description IS NOT NULL AND w.description <> '') AS "has_description!",
  COUNT(*) FILTER (WHERE w.language    IS NOT NULL AND w.language    <> '') AS "has_language!",
  COUNT(*) FILTER (WHERE m.isbn_13     IS NOT NULL) AS "has_isbn_13!",
  COUNT(*) FILTER (WHERE m.cover_path  IS NOT NULL) AS "has_cover!"
FROM manifestations m
JOIN works w ON w.id = m.work_id;

-- B. Storage + count per format (second scan; format is enum manifestation_format → ::text)
SELECT format::text AS "format!", COUNT(*) AS "count!",
       COALESCE(SUM(file_size_bytes), 0) AS "bytes!"
FROM manifestations GROUP BY format ORDER BY format;

-- C. Recent ingestion batches (activity). job_status set: queued|running|complete|failed|skipped.
--    in_progress = queued+running so completed+failed+skipped+in_progress == total
--    even for a batch mid-scan (addresses adversarial S1).
SELECT batch_id,
       MIN(created_at)   AS "started_at!",
       MAX(completed_at) AS ended_at,
       COUNT(*)                                                  AS "total!",
       COUNT(*) FILTER (WHERE status = 'complete')               AS "completed!",
       COUNT(*) FILTER (WHERE status = 'failed')                 AS "failed!",
       COUNT(*) FILTER (WHERE status = 'skipped')                AS "skipped!",
       COUNT(*) FILTER (WHERE status IN ('queued','running'))    AS "in_progress!"
FROM ingestion_jobs
GROUP BY batch_id
ORDER BY MIN(created_at) DESC
LIMIT $1;
```

> GOTCHA 1: query A returns exactly one row — use `fetch_one`. An empty library yields a
> single all-zeros row (not zero rows), because the aggregates have no `GROUP BY`. The
> `JOIN works` cannot drop manifestation rows (every manifestation has a non-null
> `work_id` FK), so the join does not under-count totals.
> GOTCHA 2: `ingestion_jobs.status` is enum `job_status`; the literal in `FILTER (WHERE
> status = 'complete')` coerces to the enum — fine. No `"col!: Type"` cast is needed in
> the folded design because validation/enrichment are returned as scalar `i64` counts,
> not as enum columns. (The `ValidationStatus`/`EnrichmentStatus` Rust enums are still
> used to *label* the reassembled breakdown arrays in the DTO.)

---

## Step-by-Step Tasks (TDD — tests first per CLAUDE.md Hard Rule 5)

Execute in order. Each task is atomic and independently verifiable. Backend validation
runs from `backend/`; frontend from `frontend/`.

### Task 0: PRE-FLIGHT — determine current admin navigation (decides Task 12)
- **ACTION**: Find how `/admin/users` is reached in the running UI before assuming a nav exists.
- **IMPLEMENT**: `rg -n "admin/users" frontend/src` and inspect any rendered `<Link
  to="/admin/users">`/`<NavLink>`. Three outcomes:
  - **Shared admin nav component found** → Task 12 adds a sibling `Dashboard` link there.
  - **Link lives in a generic header/sidebar** → Task 12 adds the link beside it, guarded
    by `me?.role === "admin"` (mirror the existing guard).
  - **No nav link exists — `/admin/users` is URL-only today** → Task 12 is **descoped**;
    record in the PR that `/admin/dashboard` is URL-only for this MVP (matches the
    existing admin UX) and remove the "Dashboard link" claim from the After-State UX. Do
    NOT invent a nav shell (scope guard).
- **VALIDATE**: outcome recorded; Task 12 branch selected. No code change in this task.

### Task 1: CREATE `backend/src/routes/dashboard/tests.rs` (failing tests first)
- **ACTION**: Write integration tests BEFORE handlers exist (they fail to compile/route → red).
- **IMPLEMENT**:
  - `stats_distinct_works_vs_manifestations` — seed **2 manifestations (1 epub clean, 1
    pdf) sharing ONE `work_id`** via `ingestion_pool`; assert 200,
    `total_manifestations==2`, **`total_works==1`** (exact — proves the `COUNT(DISTINCT
    work_id)` logic), and `clean_non_epub_count==1` (the pdf). This is the primary
    aggregation-correctness test.
  - `stats_empty_library_returns_zeros` — no seed; assert 200, all counts `0`,
    `storage_total_bytes==0` (query A `fetch_one` returns a single all-zeros row).
  - `stats_endpoint_rejects_non_admin` — basic (non-admin/adult) auth → assert 403 via
    `test_support::assert_problem(&resp, problems::FORBIDDEN, StatusCode::FORBIDDEN)`.
  - `stats_endpoint_requires_auth` — no auth → 401.
  - `activity_endpoint_admin_lists_batches` — seed `ingestion_jobs` for one batch;
    assert 200 and `batches[0].total` matches.
  - `activity_in_progress_sums_to_total` — seed a batch with one `running` (or `queued`)
    job plus terminal jobs; assert `completed + failed + skipped + in_progress == total`
    and `ended_at` is null (in-flight invariant, addresses S1).
  - `activity_limit_is_clamped` — `?limit=99999` and `?limit=0` → both 200 (clamped to
    1..=100, no panic, no Postgres negative-LIMIT error).
- **MIRROR**: `backend/src/routes/library/tests.rs:76-79,103-144`, `routes/settings/tests.rs` (403 path).
- **IMPORTS**: `use axum::http::{StatusCode, header::AUTHORIZATION}; use sqlx::PgPool; use crate::error::problems; use crate::test_support;`
- **VALIDATE**: `cargo test -p reverie dashboard 2>&1 | head` — expect compile error / route 404 (RED is correct here).

### Task 2: CREATE `backend/src/routes/dashboard/mod.rs`
- **ACTION**: Implement `router()` + `stats` + `activity` handlers + private DTO structs.
- **IMPLEMENT**: `pub fn router() -> Router<AppState>` with
  `.route("/api/dashboard/stats", get(stats)).route("/api/dashboard/activity", get(activity))`.
  - `stats`: `require_admin()?` → `acquire_with_rls(&state.pool, user_id)` → query **A**
    (`fetch_one`, the combined conditional-aggregate join) + query **B** (`fetch_all`,
    per-format) → reassemble the `validation_breakdown` / `enrichment_breakdown` arrays
    in Rust from the fixed `val_*` / `enr_*` scalar columns (label each bucket with the
    `ValidationStatus` / `EnrichmentStatus` enum variant) → `tx.commit()` → `Json`.
  - `activity`: `require_admin()?` → query **C** (`fetch_all`). `acquire_with_rls` is
    used for consistency but is not load-bearing here — `ingestion_jobs` has **no RLS**
    (verified: migration line 874 grants `reverie_app` SELECT, no `ENABLE ROW LEVEL
    SECURITY` on that table). Takes `Query<ActivityParams>` with `limit: Option<i64>`
    clamped to `1..=100` (default 20) **before** binding to `$1`.
- **MIRROR**: `routes/settings/mod.rs:33-72` (gate+DTO), `routes/library/mod.rs:130-204` (acquire+query), `routes/ingestion.rs:22-41` (module shape).
- **IMPORTS**: `use axum::{Router, routing::get, extract::{State, Query}, response::IntoResponse, Json}; use crate::{state::AppState, error::AppError, auth::CurrentUser, db}; use crate::models::{validation_status::ValidationStatus, enrichment_status::EnrichmentStatus};`
- **GOTCHA**: gate BEFORE acquiring tx; cast enum columns with `"col!: Type"`; `format::text`; `COALESCE(SUM(...),0)` so empty library returns 0 not NULL.
- **VALIDATE**: `cargo build -p reverie` (will fail sqlx offline check until Task 4 — that's expected; compile of Rust itself should pass once queries are written with `query!`).

### Task 3: UPDATE `backend/src/routes/mod.rs` + `backend/src/lib.rs`
- **ACTION**: Register the module and merge its router.
- **IMPLEMENT**: add `pub mod dashboard;` in `routes/mod.rs` (alongside `pub mod settings;`);
  in `lib.rs` `build_router_with_session_store`, `.merge(routes::dashboard::router())`
  next to the other `.merge(routes::*::router())` calls.
- **MIRROR**: existing `routes::settings::router()` / `routes::users::router()` merges.
- **VALIDATE**: `cargo build -p reverie` (compiles; sqlx offline still pending).

### Task 4: GENERATE `.sqlx` offline cache
- **ACTION**: Regenerate the compile-time query cache for the new `query!` calls.
- **IMPLEMENT**: from `backend/`: `cargo sqlx prepare -- --tests` (requires a running dev
  DB — see project DB wiring; the migrations live in `./migrations`).
- **GOTCHA**: CI runs `cargo sqlx prepare --check -- --tests` — missing/stale `.sqlx`
  fails CI. Commit the generated `.sqlx/*.json` files.
- **VALIDATE**: `cargo sqlx prepare --check -- --tests` → exit 0.

### Task 5: RUN backend tests green
- **ACTION**: Make Task 1 tests pass.
- **VALIDATE**: `cargo test -p reverie dashboard` → all pass. Then `cargo fmt --check && cargo clippy -- -D warnings`.

### Task 6: CREATE `frontend/src/api/dashboard.ts` (with Zod schemas)
- **ACTION**: Define Zod schemas mirroring the API contract + two client fns.
- **IMPLEMENT**: `DashboardStatsSchema`, `DashboardActivitySchema` (+ inferred types);
  `getDashboardStats(signal?)` → `apiFetch("/api/dashboard/stats", {signal})` →
  `DashboardStatsSchema.parse(raw)`; `getDashboardActivity(limit?, signal?)` →
  `buildUrl("/api/dashboard/activity",{limit})`. Reuse existing
  `ValidationStatusSchema`/enrichment enum schema from `api/books.ts` if exported, else
  define `z.enum([...])` locally.
- **MIRROR**: `frontend/src/api/users.ts:29-32`, `api/books.ts:27-73,184-198`.
- **IMPORTS**: `import { z } from "zod"; import { apiFetch } from "./fetch"; import { buildUrl } from "./url";` (confirm `buildUrl` location).
- **VALIDATE**: `npm run type-check` (or `npx tsc --noEmit`).

### Task 7: UPDATE `frontend/src/lib/query/keys.ts`
- **ACTION**: Add a `dashboard` family to `queryKeys`.
- **IMPLEMENT**: `dashboard: { all: ["dashboard"] as const, stats: () => ["dashboard","stats"] as const, activity: (limit: number) => ["dashboard","activity",limit] as const }`.
- **MIRROR**: existing `books`/`users` families at `keys.ts:27-66`.
- **VALIDATE**: `npm run type-check`.

### Task 8: CREATE `frontend/src/pages/admin/DashboardPage.test.tsx` (failing first)
- **ACTION**: Write page tests before the component.
- **IMPLEMENT**: pre-seed `queryKeys.dashboard.stats()` + `queryKeys.auth.me()` (admin) in
  a `QueryClient`; render in `createMemoryRouter`; assert heading renders + a known stat
  value present; a second test seeds `me.role==="adult"` and asserts redirect (no
  dashboard heading).
- **MIRROR**: `frontend/src/pages/library/LibraryPage.test.tsx:37-188`.
- **VALIDATE**: `npm test -- DashboardPage` → RED.

### Task 9: CREATE `frontend/src/pages/admin/DashboardPage.tsx`
- **ACTION**: Build the admin-guarded page.
- **IMPLEMENT**: `useAuthMe()` gate (loading→Skeleton, error→`<p>`, non-admin→`<Navigate to="/library"/>`),
  `useQuery` for stats (+ activity), then a `Card` grid for top-line stats, `Badge`-count
  rows or `Progress` bars for validation/enrichment/coverage, a `Table` for recent
  batches. Format bytes with a small `formatBytes` helper. Footnote the `clean` bucket
  using `clean_non_epub_count` (UNK-313 honesty).
- **MIRROR**: `frontend/src/pages/admin/UsersPage.tsx:36-120`.
- **IMPORTS**: `Card*`, `Table*`, `Badge` from `@/components/ui/*`; `useAuthMe`; `useQuery`; `queryKeys`; `getDashboardStats`/`getDashboardActivity`; Lucide icons (`BookOpen`, `HardDrive`, `AlertTriangle`, `Activity`).
- **GOTCHA**: derive coverage % in the component (`has_x/total`), guard divide-by-zero on empty library.
- **VALIDATE**: `npm test -- DashboardPage` → GREEN; `npm run type-check`.

### Task 10: CREATE `frontend/src/routes/dashboard.tsx` + wire routing
- **ACTION**: Route module + registration.
- **IMPLEMENT**: `loader` (admin-prefetch stats, mirror `admin.tsx:20-32`) + `export const
  Component = DashboardPage;`. In `production.ts` add `adminDashboardRoute` (`path:
  "admin/dashboard"`, lazy import). In `main.tsx` push into the `children` array.
- **MIRROR**: `routes/admin.tsx:20-32`, `routes/production.ts:18-69`, `main.tsx:42-55`.
- **VALIDATE**: `npm run type-check && npm run build`.

### Task 11: (OPTIONAL) ADD `Progress` primitive
- **ACTION**: Only if Task 9 uses `<Progress>` and `components/ui/progress.tsx` is absent.
- **IMPLEMENT**: `npx shadcn@latest add progress`; verify it adopts repo tokens (no raw
  `bg-black`/hardcoded colors — see project memory on overlay tokens). If adding deps is
  undesired, render coverage with a plain tokenized `<div>` bar instead.
- **VALIDATE**: `npm run lint && npm run type-check`.

### Task 12: ADD admin nav link (per Task 0 outcome)
- **ACTION**: Execute the branch selected in Task 0.
- **IMPLEMENT**: if a nav/link host was found, add the `me?.role === "admin"`-guarded
  `<Link to="/admin/dashboard">Dashboard</Link>` sibling to the `/admin/users` link. If
  Task 0 found URL-only access, **skip this task** and update the After-State UX +
  acceptance criteria to drop the nav-link claim.
- **GOTCHA**: do not create a nav shell that does not already exist.
- **VALIDATE**: `npm run build`; if a link was added, manual nav check in Task 14.

### Task 12b: API documentation decision (addresses adversarial C1)
- **ACTION**: Decide whether the two new endpoints need doc, matching existing convention.
- **IMPLEMENT**: `rg -l "/api/books|/api/opds" docs/` to check whether existing endpoints
  (library/OPDS) are documented in the Starlight `docs/` site.
  - **If endpoints are documented there** → add a short section for
    `GET /api/dashboard/stats` and `/activity` (admin-only, response shape) mirroring the
    existing endpoint docs.
  - **If no per-endpoint API docs exist** → explicitly waive in the PR body ("no API-doc
    convention exists for HTTP endpoints; none added"). Do not start a new doc pattern.
- **VALIDATE**: doc added and `docs` build passes, OR waiver recorded in PR body.

### Task 13: FULL backend + frontend suites
- **VALIDATE**:
  - `cd backend && cargo fmt --check && cargo clippy -- -D warnings && cargo test -p reverie && cargo sqlx prepare --check -- --tests`
  - `cd frontend && npm run lint && npm run type-check && npm test && npm run build`

### Task 14: BROWSER validation (CLAUDE.md UI rule)
- **ACTION**: Probe `localhost:5173` (supervised, always up) and screenshot
  `/admin/dashboard` with `agent-browser`.
- **VALIDATE**: page renders, stat cards populated, table shows batches, non-admin
  redirect works, no console errors.

---

## Testing Strategy

### Tests to Write

| Test File | Test Cases | Validates |
| --------- | ---------- | --------- |
| `backend/src/routes/dashboard/tests.rs` | **distinct-works (2 manifs / 1 work → works==1, manifs==2)**, empty-library zeros, 403 non-admin, 401 no-auth, activity batches, **in-flight sum invariant**, limit clamp (0 + huge), `clean_non_epub_count` (UNK-313) | Handlers, auth gate, aggregation correctness |
| `frontend/src/pages/admin/DashboardPage.test.tsx` | renders stats, renders activity table, non-admin redirect, loading skeleton, coverage % derivation, empty-library (0 books) | Page states + admin guard |
| `frontend/src/api/dashboard.ts` (covered via page tests / schema parse) | valid payload parses, malformed payload rejected | Zod boundary |

### Edge Cases Checklist
- [ ] Empty library → all counts 0, `SUM` returns 0 not NULL (`COALESCE`), coverage % no divide-by-zero
- [ ] Non-admin (adult/child) → 403 backend, redirect frontend
- [ ] Unauthenticated → 401
- [ ] `?limit` absent (default 20), `?limit=0` / negative / huge → clamped to 1..=100
- [ ] Validation `clean` bucket includes non-EPUB → `clean_non_epub_count` surfaced + footnoted
- [ ] Batch with NULL `completed_at` (in-flight) → `ended_at: null` serializes cleanly
- [ ] In-flight batch (queued/running jobs) → `completed+failed+skipped+in_progress == total`
- [ ] Work with multiple manifestations → `total_works` distinct < `total_manifestations`

---

## Validation Commands

### Level 1: STATIC_ANALYSIS
```bash
cd backend && cargo fmt --check && cargo clippy -- -D warnings
cd frontend && npm run lint && npm run type-check
```
**EXPECT**: Exit 0.

### Level 2: UNIT/INTEGRATION TESTS
```bash
cd backend && cargo test -p reverie dashboard
cd frontend && npm test -- DashboardPage
```
**EXPECT**: All pass.

### Level 3: FULL SUITE + BUILD
```bash
cd backend && cargo test -p reverie && cargo sqlx prepare --check -- --tests
cd frontend && npm test && npm run build
```
**EXPECT**: All pass, build succeeds, `.sqlx` cache current.

### Level 4: DATABASE_VALIDATION
- [ ] No migration added (confirm `git status` shows no new file under `backend/migrations/`).
- [ ] `.sqlx/*.json` regenerated and committed for each new `query!`.

### Level 5: BROWSER_VALIDATION
- [ ] `/admin/dashboard` renders (probe `localhost:5173`, `agent-browser` screenshot).
- [ ] Non-admin redirected to `/library`.
- [ ] No console errors.

### Level 6: MANUAL_VALIDATION
1. Log in as admin → open `/admin/dashboard` → verify totals match `SELECT COUNT(*) FROM manifestations`.
2. Run an ingestion scan → refresh → new batch appears at top of activity table.
3. Log in as adult user → `/admin/dashboard` → redirected; `curl /api/dashboard/stats` with that session → 403.

---

## Acceptance Criteria
- [ ] `GET /api/dashboard/stats` + `/activity` return documented JSON for admins, 403 for non-admins, 401 unauth.
- [ ] React `/admin/dashboard` page renders all six metric groups using existing primitives (no chart lib).
- [ ] Level 1-3 validation pass with exit 0; `.sqlx` cache current.
- [ ] Unit/integration tests cover happy path + 403 + edge cases (TDD, written first).
- [ ] Code mirrors Step 11 patterns exactly (router shape, `acquire_with_rls`, Zod boundary, admin guard).
- [ ] No schema change; UNK-313 caveat surfaced via `clean_non_epub_count` + UI footnote.
- [ ] PR body contains `Closes UNK-81`.

## Completion Checklist
- [ ] All tasks done in order, each validated immediately.
- [ ] Backend: fmt + clippy + test + sqlx-prepare-check green.
- [ ] Frontend: lint + type-check + test + build green.
- [ ] Browser validation passed.
- [ ] Security answer recorded in PR summary (admin-gated, read-only, clamped param).
- [ ] Implementation done on `feature/unk-81-step-12-library-health-dashboard`, NOT the current badge branch.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
| ---- | ---------- | ------ | ---------- |
| Aggregate query returns 0 rows because `acquire_with_rls` GUC not set (RLS hides manifestations) | MED | HIGH | `acquire_with_rls` is mandatory and tested by `stats_endpoint_admin_gets_totals` asserting non-zero totals on seeded data |
| `.sqlx` cache forgotten → CI red | MED | MED | Task 4 explicit + Level 3 `prepare --check`; project memory "cargo fmt/sqlx before commit" |
| Validation `clean` bucket misleads (UNK-313) | HIGH | MED | `clean_non_epub_count` field + UI footnote; documented as NOT-fixing-here |
| Enum cast syntax (`"col!: Type"`) import path wrong | LOW | MED | P1 reading of `models/validation_status.rs` + `enrichment_status.rs`; mirrors `library/mod.rs:699-744` |
| No shared admin nav component exists | LOW | LOW | Task 12 gotcha: surface to user, don't invent a nav shell |
| Plan/impl accidentally lands on current badge branch | MED | MED | Completion checklist + handoff note: branch `feature/unk-81-...` first |

## Notes
- **Branch hygiene:** session is currently on `feature/unk-345-...fixui-default-badge...`
  with unrelated modified files. Implementation must start from a fresh
  `feature/unk-81-step-12-library-health-dashboard` off `main`. Do not commit this plan
  or implementation onto the badge branch.
- **Pool decision is verified, not assumed:** grant/RLS matrix read directly from
  `migrations/20260526000000_initial_schema.up.sql` (GRANTs 843-948, RLS-enable 788-831,
  policies 790-821). `reverie_app` + admin RLS context = global visibility on every
  dashboard table.
- **Linear:** UNK-81 is on the `v0.1.0` milestone, blocked-by UNK-80 (done), related to
  UNK-313 (the validation caveat). PR must `Closes UNK-81`.
- **Perf (adversarial D2 resolved):** manifestation metrics are computed in a single
  conditional-aggregation scan (query A) + one per-format `GROUP BY` (query B), not six
  separate scans. Two `manifestations` scans per stats load regardless of library size;
  no caching needed for an admin-only MVP endpoint.
- **In-flight batches (adversarial S1 resolved):** activity exposes `in_progress`
  (`queued+running`) so the four outcome columns always reconcile to `total`.
- **Discoverability (adversarial D1 resolved):** nav link is gated on Task 0's finding —
  the plan no longer assumes an admin nav exists.
- **Follow-ups (not this PR):** charting library (needs ADR); fixing UNK-313; writeback
  job history; date-range activity filter.
