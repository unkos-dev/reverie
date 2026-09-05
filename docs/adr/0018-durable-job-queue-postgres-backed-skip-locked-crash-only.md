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

Reverie runs background work (enrichment, cover/metadata writeback, ingestion). The claim path already exists: jobs
are Postgres rows, claimed with `FOR UPDATE SKIP LOCKED`, with one `in_progress` row per work-unit enforced by a
partial unique index. What is half-wired is crash recovery of an orphaned `in_progress` row: writeback reverts
orphaned `in_progress` rows to `pending` at startup, but enrichment reverts only on graceful shutdown, so a hard kill
of enrichment strands its `in_progress` rows with nothing to reclaim them. No decision is on record for how a crashed
job is reclaimed.

The [crash-safe state ADR](./0020-durable-crash-safe-state-in-postgres-via-atomic-transactions.md) makes committed state survive an
instant kill and explicitly defers the crash-safety of in-flight work to here. Reverie is single-instance and
durable-not-distributed
([scale-stance ADR](./0021-scale-stance-stateless-application-operator-enabled-ha.md)), so the requirement is durable,
safe reclaim, not distribution. The open question: how is a crashed job reclaimed, and does the default deployment
need wall-clock lease or visibility timeouts to do it?

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
- At-least-once is acceptable if handlers are idempotent. Exactly-once is not a realistic guarantee; idempotency is
  what makes reclaim-and-retry safe.

## Considered options

- Restart-bounded reclaim: `FOR UPDATE SKIP LOCKED` claim, startup-revert of orphaned `in_progress` rows, per-job
  timeouts, and a panic guard; crash-only; idempotent handlers.
- Lease or visibility-timeout reclaim (wall-clock), with heartbeat renewal for long jobs.
- A dedicated external message broker or queue service.
- In-memory or best-effort dispatch with no durable reclaim.

## Decision outcome

Chosen option: **Restart-bounded reclaim**, because within a single instance a restart proves every `in_progress` row
is an orphan, giving exact reclaim with no lease to tune, while reusing the already-crash-safe Postgres store instead
of adding new infrastructure.

An instance is one app process (the deployment unit); a worker is one of the N concurrent job-running tasks inside it
(`enrichment.concurrency`, a semaphore-gated pool). Reclaim exactness rests on the instance boundary, not on worker
count.

- Jobs are Postgres rows claimed with `FOR UPDATE SKIP LOCKED`, so concurrent workers never grab the same job. This is
  the concurrency-safe primitive the
  [scale-stance ADR](./0021-scale-stance-stateless-application-operator-enabled-ha.md) names as a "don't preclude scale"
  guardrail. Mutual exclusion of one `in_progress` row per work-unit is enforced by a partial unique index.
- Crash recovery is restart-bounded. At instance startup, once per process boot, before the worker pool begins
  claiming, orphaned `in_progress` rows are reverted to `pending` and re-claimed. Because every worker lives inside
  the one instance, a crash kills them all together, so a restart proves any `in_progress` row is an orphan
  regardless of how many workers were running; reclaim is exact and needs no timeout to tune.
- Live-worker job death is closed by point fixes, not a lease. A worker task that dies while the instance stays alive
  (panic or hang) is the one case startup-revert misses, and in a pool it is the common individual failure, not
  whole-process death. Hangs are bounded by per-job timeouts, which is already a project-wide invariant (enrichment
  has a fetch budget), turning a hang into a caught error that completes the job's bookkeeping. A task panic re-pends
  the row via a guard on the spawned task. Both are required, not optional, for this option to be complete.
- Handlers are idempotent. Because reclaim re-runs a job that may have partially executed, every handler must be safe
  to run again. File-mutating jobs (writeback: OPF rewrite, cover embed, path rename) are not transactional with
  their Postgres row; the crash-safe-state ADR's transaction guarantee does not extend to filesystem writes, so each
  must document its re-run safety (write-to-temp-then-rename, or a per-work-unit guard), not merely be labelled
  idempotent.
- Workers are crash-only. Correctness never depends on a graceful shutdown having run. A SIGTERM drain (stop
  claiming, finish in-flight work) is an optimisation that avoids needless re-runs on a planned restart (politeness,
  not correctness).
- No distribution. Durability and mutual exclusion come from Postgres; there is no external broker, distributed
  scheduler, or cross-node coordination. Parallelism is a pool of N workers within the instance against the one
  queue.

Deferred: lease or visibility-timeout reclaim is the multi-instance lift, not part of the default. The moment an
operator runs multiple instances (enabled, not owned, by the
[scale-stance ADR](./0021-scale-stance-stateless-application-operator-enabled-ha.md); no leader election means each
instance runs its own worker pool), restart-bounded reclaim becomes unsafe: one instance booting would re-pend a
peer's still-running job. That topology, and only that topology, needs wall-clock leases plus heartbeat renewal to
avoid double-running long jobs. Adopting it now would buy nothing for the single-instance default and would add a
double-run hazard for long writeback jobs (a fixed lease expiring while the holder is still mutating an EPUB), so it
is a non-trivial build that waits for the topology that justifies it.

### Consequences

- Positive: reclaim is exact (driven by the restart signal, not a clock) with no double-run-while-alive hazard and no
  lease tuning.
- Positive: the `SKIP LOCKED` claim is concurrency-safe, so it already satisfies the scale-stance guardrail without
  the rest of a lease.
- Positive: it reuses the already-crash-safe Postgres and adds no queue component to deploy or monitor.
- Positive: it holds the same defer-multi-instance posture as the pooling ADR, keeping the data layer's stance
  consistent.
- Negative: restart-bounded reclaim does not recover a job whose worker died while the process stayed alive; that
  case is only covered if the per-job timeout and the panic guard are in place, so they are mandatory, not
  nice-to-have.
- Negative: multi-instance support carries an additive migration later (lease columns, reaper, heartbeat); deferred,
  not free.
- Neutral: idempotency is a hard per-handler obligation, and file-mutating handlers must prove re-run safety rather
  than assert it.

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

- Positive: purpose-built brokers offer high throughput and rich delivery semantics.
- Negative: it adds a stateful component and a single point of failure, duplicating durability Postgres already
  provides, for a job cadence that does not need broker-grade throughput.

### In-memory or best-effort dispatch

- Positive: it is the least code in the immediate term.
- Negative: a crash loses in-flight and queued work, the failure this decision exists to remove.

## More information

`backend/src/services/writeback/queue.rs` calls `revert_in_progress` both before it starts polling and on shutdown,
while `backend/src/services/enrichment/queue.rs` calls it only on shutdown, so an enrichment row orphaned by a hard
crash stays `in_progress` until the next graceful shutdown.

Sibling ADR: [crash-safe state](./0020-durable-crash-safe-state-in-postgres-via-atomic-transactions.md), committed-state
durability; this ADR is its in-flight-work complement, and the boundary it notes (transactions do not cover
filesystem writes) is why file-mutating handlers must prove re-run safety.

Sibling ADR: [scale stance](./0021-scale-stance-stateless-application-operator-enabled-ha.md), durable-not-distributed
posture and the `SKIP LOCKED` concurrency guardrail; multi-instance is the trigger for the deferred lease.

Sibling ADR: [pooling](./0017-in-process-sqlx-pgpool-as-the-sole-pooling-layer.md), the same
defer-the-multi-instance-concern posture this ADR mirrors.

Revisit trigger: when multi-instance becomes a supported deployment, adopt lease or visibility-timeout reclaim with
heartbeat renewal: an additive change layered on the claim model decided here.
