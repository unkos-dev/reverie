---
type: ADR
profile-version: 1
id: "REV-ADR-0012"
title: "Persist operator-tunable settings to database with live reload"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-05-26"
decision-makers:
  - "John Unkovich"
---

# Persist operator-tunable settings to database with live reload

## Context and problem statement

Reverie's configuration is entirely environment-based (`Config::from_env()` in `backend/src/config/mod.rs`). Changing
any operational parameter (enrichment concurrency, format priority, cover limits, writeback tuning) requires editing
the environment and restarting the process. This is acceptable for infrastructure fields (port, database URL, OIDC)
that change at deploy time, but creates unnecessary friction for runtime-tunable knobs that operators adjust during
normal library management.

Reverie needs `GET`/`PUT /api/settings` endpoints so that the browser UI can display and mutate operator settings
without a process restart. How should Reverie persist, propagate, and resolve operator-tunable settings?

Related: [JSON API conventions](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md) (error envelope
for validation failures), [backend auxiliary crates](./0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md).

## Decision drivers

- Self-hosting audience: operators manage Reverie via browser, not SSH, so settings should be UI-first.
- Single-process today, multi-worker plausible: the reload mechanism must not architecturally preclude horizontal
  scaling.
- Strongly typed codebase: Rust and sqlx compile-time checks mean settings should be schema-enforced, not
  stringly-typed.
- Minimal restart surface: operators should not need to restart for enrichment tuning, format reordering, or cover
  limits.
- Industry alignment: follow patterns proven in production at PostgREST, Hasura, Grafana, and GitLab rather than
  bespoke invention.

## Considered options

Storage shape:

- Single-row typed table: one column per setting, with a `singleton CHECK (id = true)` invariant
- Key-value table (jsonb): `(key text PK, value jsonb)`, flexible but untyped at the database level

Precedence:

- Env beats DB (12-factor): env is the deploy override, DB is the runtime knob
- DB beats env (UI-first): env provides the initial seed; once an operator sets a value via the UI, the DB value wins

Reload mechanism:

- RwLock + write-through: `PUT` writes the DB and updates an in-process `RwLock`; stale in other processes
- RwLock + periodic poll: poll the DB every N seconds; bounded staleness
- LISTEN/NOTIFY + local RwLock cache: zero-poll propagation to all connected processes, with a fallback periodic poll
  for connection-drop resilience

## Decision outcome

Chosen option: **single-row typed table storage, database-beats-env precedence, and LISTEN/NOTIFY reload**, because
together they give a strongly-typed, UI-first settings surface without precluding multi-worker deployment.

### Storage: single-row typed table

Chosen option: **single-row typed table**, because Reverie has a known, finite set of settings. Schema enforces
types at the database level. One `SELECT *` loads everything into a `sqlx::FromRow` struct. Adding a new setting is a
small migration (`ALTER TABLE ADD COLUMN ... DEFAULT ...`) that runs automatically on startup, matching the
strongly-typed-everywhere philosophy.

Key-value table rejected because type validation would live entirely in application code, it does not leverage sqlx
compile-time checks, it tempts schemaless drift, and it is not idiomatic for this codebase.

### Precedence: DB beats env (UI-first)

Chosen option: **DB beats env (UI-first)**, because Reverie's audience manages the application via browser UI. If an
operator sets a value on the settings page, that value must take effect; an invisible env var silently overriding it
is surprising and frustrating. Env vars provide the initial seed (migration `DEFAULT` values come from env at first
boot via a seed step), but once persisted, the DB value is authoritative.

Env-beats-DB rejected because, while canonical under twelve-factor principles, it optimises for the wrong audience.
Kubernetes operators who pin settings via env can omit those fields from the UI (the restart-required classification
handles this naturally). Self-hosting operators using Docker Compose or bare metal expect the UI to be authoritative.

Seed behaviour: on first startup (empty `settings` row), the migration inserts defaults. A one-time seed function in
`backend/src/services/settings.rs` populates columns from current env values where the column is still at its
migration default. This gives env vars first-boot authority without ongoing override semantics.

### Reload: LISTEN/NOTIFY + local RwLock cache

Chosen option: **LISTEN/NOTIFY + local RwLock cache**, because this is the PostgREST/Hasura pattern, proven at scale,
with zero per-request database cost, instant propagation to all connected processes, and readiness for multi-worker
deployment without code changes.

Shape:

1. Startup: `SELECT * FROM settings` populates an `Arc<RwLock<Settings>>` in `AppState`.
2. A background task issues `LISTEN settings_changed`; on notification it re-`SELECT`s and updates the `RwLock`.
3. The `PUT` handler writes the DB and issues `NOTIFY settings_changed` in the same transaction.
4. Readers use `state.settings.read().await`, at zero database cost per request.
5. A fallback periodic poll every 60 seconds catches lost notifications, since PostgreSQL `NOTIFY` delivery is not
   transactional and a connection drop loses pending notifications.

