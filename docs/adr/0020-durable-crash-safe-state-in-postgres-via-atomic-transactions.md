---
type: ADR
profile-version: 1
id: "REV-ADR-0020"
title: "Durable, crash-safe state in Postgres via atomic transactions"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-08"
decision-makers:
  - "John Unkovich"
---

# Durable, crash-safe state in Postgres via atomic transactions

## Context and problem statement

Reverie is self-hosted on hardware the operator controls, including consumer hardware with no UPS, where an instant
power-cut is a realistic event, not a theoretical one. The system must come back up consistent after any abrupt kill,
with no half-applied changes and no bespoke recovery dance.

Durable state already lives in Postgres in the places that matter: operator settings
([persist operator-tunable settings ADR](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md))
and sessions ([first-party session layer ADR](./0015-first-party-session-layer-on-the-tower-sessions-core.md)), but
this is convention, not a recorded decision, so nothing stops new code from parking critical state in process memory
or a local file where a crash would lose or corrupt it. Equally, a state change that spans several rows or tables can
leave an invariant half-applied if it is not atomic.

Two questions need to be on record: where does critical state live, and how are multi-write invariants kept
consistent across a crash?

## Decision drivers

- Crash-safety is a hard requirement: a power-cut at any instant must leave persisted state consistent, and recovery
  must not depend on a graceful shutdown having run.
- Single-instance, operator-controlled deployment
  ([migration model ADR](./0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md)).
- Avoid bespoke durability machinery: Reverie should not hand-roll a write-ahead log or crash-recovery logic that a
  database already provides.
- Make the existing convention a rule so new code does not regress critical state into process memory.

## Considered options

- Postgres-backed state with atomic transactions
- Critical state in process memory or local files, rebuilt on restart
- Postgres-backed state without enclosing transactions

## Decision outcome

Chosen option: **Postgres-backed state with atomic transactions**, because it delegates durability and atomicity to a
database built for exactly that, and crash-safety follows from Postgres WAL and fsync rather than from any
Reverie-specific recovery code.

Critical, durable state lives in Postgres. Process memory holds only derived or cached state that can be rebuilt from
Postgres on restart, for example the settings cache, which is reloaded from its table on startup
([persist operator-tunable settings ADR](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md)).
An in-memory value is never the source of truth.

Multi-write invariants are atomic. Any state change spanning multiple rows or tables that must hold an invariant is
wrapped in a single transaction: it commits whole or rolls back whole, never half.

Crash-safety follows by construction. A committed transaction survives an instant kill through Postgres WAL and
fsync; an uncommitted one rolls back on recovery. Reverie therefore needs no application-level write-ahead log and no
custom crash-recovery code for committed state: a crash is a safe event.

The crash-safety of in-flight background work, a job killed mid-execution, is out of scope here and is owned by the
[durable job queue ADR](./0018-durable-job-queue-postgres-backed-skip-locked-crash-only.md) (leases, visibility
timeouts, idempotency). Statelessness as a horizontal-scaling enabler is owned by the
[scale-stance ADR](../../adr/2026-06-08-scale-stance-stateless-enable-not-own.md). This decision covers only the
durability and atomicity of persisted state.

This guarantee rests on Postgres running with `fsync` enabled, which is the default. An operator who disables
`fsync`, or runs on storage that lies about flushes, voids the crash-safety guarantee; that operator dependency is
documented and not undertaken.

### Consequences

- Positive: an instant kill leaves committed state consistent with no bespoke recovery code, because Postgres'
  durability does the work.
- Positive: a multi-write invariant can never half-apply.
- Positive: it codifies the shape settings and sessions already follow, so new code inherits the rule instead of
  re-deciding per feature.
- Negative: every critical write pays a transaction commit, and its fsync, rather than a cheaper in-memory update;
  correctness of state is chosen over write latency.
- Negative: in-memory caches remain allowed but are demoted to rebuildable derived state, never authoritative.
- Negative: the guarantee is contingent on the operator not disabling `fsync`.

## Pros and cons of the options

### Postgres-backed state with atomic transactions

- Positive: durability and atomicity are delegated to a database built for exactly that.
- Positive: crash recovery requires no Reverie-specific code.
- Negative: it pays the commit and fsync cost on every critical write.

### Critical state in process memory or local files, rebuilt on restart

- Positive: in-memory reads and writes are faster.
- Negative: a crash between mutation and the next persistence point loses or corrupts state, and reconstruction logic
  is exactly the bespoke recovery code this decision avoids.
- Negative: it does not survive a power-cut, which is the requirement.

### Postgres-backed state without enclosing transactions

- Positive: individual statements are marginally simpler to write.
- Negative: a crash partway through a multi-statement change leaves the invariant half-applied, which is the failure
  mode the decision exists to prevent.

## More information

Existing instances of this rule, not re-derived here:
[persist operator-tunable settings ADR](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md)
(database-backed settings with a rebuildable in-memory cache) and
[first-party session layer ADR](./0015-first-party-session-layer-on-the-tower-sessions-core.md) (sessions in a
Postgres table).

[Durable job queue ADR](./0018-durable-job-queue-postgres-backed-skip-locked-crash-only.md) owns the crash-safety of
in-flight work, the complement to this decision's committed-state durability.
[Scale-stance ADR](../../adr/2026-06-08-scale-stance-stateless-enable-not-own.md) owns statelessness as a scaling
enabler; this decision owns it as a durability property.

Revisit trigger: if a future feature has a genuine need for authoritative in-memory or non-Postgres durable state,
for example an embedded cache that must survive restart, it gets its own ADR rather than an exception here.
