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
any operational parameter (enrichment concurrency, format priority, cover limits, writeback tuning) requires editing the
environment and restarting the process. This is acceptable for infrastructure fields (port, database URL, OIDC) that
change at deploy time, but creates unnecessary friction for runtime-tunable knobs that operators adjust during normal
library management.

Reverie needs `GET`/`PUT /api/v1/settings` endpoints so that the browser UI can display and mutate operator settings
without a process restart. How should Reverie persist, propagate, and resolve operator-tunable settings?

Related: [JSON API conventions](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md) (error envelope for
validation failures), [backend auxiliary crates](./0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md).

## Decision drivers

- Self-hosting audience: operators manage Reverie via browser, not SSH, so settings should be UI-first.
- Single-process today, multi-worker plausible: the reload mechanism must not architecturally preclude horizontal
  scaling.
- Strongly typed codebase: Rust and sqlx compile-time checks mean settings should be schema-enforced, not
  stringly-typed.
- Minimal restart surface: operators should not need to restart for enrichment tuning, format reordering, or cover
  limits.
- Industry alignment: follow patterns proven in production at PostgREST, Hasura, Grafana, and GitLab rather than bespoke
  invention.

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

The settings use one typed database row because the set is finite and schema-checked. Database values take precedence
after the initial environment seed because operators edit settings in the UI. LISTEN/NOTIFY propagates changes to
process-local caches, with polling as a recovery fallback. Infrastructure settings that cannot safely change in a
running process require a restart.

### Consequences

- Positive: operators can tune runtime knobs from the browser without SSH or a restart.
- Positive: multi-worker deployment works without code changes, since LISTEN/NOTIFY propagates across processes.
- Positive: type safety is preserved end to end, from the Rust struct through typed PostgreSQL columns to the TypeScript
  interface.
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
- Neutral: restart-required fields naturally handle the deploy-time-pin use case without added precedence complexity.

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
- Per-user settings should get their own ADR when they ship; this record's patterns apply, but the cache shape differs
  (an LRU keyed by user rather than a single struct).

Industry references:

- PostgREST reloads schema via `NOTIFY`:
  [postgrest.org/en/stable/references/admin.html](https://postgrest.org/en/stable/references/admin.html)
- Hasura reloads metadata using PostgreSQL `NOTIFY` plus an in-memory cache.
- Grafana keeps most settings hot-reloadable, with infrastructure fields requiring a restart.
- GitLab uses a single-row application settings table with Sidekiq-driven `NOTIFY` propagation.
