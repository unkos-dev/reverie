---
status: "accepted"
date: 2026-06-08
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Durable job queue: Postgres-backed, SKIP-LOCKED, crash-only via restart-bounded reclaim (not distributed)

## Context and Problem Statement

Reverie runs background work — enrichment, cover/metadata writeback, ingestion
follow-up. The claim path already exists: jobs are Postgres rows, claimed with
`FOR UPDATE SKIP LOCKED`, with one `in_progress` row per work-unit enforced by a
partial unique index. What is half-wired is crash recovery of an orphaned
`in_progress` row: writeback reverts orphaned `in_progress` rows to `pending` at
startup, but enrichment reverts only on graceful shutdown — so a hard
kill of enrichment strands its `in_progress` rows with nothing to reclaim them.
No decision is on record for how a crashed job is reclaimed.

The [crash-safe state ADR](2026-06-08-postgres-backed-crash-safe-state.md) makes
_committed_ state survive an instant kill and explicitly defers the crash-safety
of _in-flight work_ to here. Reverie is single-instance and
durable-not-distributed
([scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md)), so the
requirement is durable, safe reclaim — not distribution. The open question: how
is a crashed job reclaimed, and does the default deployment need wall-clock
lease / visibility timeouts to do it?

## Decision Drivers

- **In-flight work must survive an instant kill** with no lost work and no
  permanently-stuck `in_progress` rows — without bespoke liveness tracking.
- **Single-instance is the default; don't pre-build multi-instance machinery.**
  The [pooling ADR](2026-06-08-connection-pooling.md) defers its
  multi-instance concern to the topology that needs it; the job model should
  hold the same posture.
- **Exact reclaim beats a wall-clock guess.** Within a single instance, a
  restart _proves_ every `in_progress` row is an orphan (the process that held
  them is gone); a fixed timeout only guesses whether the holder is dead, and
  guesses wrong for a long-but-healthy job.
- **Reuse Postgres, add no broker.** Postgres is already the crash-safe store; a
  queue service is another component and SPOF.
- **At-least-once is acceptable if handlers are idempotent.** Exactly-once is not
  a realistic guarantee; idempotency is what makes reclaim-and-retry safe.

## Considered Options

- **A — Restart-bounded reclaim: `FOR UPDATE SKIP LOCKED` claim + startup-revert
  of orphaned `in_progress` + per-job timeouts + a panic guard; crash-only;
  idempotent handlers.**
- **B — Lease / visibility-timeout reclaim (wall-clock), with heartbeat renewal
  for long jobs.**
- **C — A dedicated external message broker / queue service.**
- **D — In-memory or best-effort dispatch with no durable reclaim.**

## Decision Outcome

Chosen option: **A**.

_Terminology:_ an **instance** is one app process (the deployment unit); a
**worker** is one of the N concurrent job-running tasks inside it
(`enrichment.concurrency`, a `Semaphore`-gated pool). Reclaim exactness rests on
the **instance** boundary, not on worker count.

- **Jobs are Postgres rows claimed with `FOR UPDATE SKIP LOCKED`**, so concurrent
  workers never grab the same job. This is the concurrency-safe primitive the
  [scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md) names
  as a "don't preclude scale" guardrail. Mutual exclusion of one `in_progress`
  row per work-unit is enforced by a partial unique index.
- **Crash recovery is restart-bounded.** At instance startup — once per process
  boot, before the worker pool begins claiming — orphaned `in_progress` rows are
  reverted to `pending` and re-claimed. Because every worker lives inside the one
  instance, a crash kills them all together, so a restart proves any
  `in_progress` row is an orphan regardless of how many workers were running;
  reclaim is exact and needs no timeout to tune. Writeback already reverts at
  startup; enrichment must be brought to parity (it currently reverts only on
  graceful shutdown, so a hard kill strands its rows).
