---
type: ADR
profile-version: 1
id: "REV-ADR-0068"
title: "Durable job queue: restart-bounded reclaim with fenced enrichment rescue"
status: "accepted"
recorded-on: "2026-11-05"
decided-on: "2026-11-05"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0018"
---

# Durable job queue: restart-bounded reclaim with fenced enrichment rescue

## Context and problem statement

Reverie runs background work (enrichment, writeback, ingestion, mail) as Postgres rows claimed with
`FOR UPDATE SKIP LOCKED`, with one `in_progress` row per work unit. A job whose worker fails or whose process dies must
be reclaimed without losing work and without two attempts changing the same state. Reverie is single-instance and
durable-not-distributed ([scale-stance ADR](./0021-scale-stance-stateless-application-operator-enabled-ha.md)).

Within one instance, a restart proves every `in_progress` row is an orphan. A failure inside a live process proves
nothing of the kind: an enrichment attempt can lose its database connection while recording its outcome, or end with a
transaction whose outcome PostgreSQL has not confirmed, and its book then stays `in_progress` until the next restart.
Enrichment runs under a configured whole-run deadline and changes only database state. How should an orphaned claim be
reclaimed, and where does a wall-clock rule earn its place?

## Decision drivers

- In-flight work survives an instant kill with no lost work and no permanently stuck `in_progress` rows.
- A failure inside a live process does not strand a job until restart.
- A reclaimed attempt can never change state after its claim is released.
- Reclaim stays exact where it can be: a wall-clock guess must not catch a healthy job.
- Reuse Postgres and the claim primitive already in place; add no broker or queue component.
- At-least-once is acceptable when handlers are idempotent.

## Considered options

- Restart-bounded reclaim for every queue, with per-job timeouts and a panic guard.
- Restart-bounded reclaim for every queue, plus fenced time-based rescue for enrichment.
- Lease or visibility-timeout reclaim with heartbeat renewal for every queue.
- A Postgres-backed queue library that owns its own job tables.
- A dedicated external message broker or queue service.

## Decision outcome

Chosen option: **restart-bounded reclaim for every queue, plus fenced time-based rescue for enrichment**, because the
enrichment deadline makes a time rule exact enough for enrichment alone, and fencing makes a premature rescue cost a
repeated provider lookup rather than overlapping writes.

Every queue keeps the restart-bounded model: a SKIP LOCKED claim, startup revert of orphaned `in_progress` rows, per-job
timeouts and a panic guard. Enrichment adds two things. Each claim increments a per-row claim sequence that nothing
resets, and every write an attempt makes is conditioned on that sequence: its catalogue transaction locks the
manifestation row against the sequence first and holds the lock to commit, and its outcome bookkeeping updates only a
row still carrying it. On each poll, the queue fails `in_progress` claims older than twice the run budget through the
ordinary failure transition, skipping rows another transaction has locked. A rerun request does not reset the sequence,
so the next claim can never share a stale attempt's fence.

### Consequences

- Positive: an enrichment claim stranded by a live-process failure is released without a restart, through the same
  backoff, attempt cap and rerun rules as any other failure.
- Positive: a stale attempt cannot write after rescue, and rescue cannot take a claim whose catalogue transaction is
  still open, so overlapping attempts never change the same catalogue state.
- Positive: writeback, ingestion and mail keep the exact restart signal, where a lease could let two workers mutate the
  same file.
- Negative: two reclaim mechanisms coexist, and only enrichment is restart-free for live-process failures. A live
  failure in another queue still waits for its per-job timeout, panic guard or restart.
- Negative: the rescue threshold is a fixed multiple of the run budget, not a separate setting. A run still cleaning up
  past twice its budget is rescued and repeated.
- Negative: the claim sequence is a schema change, and every enrichment write path must carry the fence.
- Neutral: idempotency remains a per-handler obligation; file-mutating handlers prove re-run safety rather than assert
  it.

## Pros and cons of the options

### Restart-bounded reclaim for every queue

- Positive: reclaim is exact and needs no timeout tuning.
- Negative: a live-process failure strands an enrichment claim until restart, and nothing short of a restart reclaims
  it.

### Lease or visibility-timeout reclaim for every queue

- Positive: it is the correct mechanism once multiple instances run, where no worker may assume a peer is dead.
- Negative: for queues without a bounded run time it is a wall-clock guess that repeats long healthy jobs unless
  heartbeat renewal is added.
- Negative: a lease expiring mid-run can let two workers mutate the same EPUB.

### A Postgres-backed queue library

- Positive: retries, worker heartbeats, orphan recovery and a monitoring surface are maintained upstream.
- Negative: the library keeps job state in its own tables, while enrichment's queue state lives on the manifestation row
  that the failure dashboard, trigger endpoint and metadata routes read and write. Adopting it replaces the queue model
  rather than adding rescue.
- Negative: heartbeat-based orphan recovery does not release a job stranded inside a worker that is still alive.

### A dedicated external message broker

- Positive: purpose-built brokers have high throughput and rich delivery semantics.
- Negative: it adds a stateful component and a single point of failure, and cannot join the Postgres transaction that
  commits a catalogue change with its writeback job.

## More information

Writeback relocation replay uses paired manifestation paths and stored hash/size evidence under the current job claim.
Every claim reconciles before rewriting and reloads its snapshot afterwards. Exhausted jobs retain their budgets while a
bounded startup/five-minute sweep supplies relocation-only carriers. Permanent evidence clears intent and records the
terminal diagnosis in one queue::finish transaction, retaining exclusion until commit. The `relocation_recovery_` and
`relocation_queue_` cases exercise evidence states and transactional exclusion through existing filesystem and database
seams. They do not establish continuation, carrier behaviour, cover-sidecar replay, complete writeback replay safety,
process-kill behaviour or NAS flush guarantees. Successful re-sync after a reported error is a weaker guarantee than
proof that earlier writes became durable.

Shared path claims enforce owner-correct recorded and intent locations. Ordinary recovery retains its edit budget until
destination finalisation commits the edit debit; carriers keep bounded attempts, and finish reads the durable count.
Transient reset, sweep and claim errors retry within the existing worker without losing its tracked jobs. The
`path_claim_` cases cover publisher ownership through existing test seams.

Sibling ADR: [crash-safe state](./0020-durable-crash-safe-state-in-postgres-via-atomic-transactions.md), committed-state
durability; this ADR is its in-flight-work complement, and the boundary it notes (transactions do not cover filesystem
writes) is why file-mutating handlers must prove re-run safety.

Sibling ADR: [pooling](./0017-in-process-sqlx-pgpool-as-the-sole-pooling-layer.md), the same
defer-the-multi-instance-concern posture this ADR mirrors.

Revisit trigger: when multi-instance becomes a supported deployment, adopt lease or visibility-timeout reclaim with
heartbeat renewal for every queue, layered on the claim model and the enrichment claim sequence decided here.
