---
status: superseded
date: 2026-05-08
superseded-by: ["../2026-06-04-first-party-session-layer.md"]
decision-makers: john
---

# Adopt `tower-sessions-sqlx-store` for Postgres-backed sessions

## Context and Problem Statement

[UNK-163](https://linear.app/unkos/issue/UNK-163) replaces the
in-process `MemoryStore` with a Postgres-backed `SessionStore` so
that sessions survive container restarts. The MVP staging deploy
hits the LXC redeploy cycle frequently enough that forced re-login
on every redeploy is a measurable friction.

`tower-sessions` 0.14 (the version Reverie pins) does not ship a
Postgres backend on its own. The crate ecosystem splits the
backends into a sibling crate, `tower-sessions-sqlx-store`. Two
candidates were considered:

- **`tower-sessions-sqlx-store`** — first-party of the
  `maxcountryman/tower-sessions-stores` repo, same author as
  `tower-sessions` itself. Tracks `tower-sessions-core` versions
  closely. Apache-2.0/MIT.
- **`tower-sessions-rusqlite-store` / `tower-sessions-redis-store`**
  — same family, different backends. Not relevant: Reverie's
  primary store is already Postgres (per
  [`adr/2026-05-05-single-image-distribution-central-csp.md`](../2026-05-05-single-image-distribution-central-csp.md)
  and the wider stack), and adding Redis or sqlite as a session
  backend introduces a second persistence dependency for no win.

CLAUDE.md hard rule §1 (Plan Discipline) and the project's
dependency-governance posture require an ADR before a new direct
dependency lands in `[dependencies]`. PR #180 missed this — the
pinning rationale lived in the inline `Cargo.toml` comment and the
PR body, neither of which is a durable decision record. Greptile
flagged it on the PR (rule "No new direct dependencies without an
ADR"); this ADR is the resolution.

## Decision

Adopt `tower-sessions-sqlx-store = { version = "0.15.0", features
= ["postgres"] }` as the Postgres `SessionStore` backend, paired
with `tower-sessions = "0.14"`.

### Version pinning rationale

The 0.15.0 line of `tower-sessions-sqlx-store` is what pairs with
`tower-sessions` 0.14. Both depend on `tower-sessions-core` 0.14.
Despite the version-number divergence, the 0.14.x line of
`tower-sessions-sqlx-store` pins `tower-sessions-core` 0.13 and is
compile-incompatible with `tower-sessions` 0.14. The version
mismatch is documented inline at `backend/Cargo.toml:39-44`.

### Coordinated bump path

When `tower-sessions` advances to 0.15, `tower-sessions-sqlx-store`
must move in lockstep. Renovate Cargo PR #128 has been blocked on
exactly this since 2026-04-26 — `axum-login@0.18.0` peer-pins
`tower-sessions = "0.14"` and the upstream tracker
[`maxcountryman/axum-login#320`](https://github.com/maxcountryman/axum-login/issues/320)
is the unblock. [UNK-101](https://linear.app/unkos/issue/UNK-101)
tracks the eventual coordinated bump and references this ADR for
the framework-pair invariant.

### Schema and grants

A dedicated `tower_sessions` schema isolates the framework table
from application tables. The crate's own `migrate()` helper uses
the same convention; the migration replicates it manually so the
schema sits under the project's standard sqlx migration pipeline
rather than crate-bundled DDL.

- `CREATE TABLE tower_sessions.session (id, data bytea, expiry_date)`
  — schema and column names exactly match the
  `tower-sessions-sqlx-store@0.15.0` defaults so no
  `with_schema_name` / `with_table_name` override is needed at
  `PostgresStore::new` construction.
- No RLS on the session table. `SessionStore::load` runs before
  any auth context exists — the cookie's session id is the
  bootstrap that resolves the user — so RLS-gating the session
  lookup is chicken-and-egg. Access is enforced at the
  role-grant boundary instead.
- `reverie_app` gets full DML on `tower_sessions.session`.
- `reverie_readonly` gets _column-scoped_ `SELECT (id, expiry_date)`
  only. The `data bytea` column holds the MessagePack-encoded
  full session `Record` (axum-login user identity, OIDC nonce,
  any other session payload); granting blanket SELECT to the
  diagnostic role would let any principal on that connection
  enumerate live sessions and decode their payloads. Diagnostic
  intent is session counts, which `(id, expiry_date)` satisfies.
- `reverie_ingestion` gets nothing — no role grant.

### Index

`session_expiry_date_idx` on `expiry_date` supports the
`ExpiredDeletion` sweep (`DELETE … WHERE expiry_date < now()`).
The library does not ship its own index; without it the sweep is
a sequential scan.

### Test coverage

Two `#[sqlx::test(migrations = "./migrations")]` cases pinning
the contract:

- `session_record_survives_store_restart` — happy path. Saves a
  record through one `PostgresStore` instance, drops it, builds a
  fresh `PostgresStore` against the same DB pool, asserts the
  record loads with identical payload (including a CSRF-nonce
  shape).
- `expired_session_is_not_returned` — negative path. Inserts a
  record whose `expiry_date` is one second in the past and
  asserts `SessionStore::load` returns `Ok(None)`. This is the
  load-bearing seam for stale-cookie auth: if it broke, a user
  holding an expired cookie would still resolve to an
  authenticated identity.

## Consequences

- Good — sessions survive backend restarts. Eliminates the LXC
  redeploy → forced re-login friction that motivated the swap.
- Good — column-scoped grant on `reverie_readonly` lets the
  diagnostic role do session-count queries without exposing
  session payloads. Defence-in-depth against a compromised or
  misused readonly principal.
- Good — explicit `tower-sessions-core` version invariant is
  recorded here and inline at `backend/Cargo.toml`. Future
  agents and contributors do not have to re-discover the pairing
  rule.
- Bad — adds a new Cargo dependency to the runtime tree.
  `tower-sessions-sqlx-store` is a thin wrapper around the
  storage layer; the maintenance burden is small but non-zero.
- Bad — coupled version bumps. `tower-sessions` 0.14 → 0.15 is
  not a one-crate change; it's a four-crate change spanning
  `tower-sessions`, `tower-sessions-sqlx-store`,
  `tower-sessions-core`, and the downstream `axum-login`
  consumer. The coordination cost is real and tracked under
  UNK-101.
- Neutral — no production migration of existing session data.
  The MemoryStore was always restart-eviction by definition, so
  the swap is a strict improvement; no users had a "long-lived"
  session under the old store to migrate.

## Alternatives Considered

- **Stick with `MemoryStore`.** Rejected — does not solve the
  redeploy-eviction problem that motivated UNK-163.
- **Roll a hand-written sqlx-backed `SessionStore` impl.**
  Rejected — `SessionStore` is a 4-method trait, but the
  serialization, expiry sweep, and SQL upsert semantics are non-
  trivial. The `tower-sessions-sqlx-store` crate has a multi-
  release track record solving exactly this and is maintained by
  the same author as `tower-sessions` itself.
- **Use Redis as a session backend
  (`tower-sessions-redis-store`).** Rejected — adds a second
  persistence dependency to the deployment. Reverie targets
  single-image self-hosting (per the
  [single-image distribution ADR](../2026-05-05-single-image-distribution-central-csp.md));
  every additional service the operator has to run is friction
  against the deployment story.
- **Embed the schema migration in the crate's own `migrate()`
  helper rather than authoring it under
  `backend/migrations/`.** Rejected — Reverie's sqlx migration
  pipeline is the authoritative DDL source; mixing crate-
  bundled DDL with project-managed DDL fragments the migration
  history and complicates rollback semantics.
- **Defer the Postgres swap until `tower-sessions` 0.15 lands.**
  Rejected — the redeploy-eviction friction is concrete today,
  the 0.15 bump is blocked on
  [`axum-login#320`](https://github.com/maxcountryman/axum-login/issues/320)
  with no projected timeline. UNK-101 captures the upgrade path
  without blocking the current need.

## More Information

- [UNK-163](https://linear.app/unkos/issue/UNK-163) — the work
  this ADR records the dependency decision for
- [UNK-101](https://linear.app/unkos/issue/UNK-101) — coordinated
  `tower-sessions` 0.14 → 0.15 bump, blocked on
  `axum-login@0.18.0` peer-pin
- [`adr/2026-05-04-greptile-trial.md`](../2026-05-04-greptile-trial.md)
  — Greptile's "No new direct dependencies without an ADR" rule
  flagged the original PR #180 missing this ADR
- `backend/Cargo.toml:39-44` — inline pin rationale, cross-
  references this ADR
- `backend/migrations/20260526000000_initial_schema.up.sql`
  — the schema + grants this ADR ratifies (tower_sessions schema section)
- `tower-sessions` upstream:
  <https://github.com/maxcountryman/tower-sessions>
- `tower-sessions-sqlx-store` upstream:
  <https://github.com/maxcountryman/tower-sessions-stores>