RwLock-only rejected because it is stale in multi-process deployments. Periodic-poll-only rejected because it adds
unnecessary staleness, up to the poll interval, when LISTEN/NOTIFY is trivial to add alongside it.

### Field classification

All settings are hot-reloadable except four groups of infrastructure fields that require a process restart:

| Field                                                                          | Why restart-required                                                                                                                  |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| `port`                                                                         | Requires a `TcpListener` rebind; `axum::serve` does not support a hot swap.                                                           |
| `database_url`                                                                 | Requires pool reconstruction and drain coordination.                                                                                  |
| `oidc_issuer_url`, `oidc_client_id`, `oidc_client_secret`, `oidc_redirect_uri` | Requires OIDC re-discovery (async HTTP) and a client rebuild.                                                                         |
| `library_path`                                                                 | Workers read this from settings each cycle and could hot-reload it, but a path change mid-scan risks partial state; restart is safer. |

All other fields (enrichment, cover, writeback, OPDS, format priority, cleanup mode, API base URLs, operator contact)
are hot-reloadable. Workers and handlers read from the `RwLock` on each request or job cycle.

The `PUT` response includes a `restart_required: bool` flag when the request mutates any restart-required field, and
the frontend surfaces a restart-required badge.

This ADR covers system and admin settings only. Per-user settings (reading preferences, display density, default
sort, notification preferences) are a separate concern, architecturally compatible with the decisions above: a
separate `user_settings` table keyed by `user_id`, a separate self-service endpoint rather than an admin-gated one,
and the same LISTEN/NOTIFY and `RwLock` pattern with a per-user cache shape instead of a single struct.

### Consequences

- Positive: operators can tune runtime knobs from the browser without SSH or a restart.
- Positive: multi-worker deployment works without code changes, since LISTEN/NOTIFY propagates across processes.
- Positive: type safety is preserved end to end, from the Rust struct through typed PostgreSQL columns to the
  TypeScript interface.
- Positive: the 60-second fallback poll guarantees eventual consistency even after a PostgreSQL connection blip.
- Negative: adding a new setting requires a migration, though migrations auto-run on startup.
- Negative: env vars lose authority after first boot; operators must use the UI or direct database access to change
  values post-seed.
- Neutral: restart-required fields still need a process restart, which matches industry norms such as Grafana and
  GitLab.

## Pros and cons of the options

### Single-row typed table

- Positive: schema enforces types at the database level.
- Positive: one `SELECT *` loads everything, with a trivial `FromRow`.
- Positive: `NOT NULL DEFAULT` auto-populates new settings without a backfill.
- Positive: migrations are self-documenting.
- Neutral: the table gets wide (20+ columns eventually), but single-row tables are tiny regardless.
- Negative: adding a setting requires a migration.

### Key-value table (jsonb)

- Positive: no migration is needed per new setting.
- Positive: plugin systems can store arbitrary configuration.
- Negative: type validation lives entirely in application code.
- Negative: loading requires multi-row deserialization and merge.
- Negative: no schema documentation at the database level.
- Negative: does not match Reverie's strongly-typed philosophy.

### Env beats DB (12-factor)

- Positive: aligns with the Kubernetes ConfigMap pattern.
- Positive: deploy-time pins cannot be accidentally overridden via the UI.
- Negative: UI-set values are silently ignored when an env var is present, which is surprising for a self-hosting
  audience.
- Negative: requires operators to understand an env-var precedence model.

### DB beats env (UI-first)

- Positive: what the operator sets in the UI is what takes effect.
- Positive: simpler mental model for a self-hosting audience.
- Negative: breaks twelve-factor expectations for Kubernetes-native operators.
- Neutral: restart-required fields naturally handle the deploy-time-pin use case without added precedence
  complexity.

### LISTEN/NOTIFY + local RwLock cache

- Positive: zero per-request database cost, since an `RwLock` read is on the order of nanoseconds.
- Positive: instant propagation to all connected processes.
- Positive: architecturally ready for multi-worker deployment without code changes.
- Positive: industry-proven at PostgREST, Hasura, and GitLab.
- Neutral: requires a fallback poll for the connection-drop edge case.
- Negative: slightly more wiring than a pure write-through cache.

## More information

Revisit conditions:

- If Reverie adopts a plugin or extension system that needs arbitrary settings, reconsider a key-value table as a
  companion, not a replacement.
- If multi-worker deployment becomes common and NOTIFY latency proves measurably insufficient, consider a pub/sub
  transport such as Redis, though this is unlikely at settings cadence.
- Per-user settings should get their own ADR when they ship; this record's patterns apply, but the cache shape
  differs (an LRU keyed by user rather than a single struct).

Industry references:

- PostgREST reloads schema via `NOTIFY`: [postgrest.org/en/stable/references/admin.html](https://postgrest.org/en/stable/references/admin.html)
- Hasura reloads metadata using PostgreSQL `NOTIFY` plus an in-memory cache.
- Grafana keeps most settings hot-reloadable, with infrastructure fields requiring a restart.
- GitLab uses a single-row application settings table with Sidekiq-driven `NOTIFY` propagation.
