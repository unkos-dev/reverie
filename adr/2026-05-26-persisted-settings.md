---
status: accepted
date: 2026-05-26
decision-makers: junkovich
consulted: []
informed: []
---

# Persist operator-tunable settings to database with live reload

## Context and Problem Statement

Reverie's configuration is entirely environment-based (`Config::from_env()` in `backend/src/config.rs`). Changing any operational parameter (enrichment concurrency, format priority, cover limits, writeback tuning) requires editing the environment and restarting the process. This is acceptable for infrastructure fields (port, database URL, OIDC) that change at deploy time, but creates unnecessary friction for runtime-tunable knobs that operators adjust during normal library management.

Step 11's Blueprint task 9 requires `GET/PUT /api/settings` endpoints so that the browser UI can display and mutate operator settings without a process restart. Sub-phase 11f implements this, gated on this ADR.

How should Reverie persist, propagate, and resolve operator-tunable settings?

Related: [JSON API conventions](2026-05-22-json-api-conventions.md) (error envelope for validation failures), [backend aux crates](2026-05-22-backend-aux-crates.md).

## Decision Drivers

- **Self-hosting audience**: operators manage Reverie via browser, not SSH. Settings should be UI-first.
- **Single-process today, multi-worker plausible**: the reload mechanism must not architecturally preclude horizontal scaling.
- **Strongly typed codebase**: Rust + sqlx compile-time checks; settings should be schema-enforced, not stringly-typed.
- **Minimal restart surface**: operators shouldn't restart for enrichment tuning, format reordering, or cover limits.
- **Industry alignment**: follow patterns proven in production at PostgREST, Hasura, Grafana, GitLab — not bespoke invention.

## Considered Options

### Storage shape

1. **Single-row typed table** — one column per setting, `singleton CHECK (id = true)` invariant
2. **Key-value table (jsonb)** — `(key text PK, value jsonb)`, flexible but untyped at DB level

### Precedence

1. **Env beats DB (12-factor)** — env is deploy override, DB is runtime knob
2. **DB beats env (UI-first)** — env provides initial seed; once operator sets via UI, DB value wins

### Reload mechanism

1. **RwLock + write-through** — PUT writes DB + updates in-process RwLock; stale in other processes
2. **RwLock + periodic poll** — poll DB every N seconds; bounded staleness
3. **LISTEN/NOTIFY + local RwLock cache** — zero-poll propagation to all connected processes; fallback periodic poll for connection-drop resilience

## Decision Outcome

### Storage: Single-row typed table

Chosen because: Reverie has a known, finite set of settings. Schema enforces types at the database level. One `SELECT *` loads everything into a `sqlx::FromRow` struct. Adding a new setting is a 3-line migration (`ALTER TABLE ADD COLUMN ... DEFAULT ...`) that runs automatically on startup. Matches the strongly-typed-everywhere philosophy.

Key-value rejected because: type validation would live entirely in app code; doesn't leverage sqlx compile-time checks; tempts schemaless drift; not idiomatic for this codebase.

### Precedence: DB beats env (UI-first)

Chosen because: Reverie's audience manages via browser UI. If an operator sets a value in the settings page, that value must take effect — having an invisible env var silently override it is surprising and frustrating UX. Env vars provide the initial seed (migration `DEFAULT` values come from env at first boot via a seed step), but once persisted, the DB value is authoritative.

Env-beats-DB rejected because: while 12-factor canonical, it optimises for the wrong audience. Kubernetes operators who pin via env can simply not expose those fields in the UI (the "restart-required" classification handles this naturally). Self-hosting operators using Docker Compose or bare-metal expect the UI to be authoritative.

