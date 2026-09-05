---
type: ADR
profile-version: 1
id: "REV-ADR-0017"
title: "In-process sqlx PgPool as the sole pooling layer"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-08"
decision-makers:
  - "John Unkovich"
---

# In-process sqlx PgPool as the sole pooling layer

## Context and problem statement

Every database access in Reverie's backend goes through an in-process `sqlx::PgPool` held in `AppState`, with
role-scoped pools per the migration-role model (`reverie_app` for request handlers, `reverie_ingestion` for the
ingestion worker). This pooling shape arrived incrementally and was never recorded as a decision.

Reverie's deployment contract is a single-instance Docker Compose service (see the migration model ADR), not a
horizontally scaled fleet. The open question: is the in-process pool the connection-pooling layer, or does Reverie
need a separate pooling tier between the application and Postgres?

## Decision drivers

- Single-instance deployment: the shipped topology is one app process. The problem a separate pooling tier solves,
  many app processes exhausting Postgres `max_connections`, does not exist by default.
- Minimise component count and single points of failure for a self-hosted operator; every bundled component is one
  more thing to run, monitor, and have fail.
- Session-level Postgres features are load-bearing: Reverie's persisted settings use `LISTEN`/`NOTIFY` over a
  `PgListener` (see the persisted-settings ADR), and the migration path takes a session-level advisory lock. A
  transaction-multiplexing pooler tier breaks both.
- Enable, don't own: an operator who scales out can place their own pooler in front of Postgres; Reverie should not
  preclude that, but it should not own that infrastructure.

## Considered options

- In-process `sqlx::PgPool`, no separate pooling tier
- Separate connection-pooling tier between app and Postgres
- Connection per request (no pool)

## Decision outcome

Chosen option: **in-process `sqlx::PgPool`, no separate pooling tier**, because `sqlx` already bounds the connection
count, recycles idle connections, and applies acquire timeouts, so for a single-instance deployment a second pooling
tier is redundant. It would add a component and a single point of failure to solve a fan-out problem the deployment
contract does not have, and a transaction-multiplexing tier would break the `LISTEN`/`NOTIFY` settings-reload path and
the session-level migration lock, both first-party invariants. All database access goes through the shared
`sqlx::PgPool` in `AppState`, including the role-scoped ingestion pool; no handler or worker opens a raw per-request
`PgConnection`, and no external pooling component appears in the Docker Compose deployment.

This does not constrain an operator: per the scale-stance ADR's "enable, don't own", an operator who runs their own
fleet may front Postgres with a pooler externally. Reverie does not ship or depend on one.

### Consequences

- Positive: no new component and no new single point of failure, which ratifies what already ships rather than
  adding code.
- Positive: `LISTEN`/`NOTIFY` settings reload, session-level advisory locks, and prepared-statement caching all keep
  working natively, because an in-process pool runs each connection in session mode.
- Positive: the connection budget is bounded and predictable, one process, one pool per role, sized to the Docker
  host.
- Positive: choosing not to bundle a pooling tier does not forbid one; an operator may add one externally with no
  Reverie change.
- Negative: an operator who runs multiple app instances multiplies the connection budget (instances times pool size)
  against one Postgres; that topology is explicitly not the supported default and is the scale-stance ADR's concern,
  where pool sizing or an operator-owned pooler would be revisited.

## Pros and cons of the options

### In-process `sqlx::PgPool`, no separate pooling tier

- Positive: ratifies what already ships, zero new surface.
- Positive: session-mode connections preserve `LISTEN`/`NOTIFY`, advisory locks, and prepared statements.
- Neutral: pool sizing becomes the one knob to get right per host.
- Negative: offers no built-in answer for a multi-instance operator, deferred to the scale-stance ADR by design.

### Separate connection-pooling tier between app and Postgres

- Positive: would let many app processes share a small Postgres connection budget, valuable if Reverie were a
  horizontally scaled fleet.
- Negative: a transaction-multiplexing tier breaks `LISTEN`/`NOTIFY` and the session-level locks Reverie relies on; a
  session-mode tier gives up most of the multiplexing benefit that would justify it.
- Negative: adds a bundled component and a single point of failure to a single-instance deployment that gains
  nothing from it.

### Connection per request (no pool)

- Negative: per-request connect, authentication, and TLS handshake latency, plus Postgres backend churn, make this
  strictly worse than a pool at any load, with no upside.

## More information

- Pairs with the
  [scale-stance ADR](./0021-scale-stance-stateless-application-operator-enabled-ha.md), which owns the multi-instance
  pool-sizing question this one defers.
- [Migration model ADR](./0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md): the single-instance
  contract and the `reverie_app` / `reverie_ingestion` / `reverie_readonly` role split the per-role pools follow.
- [Persisted-settings ADR](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md): the
  `LISTEN`/`NOTIFY` reload path that a transaction-multiplexing tier would break.
- Revisit trigger: if a multi-instance or high-availability topology becomes a supported deployment, not just
  operator-enabled, revisit pool sizing and whether a pooling tier belongs in the shipped stack.
