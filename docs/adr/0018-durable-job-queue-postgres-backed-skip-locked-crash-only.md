---
type: ADR
profile-version: 1
id: "REV-ADR-0018"
title: "Durable job queue: Postgres-backed, SKIP LOCKED, crash-only"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-08"
decision-makers:
  - "John Unkovich"
---

# Durable job queue: Postgres-backed, SKIP LOCKED, crash-only

## Context and problem statement

Reverie runs background work (enrichment, cover/metadata writeback, ingestion). The claim path already exists: jobs are
Postgres rows, claimed with `FOR UPDATE SKIP LOCKED`, with one `in_progress` row per work-unit enforced by a partial
unique index. What is half-wired is crash recovery of an orphaned `in_progress` row: writeback reverts orphaned
`in_progress` rows to `pending` at startup, but enrichment reverts only on graceful shutdown, so a hard kill of
enrichment strands its `in_progress` rows with nothing to reclaim them. No decision is on record for how a crashed job
is reclaimed.

The [crash-safe state ADR](./0020-durable-crash-safe-state-in-postgres-via-atomic-transactions.md) makes committed state
survive an instant kill and explicitly defers the crash-safety of in-flight work to here. Reverie is single-instance and
durable-not-distributed ([scale-stance ADR](./0021-scale-stance-stateless-application-operator-enabled-ha.md)), so the
requirement is durable, safe reclaim, not distribution. The open question: how is a crashed job reclaimed, and does the
default deployment need wall-clock lease or visibility timeouts to do it?

## Decision drivers

- In-flight work must survive an instant kill with no lost work and no permanently stuck `in_progress` rows, without
  bespoke liveness tracking.
- Single-instance is the default; the job model should not pre-build multi-instance machinery. The
  [pooling ADR](./0017-in-process-sqlx-pgpool-as-the-sole-pooling-layer.md) defers its multi-instance concern to the
  topology that needs it, and the job model should hold the same posture.
- Exact reclaim beats a wall-clock guess. Within a single instance, a restart proves every `in_progress` row is an
  orphan (the process that held them is gone); a fixed timeout only guesses whether the holder is dead, and guesses
  wrong for a long-but-healthy job.
- Reuse Postgres, add no broker. Postgres is already the crash-safe store; a queue service is another component and a
  single point of failure.
- At-least-once is acceptable if handlers are idempotent. Exactly-once is not a realistic guarantee; idempotency is what
  makes reclaim-and-retry safe.

## Considered options

- Restart-bounded reclaim: `FOR UPDATE SKIP LOCKED` claim, startup-revert of orphaned `in_progress` rows, per-job
  timeouts, and a panic guard; crash-only; idempotent handlers.
- Lease or visibility-timeout reclaim (wall-clock), with heartbeat renewal for long jobs.
- A dedicated external message broker or queue service.
- In-memory or best-effort dispatch with no durable reclaim.

## Decision outcome

Chosen option: **Restart-bounded reclaim**, because within a single instance a restart proves every `in_progress` row is
an orphan, so reclaim is exact with no lease to tune, and it reuses the already-crash-safe Postgres store instead of
adding new infrastructure.

Jobs are durable Postgres rows claimed with SKIP LOCKED. An instance restart reclaims its orphaned in-progress jobs;
handlers must tolerate replay, and a live worker failure must not strand a job. A lease-based multi-instance queue is
deferred until that topology is adopted.

### Consequences

- Positive: reclaim is exact (driven by the restart signal, not a clock) with no double-run-while-alive hazard and no
  lease tuning.
- Positive: the `SKIP LOCKED` claim is concurrency-safe, so it already satisfies the scale-stance guardrail without the
  rest of a lease.
- Positive: it reuses the already-crash-safe Postgres and adds no queue component to deploy or monitor.
- Positive: it holds the same defer-multi-instance posture as the pooling ADR, keeping the data layer's stance
  consistent.
- Negative: restart-bounded reclaim does not recover a job whose worker died while the process stayed alive; that case
  is only covered if the per-job timeout and the panic guard are in place, so they are mandatory.
- Negative: multi-instance support carries an additive migration later (lease columns, reaper, heartbeat); deferring it
  does not remove that cost.
- Neutral: idempotency is a hard per-handler obligation, and file-mutating handlers must prove re-run safety rather than
  assert it.

## Pros and cons of the options

### Restart-bounded reclaim

- Positive: reclaim is exact for the single-instance default and needs no timeout tuning.
- Positive: it reuses Postgres and the claim primitive already in place.
- Negative: it needs the panic guard and per-job timeout to cover live-process task death, and is unsafe under
  multi-instance, which is why the lease is the deferred lift, not a rejected idea.

### Lease or visibility-timeout reclaim

- Positive: it is the correct mechanism once multiple instances run, where no worker may assume a peer is dead.
- Negative: for a single instance it replaces an exact restart signal with a wall-clock guess that double-runs
  long-but-healthy jobs unless heartbeat renewal is added, which is real machinery for a topology not in the default.
- Negative: a fixed lease expiring mid-run can let two workers mutate the same EPUB concurrently.

### Dedicated external message broker

- Positive: purpose-built brokers have high throughput and rich delivery semantics.
- Negative: it adds a stateful component and a single point of failure, duplicating durability Postgres already
  provides, for a job cadence that does not need broker-grade throughput.

### In-memory or best-effort dispatch

- Positive: it is the least code in the immediate term.
- Negative: a crash loses in-flight and queued work, the failure this decision exists to remove.

## More information

Sibling ADR: [crash-safe state](./0020-durable-crash-safe-state-in-postgres-via-atomic-transactions.md), committed-state
durability; this ADR is its in-flight-work complement, and the boundary it notes (transactions do not cover filesystem
writes) is why file-mutating handlers must prove re-run safety.

Sibling ADR: [scale stance](./0021-scale-stance-stateless-application-operator-enabled-ha.md), durable-not-distributed
posture and the `SKIP LOCKED` concurrency guardrail; multi-instance is the trigger for the deferred lease.

Sibling ADR: [pooling](./0017-in-process-sqlx-pgpool-as-the-sole-pooling-layer.md), the same
defer-the-multi-instance-concern posture this ADR mirrors.

Revisit trigger: when multi-instance becomes a supported deployment, adopt lease or visibility-timeout reclaim with
heartbeat renewal: an additive change layered on the claim model decided here.
