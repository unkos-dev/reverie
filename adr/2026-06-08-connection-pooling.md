---
status: "accepted"
date: 2026-06-08
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Connection pooling: in-process `sqlx::PgPool` as the sole pooling layer

## Context and Problem Statement

Every database access in Reverie's backend goes through an in-process
`sqlx::PgPool` held in `AppState`, with role-scoped pools per the
migration-role model (`reverie_app` for request handlers, `reverie_ingestion`
for the ingestion worker). This pooling shape arrived incrementally and was
never recorded as a decision.

Reverie's deployment contract is a single-instance Docker Compose service
([migration model ADR](2026-06-02-hybrid-migration-entrypoints-and-role.md)),
not a horizontally-scaled fleet. The open question: is the in-process pool
**the** connection-pooling layer, or does Reverie need a separate pooling tier
between the application and Postgres?

## Decision Drivers

- **Single-instance deployment.** The shipped topology is one app process. The
  problem a separate pooling tier solves, many app processes exhausting
  Postgres `max_connections`: does not exist by default.
- **Minimise component count and SPOF** for a self-hosted operator. Every
  bundled component is one more thing to run, monitor, and have fail.
- **Session-level Postgres features are load-bearing.** Reverie's persisted
  settings use `LISTEN`/`NOTIFY` over a `PgListener`
  ([persisted-settings ADR](2026-05-26-persisted-settings.md)); the migration
  path takes a session-level advisory lock. A transaction-multiplexing pooler
  tier breaks both.
- **Enable, don't own.** An operator who scales out can place their
  own pooler in front of Postgres; Reverie should not preclude that, but it
  should not own that infrastructure.

## Considered Options

- **A: In-process `sqlx::PgPool`(s) as the sole pooling layer.**
- **B: Add a separate connection-pooling tier between app and Postgres.**
- **C: No pool; open a connection per request.**

## Decision Outcome

Chosen option: **A**. The in-process `sqlx::PgPool` is the sole
connection-pooling layer. `sqlx` already bounds the connection count, recycles
idle connections, and applies acquire timeouts; for a single-instance
deployment a second pooling tier is redundant. It would add a component and a
SPOF to solve a fan-out problem the deployment contract does not have, and a
transaction-multiplexing tier would break the `LISTEN`/`NOTIFY`
settings-reload path and the session-level migration lock, both first-party
invariants.

This does not constrain an operator: per the
[scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md)'s
"enable, don't own", an operator who runs their own fleet may front Postgres
with a pooler externally. Reverie does not ship or depend on one.

### Consequences

- Good, because no new component and no new SPOF, which ratifies what already
  ships rather than adding code.
- Good, because `LISTEN`/`NOTIFY` settings reload, session-level advisory
  locks, and prepared-statement caching all keep working natively; an
  in-process pool runs each connection in session mode.
- Good, because the connection budget is bounded and predictable: one process,
  one pool per role, sized to the Docker host.
- Bad, because an operator who runs multiple app instances multiplies the
  connection budget (instances × pool size) against one Postgres; that topology
  is explicitly not the supported default and is the
  [scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md)'s
  concern, where pool sizing or an operator-owned pooler would be revisited.
- Neutral, because choosing not to _bundle_ a pooling tier does not forbid one,
  an operator may add one externally with no Reverie change.

### Confirmation

All database access goes through a shared `sqlx::PgPool` in `AppState`
(including the role-scoped ingestion pool); no handler or worker opens a raw
per-request `PgConnection`. No external pooling component appears in the Docker
Compose deployment.

## Pros and Cons of the Options

### A: in-process `sqlx::PgPool`, no separate pooling tier

- Good, because it ratifies what already ships; zero new surface.
- Good, because session-mode connections preserve `LISTEN`/`NOTIFY`, advisory
  locks, and prepared statements.
- Neutral, because pool sizing becomes the one knob to get right per host.
- Bad, because it offers no built-in answer for a multi-instance operator, deferred to the scale-stance ADR by design.

### B: separate connection-pooling tier

- Good, because it would let many app processes share a small Postgres
  connection budget, which is valuable _if_ Reverie were a horizontally-scaled fleet.
- Bad, because a transaction-multiplexing tier breaks `LISTEN`/`NOTIFY` and
  session-level locks Reverie relies on; a session-mode tier gives up most of
  the multiplexing benefit that would justify it.
- Bad, because it adds a bundled component and a SPOF to a single-instance
  deployment that gains nothing from it.

### C: connection per request

- Bad, because per-request connect/auth/TLS handshake latency and Postgres
  backend churn make this strictly worse than a pool at any load; no upside.

## More Information

- Pairs with [scale stance: stateless app, enable-don't-own HA](2026-06-08-scale-stance-stateless-enable-not-own.md),
  that ADR owns the multi-instance pool-sizing question this one defers.
- [Migration model ADR](2026-06-02-hybrid-migration-entrypoints-and-role.md):
  the single-instance contract and the `reverie_app` / `reverie_ingestion` /
  `reverie_readonly` role split the per-role pools follow.
- [Persisted-settings ADR](2026-05-26-persisted-settings.md): the
  `LISTEN`/`NOTIFY` reload path that a transaction-multiplexing tier would
  break.
- Revisit trigger: if a multi-instance / HA topology becomes a _supported_
  deployment (not just operator-enabled), revisit pool sizing and whether a
  pooling tier belongs in the shipped stack.