**Seed behaviour** (deferred to [UNK-294](https://linear.app/unkos/issue/UNK-294)): on first startup (empty `settings` row), the migration inserts defaults. A one-time seed function in `services/settings.rs` will run post-migration and populate columns from current env values where the column is still at its migration default. This gives env vars "first boot" authority without ongoing override semantics. Not needed pre-0.1.0 — no real operators to break yet; migration defaults are reasonable starting values.

### Reload: LISTEN/NOTIFY + local RwLock cache

Chosen because: this is the PostgREST/Hasura pattern — proven at scale, zero per-request DB cost, instant propagation to all connected processes, and architecturally ready for multi-worker without code changes.

Shape:

1. Startup: `SELECT * FROM settings` → populate `Arc<RwLock<Settings>>` in `AppState`
2. Background task: `LISTEN settings_changed`; on notification → re-`SELECT` → update RwLock
3. PUT handler: write DB → `NOTIFY settings_changed` (same transaction)
4. Readers: `state.settings.read().await` — zero DB cost per request
5. Fallback: periodic poll every 60 seconds catches lost notifications (PG NOTIFY is not transactional-delivery; connection drop = lost)

RwLock-only rejected because: stale in multi-process deployments.
Periodic-poll-only rejected because: unnecessary staleness (up to poll interval) when LISTEN/NOTIFY is trivial to add.

### Field classification

All settings are hot-reloadable **except** four infrastructure fields that require process restart:

| Field                                                                          | Why restart-required                                                                                                 |
| ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| `port`                                                                         | Requires TcpListener rebind; axum::serve doesn't support hot swap                                                    |
| `database_url`                                                                 | Requires pool reconstruction + drain coordination                                                                    |
| `oidc_issuer_url`, `oidc_client_id`, `oidc_client_secret`, `oidc_redirect_uri` | Requires OIDC re-discovery (async HTTP) + client rebuild                                                             |
| `library_path`                                                                 | Worker reads from settings each cycle — could be hot, but path change mid-scan risks partial-state. Restart is safer |

All other fields (enrichment, cover, writeback, OPDS, format priority, cleanup mode, API base URLs, operator contact) are hot-reloadable. Workers and handlers read from the RwLock on each request/job cycle.

The PUT response includes a `restart_required: bool` flag when the request mutates any restart-required field. The frontend surfaces a "restart required" badge.

### Consequences

- Good, because operators can tune runtime knobs from the browser without SSH or restart
- Good, because multi-worker deployment works without code changes (LISTEN/NOTIFY propagates)
- Good, because type safety is preserved end-to-end (Rust struct ↔ typed PG columns ↔ TypeScript interface)
- Good, because 60s fallback poll guarantees eventual consistency even after PG connection blip
- Bad, because adding a new setting requires a migration (acceptable: migrations auto-run on startup)
- Bad, because env vars lose authority after first boot — operators must use UI or direct DB access to change values post-seed
- Neutral, because restart-required fields still need process restart — but this matches industry norms (Grafana, GitLab)

## Per-user settings (future path)

This ADR covers **system/admin settings** only. Per-user settings (reading preferences, display density, default sort, notification prefs) are a separate concern:

- Separate table: `user_settings` with `user_id` FK (or columns on `users` as `theme_preference` already is)
- Separate endpoint: `/auth/me/preferences` (self-service, not admin-gated)
- Same LISTEN/NOTIFY + RwLock pattern works — different cache shape (LRU keyed by `user_id` vs single struct)
- Out of 11f scope; architecturally compatible with all decisions in this ADR

## Implementation Plan

### Affected paths

- `backend/migrations/{ts}_settings.up.sql` — CREATE TABLE + trigger
- `backend/migrations/{ts}_settings.down.sql` — DROP
- `backend/src/models/settings.rs` — CREATE: `Settings` struct, `FromRow`, field classification marker
- `backend/src/services/settings.rs` — CREATE: load, save, reload listener, validate
- `backend/src/routes/settings/` — CREATE: `mod.rs` (router), `tests.rs` (integration tests)
- `backend/src/routes/mod.rs` — UPDATE: add `pub mod settings;`
- `backend/src/lib.rs` — UPDATE: `.merge(routes::settings::router())` + spawn listener task
- `backend/src/state.rs` — UPDATE: add `pub settings: Arc<RwLock<Settings>>` field

#### Deferred (follow-up PRs)

- `backend/src/config.rs` — seed helper (`fn from_config`) deferred to UNK-294
- `backend/src/services/settings.rs` — `seed` function deferred to UNK-294
- `frontend/src/api/settings.ts` — CREATE: `getSettings()`, `putSettings()`
- `frontend/src/pages/settings/SettingsPage.tsx` — CREATE: form with provenance badges
- `frontend/src/routes/settings.tsx` — CREATE: route + loader
- `frontend/src/main.tsx` — UPDATE: add settings route

### Dependencies

- No new crates required (`tokio`, `sqlx`, `serde` already present; `PgListener` is in `sqlx`)
- Frontend: no new deps (react-query already present from 11a)

### Patterns to follow

- Route module: `pub fn router() -> Router<AppState>` per `backend/src/routes/mod.rs` convention
- Auth gate: `current_user.require_admin()?` per `backend/src/auth/middleware.rs`
- RLS: `db::acquire_with_rls(&state.pool, current_user.user_id)` per `backend/src/db.rs:98-108`
- Error envelope: RFC 7807 Problem Details per [JSON API conventions ADR](2026-05-22-json-api-conventions.md)
- Frontend API: `apiFetch` wrapper from `src/api/fetch.ts`; react-query `useQuery`/`useMutation`

### Patterns to avoid

- Do NOT use env vars as runtime overrides post-boot — DB is authoritative
- Do NOT add `unsafe` or `unwrap` for the listener task — propagate errors, log failures, retry
- Do NOT cache settings in multiple places — single `Arc<RwLock<Settings>>` is the one source of truth for the running process
- Do NOT add LISTEN/NOTIFY to the ingestion pool — use the primary `reverie_app` pool; the listener doesn't need ingestion-role privileges

### Migration (illustrative)

The canonical schema is `backend/migrations/20260526015539_settings.up.sql`;
this snippet is a design-time sketch and may diverge in defaults or constraints.

```sql
-- up
CREATE TABLE settings (
  id bool PRIMARY KEY DEFAULT true CONSTRAINT singleton CHECK (id = true),
  -- Enrichment
  enrichment_enabled bool NOT NULL DEFAULT true,
  enrichment_concurrency int NOT NULL DEFAULT 3 CHECK (enrichment_concurrency BETWEEN 1 AND 10),
  enrichment_poll_idle_secs int NOT NULL DEFAULT 30,
  enrichment_fetch_budget_secs int NOT NULL DEFAULT 10,
  -- Cover
  cover_max_bytes bigint NOT NULL DEFAULT 5242880,
  cover_download_timeout_secs int NOT NULL DEFAULT 15,
  cover_min_long_edge_px int NOT NULL DEFAULT 300,
  cover_redirect_limit int NOT NULL DEFAULT 5,
  -- Writeback
  writeback_enabled bool NOT NULL DEFAULT true,
  writeback_concurrency int NOT NULL DEFAULT 2 CHECK (writeback_concurrency BETWEEN 1 AND 10),
  writeback_poll_idle_secs int NOT NULL DEFAULT 60,
  writeback_max_attempts int NOT NULL DEFAULT 3,
  -- OPDS
  opds_enabled bool NOT NULL DEFAULT true,
  opds_page_size int NOT NULL DEFAULT 50 CHECK (opds_page_size BETWEEN 1 AND 500),
  -- Ingestion
  format_priority text[] NOT NULL DEFAULT '{epub,pdf,mobi,azw3,cbz,cbr}',
  cleanup_mode text NOT NULL DEFAULT 'all' CHECK (cleanup_mode IN ('all', 'ingested', 'none')),
  -- API base URLs (enrichment sources)
  openlibrary_base_url text NOT NULL DEFAULT 'https://openlibrary.org',
  googlebooks_base_url text NOT NULL DEFAULT 'https://www.googleapis.com/books/v1',
  hardcover_base_url text NOT NULL DEFAULT 'https://api.hardcover.app/v1/graphql',
  -- Metadata
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- Singleton row
INSERT INTO settings DEFAULT VALUES;

-- Notify trigger
CREATE OR REPLACE FUNCTION notify_settings_changed() RETURNS trigger AS $$
BEGIN
  PERFORM pg_notify('settings_changed', '');
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER settings_changed_trigger
  AFTER UPDATE ON settings
  FOR EACH ROW EXECUTE FUNCTION notify_settings_changed();

-- Grant access
GRANT SELECT, UPDATE ON settings TO reverie_app;
GRANT SELECT ON settings TO reverie_readonly;
```

### Configuration

- No new env vars introduced by this change
- Existing env vars seed the settings row on first boot only
- `LISTEN` channel name: `settings_changed`

### Verification

- [ ] `cargo test` passes — all existing tests unaffected
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo sqlx prepare --workspace --check` reports no drift
- [ ] Admin `GET /api/settings` returns all fields with correct types
- [ ] Admin `PUT /api/settings` with valid partial body updates fields and triggers NOTIFY
- [ ] Non-admin `GET /api/settings` returns 403 (problem-type: `forbidden`)
- [ ] Non-admin `PUT /api/settings` returns 403
- [ ] PUT with invalid value (e.g. `enrichment_concurrency: -1`) returns 422 with field diagnostic
- [ ] PUT of restart-required field returns `restart_required: true` in response (deferred — no restart-required fields in table yet)
- [ ] After PUT, subsequent GET reflects new value (proves NOTIFY → RwLock update path works)
- [ ] LISTEN/NOTIFY propagation tested: direct `UPDATE settings SET ...` via psql triggers cache refresh
- [ ] 60s fallback poll tested: settings change detected even when NOTIFY lost (simulate by dropping listener connection)
- [ ] Frontend settings page renders with correct values and provenance badges (deferred — frontend not yet shipped)
- [ ] Frontend "restart required" badge appears when restart-required field differs from running value (deferred — frontend not yet shipped)
- [ ] Migration round-trips: `sqlx migrate revert` then `sqlx migrate run` clean
- [ ] No regression in existing test suites (auth, ingestion, enrichment, writeback, OPDS, covers)

## Pros and Cons of the Options

### Single-row typed table

- Good, because schema enforces types at DB level
- Good, because one `SELECT *` loads everything — trivial `FromRow`
- Good, because `NOT NULL DEFAULT` auto-populates new settings without backfill
- Good, because migrations are self-documenting
- Neutral, because table gets wide (20+ columns eventually) — but single-row tables are tiny regardless
- Bad, because adding a setting requires a migration (3-line `ALTER TABLE ADD COLUMN`)

### Key-value table (jsonb)

- Good, because no migration per new setting
- Good, because plugin systems can store arbitrary config
- Bad, because type validation lives entirely in app code
- Bad, because loading requires multi-row deserialization and merge
- Bad, because no schema documentation at DB level
- Bad, because doesn't match Reverie's strongly-typed philosophy

### Env beats DB (12-factor)

- Good, because aligns with Kubernetes ConfigMap pattern
- Good, because deploy-time pins can't be accidentally overridden via UI
- Bad, because UI-set values silently ignored when env var present — surprising for self-hosting audience
- Bad, because requires operators to understand env-var precedence model

### DB beats env (UI-first)

- Good, because what operator sets in UI is what takes effect
- Good, because simpler mental model for self-hosting audience
- Bad, because breaks 12-factor expectations for Kubernetes-native operators
- Neutral, because restart-required fields naturally handle the "deploy-time pin" use case without precedence complexity

### LISTEN/NOTIFY + local RwLock cache

- Good, because zero per-request DB cost (RwLock read is ~nanoseconds)
- Good, because instant propagation to all connected processes
- Good, because architecturally ready for multi-worker without code changes
- Good, because industry-proven (PostgREST, Hasura, GitLab)
- Neutral, because requires fallback poll for connection-drop edge case (~10 LOC)
- Bad, because slightly more wiring than pure write-through (~40 LOC more)

## More Information

**Revisit conditions:**

- If Reverie adopts a plugin/extension system that needs arbitrary settings, reconsider key-value as a companion table (not replacement)
- If multi-worker deployment becomes common and NOTIFY latency is measurably insufficient, consider Redis pub/sub as transport (unlikely for settings cadence)
- When per-user settings ship, create a separate `user_settings` ADR — this ADR's patterns apply but the cache shape differs (LRU vs single struct)

**Industry references:**

- PostgREST schema reload via NOTIFY: [postgrest.org/en/stable/references/admin.html](https://postgrest.org/en/stable/references/admin.html)
- Hasura metadata reload: uses PG NOTIFY + in-memory cache
- Grafana: most settings hot-reloadable, infrastructure fields require restart
- GitLab: application settings table (single-row) + Sidekiq NOTIFY propagation