- **Live-worker job death is closed by point fixes, not a lease.** A worker task
  that dies while the instance stays alive (panic or hang) is the one case
  startup-revert misses — and in a pool it is the _common_ individual failure,
  not whole-process death. Hangs are bounded by per-job timeouts — already a
  project-wide invariant (`backend/CLAUDE.md` "Timeouts + backpressure
  everywhere"; enrichment has a fetch budget) — turning a hang into a caught
  error that completes the job's bookkeeping. A task panic re-pends the row via a
  guard on the spawned task. Both are required, not optional, for this option to
  be complete.
- **Handlers are idempotent.** Because reclaim re-runs a job that may have
  partially executed, every handler must be safe to run again. File-mutating jobs
  (writeback: OPF rewrite, cover embed, path rename) are _not_ transactional with
  their Postgres row — the crash-safe-state ADR's transaction guarantee does not
  extend to filesystem writes — so each must document its re-run safety
  (write-to-temp-then-rename, or a per-work-unit guard), not merely be labelled
  idempotent.
- **Workers are crash-only.** Correctness never depends on a graceful shutdown
  having run. A SIGTERM drain (stop claiming, finish in-flight;
  [UNK-194](https://linear.app/unkos/issue/UNK-194)) is an _optimization_ that
  avoids needless re-runs on a planned restart — politeness, not correctness.
- **No distribution.** Durability and mutual exclusion come from Postgres; there
  is no external broker, distributed scheduler, or cross-node coordination.
  Parallelism is a pool of N workers within the instance against the one queue.

**Deferred: lease / visibility-timeout is the multi-instance lift, not part of
the default.** The moment an operator runs multiple instances (enabled, not
owned, by the [scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md);
no leader election means each instance runs its own worker pool), restart-bounded reclaim becomes
unsafe — one instance booting would re-pend a peer's still-running job. That
topology, and only that topology, needs wall-clock leases _plus_ heartbeat
renewal to avoid double-running long jobs. The lease lands with multi-instance
support, mirroring how the pooling ADR defers pool sizing to the same trigger.
Adopting it now would buy nothing for the single-instance default and would add a
double-run hazard for long writeback jobs (a fixed lease expiring while the
holder is still mutating an EPUB), so it is a non-trivial build that waits for the
topology that justifies it.

Implementation is the durable-queue epic
([UNK-365](https://linear.app/unkos/issue/UNK-365)); dispatcher idempotency is
tracked in [UNK-98](https://linear.app/unkos/issue/UNK-98); the enrichment
startup-revert parity gap is tracked in
[UNK-373](https://linear.app/unkos/issue/UNK-373).

### Consequences

- Good, because reclaim is exact (driven by the restart signal, not a clock) with
  no double-run-while-alive hazard and no lease tuning.
- Good, because the `SKIP LOCKED` claim is concurrency-safe, so it already
  satisfies the scale-stance guardrail without the rest of a lease.
- Good, because it reuses the already-crash-safe Postgres and adds no queue
  component to deploy or monitor.
- Good, because it holds the same defer-multi-instance posture as the pooling
  ADR, keeping the data layer's stance consistent.
- Bad, because restart-bounded reclaim does **not** recover a job whose worker
  died while the process stayed alive; that case is only covered if the per-job
  timeout and the panic guard are in place — they are therefore mandatory, not
  nice-to-have.
- Bad, because multi-instance support carries an additive migration later (lease
  columns + reaper + heartbeat); deferred, not free.
- Neutral, because idempotency is a hard per-handler obligation, and
  file-mutating handlers must prove re-run safety rather than assert it.

### Confirmation

Jobs are claimed with `FOR UPDATE SKIP LOCKED`, one `in_progress` row per
work-unit; every job is bounded by a timeout and guarded against task panic;
handlers are idempotent and file-mutating handlers document re-run safety.
No external message broker appears in the deployment. No conformant subsystem
correctness depends on graceful shutdown.

## Pros and Cons of the Options

### A — restart-bounded reclaim (chosen)

- Good, because reclaim is exact for the single-instance default and needs no
  timeout tuning.
- Good, because it reuses Postgres and the claim primitive already in place.
- Bad, because it needs the panic guard + per-job timeout to cover live-process
  task death, and is unsafe under multi-instance (which is why the lease is the
  deferred lift, not a rejected idea).

### B — lease / visibility-timeout

- Good, because it is the correct mechanism once multiple instances run, where no
  worker may assume a peer is dead.
- Bad, because for a single instance it replaces an exact restart signal with a
  wall-clock guess that double-runs long-but-healthy jobs unless heartbeat
  renewal is added — real machinery for a topology not in the default.
- Bad, because a fixed lease expiring mid-run can let two workers mutate the same
  EPUB concurrently.

### C — dedicated external message broker

- Good, because purpose-built brokers offer high throughput and rich delivery
  semantics.
- Bad, because it adds a stateful component and a SPOF, duplicating durability
  Postgres already provides, for a job cadence that does not need broker-grade
  throughput.

### D — in-memory / best-effort dispatch

- Good, because it is the least code in the immediate term.
- Bad, because a crash loses in-flight and queued work — the failure this
  decision exists to remove.

## More Information

- [Crash-safe state ADR](2026-06-08-postgres-backed-crash-safe-state.md) —
  committed-state durability; this ADR is its in-flight-work complement, and the
  boundary it notes (transactions do not cover filesystem writes) is why
  file-mutating handlers must prove re-run safety.
- [Scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md) —
  durable-not-distributed posture and the `SKIP LOCKED` concurrency guardrail;
  multi-instance is the trigger for the deferred lease.
- [Pooling ADR](2026-06-08-connection-pooling.md) — the same
  defer-the-multi-instance-concern posture this ADR mirrors.
- [UNK-365](https://linear.app/unkos/issue/UNK-365) (durable queue),
  [UNK-98](https://linear.app/unkos/issue/UNK-98) (dispatcher idempotency),
  [UNK-194](https://linear.app/unkos/issue/UNK-194) (graceful-shutdown drain) —
  implementation tracked there, not here.
- Revisit trigger: when multi-instance becomes a supported deployment, adopt
  option B (lease / visibility-timeout + heartbeat) for reclaim — an additive
  change layered on the claim model decided here.
