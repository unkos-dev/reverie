---
status: "accepted"
date: 2026-06-08
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Durable, crash-safe state in Postgres via atomic transactions

## Context and Problem Statement

Reverie is self-hosted on hardware the operator controls, including consumer
hardware with no UPS, where an instant power-cut is a realistic event, not a
theoretical one. The system must come back up consistent after any abrupt kill,
with no half-applied changes and no bespoke recovery dance.

Durable state already lives in Postgres in the places that matter: operator
settings ([persisted-settings ADR](2026-05-26-persisted-settings.md)) and
sessions ([first-party session layer ADR](2026-06-04-first-party-session-layer.md))
, but this is convention, not a recorded decision, so nothing stops new code
from parking critical state in process memory or a local file where a crash
would lose or corrupt it. Equally, a state change that spans several rows or
tables can leave an invariant half-applied if it is not atomic.

Two questions need to be on record: where does critical state live, and how are
multi-write invariants kept consistent across a crash?

## Decision Drivers

- **Crash-safety is a hard requirement.** A power-cut at any instant must leave
  persisted state consistent; recovery must not depend on a graceful shutdown
  having run.
- **Single-instance, operator-controlled deployment**
  ([migration model ADR](2026-06-02-hybrid-migration-entrypoints-and-role.md)).
- **Avoid bespoke durability machinery.** Reverie should not hand-roll a
  write-ahead log or crash-recovery logic that a database already provides.
- **Make the existing convention a rule** so new code does not regress critical
  state into process memory.

## Considered Options

- **A**: Critical state lives in Postgres; multi-write invariants are wrapped in
  atomic transactions; crash-safety rests on Postgres WAL + fsync.\*\*
- **B**: Keep some critical state in process memory or local files for speed and
  reconstruct it on restart.\*\*
- **C**: Persist state to Postgres but apply multi-row changes as independent
  statements (no enclosing transaction), relying on statement ordering.\*\*

## Decision Outcome

Chosen option: **A**.

- **Critical, durable state lives in Postgres.** Process memory holds only
  derived or cached state that can be rebuilt from Postgres on restart, for
  example the settings cache, which is reloaded from its table on startup
  ([persisted-settings ADR](2026-05-26-persisted-settings.md)). An in-memory
  value is never the source of truth.
- **Multi-write invariants are atomic.** Any state change spanning multiple
  rows or tables that must hold an invariant is wrapped in a single
  transaction: it commits whole or rolls back whole, never half.
- **Crash-safety follows by construction.** A committed transaction survives an
  instant kill through Postgres WAL + fsync; an uncommitted one rolls back on
  recovery. Reverie therefore needs no application-level write-ahead log and no
  custom crash-recovery code for committed state: a crash is a safe event.

The crash-safety of in-flight background _work_ (a job killed mid-execution) is
out of scope here and is owned by the
[durable job queue ADR](2026-06-08-durable-job-queue-crash-only.md) (leases,
visibility timeouts, idempotency). Statelessness as a horizontal-scaling
_enabler_ is owned by the
[scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md). This
ADR records only the durability and atomicity of persisted state.

This guarantee rests on Postgres running with `fsync` enabled (the default). An
operator who disables `fsync` or runs on storage that lies about flushes voids
the crash-safety guarantee; that operator dependency must be documented and not
done.

### Consequences

- Good, because an instant kill leaves committed state consistent with no
  bespoke recovery code: Postgres' durability does the work.
- Good, because a multi-write invariant can never half-apply.
- Good, because it codifies the shape settings and sessions already follow, so
  new code inherits the rule instead of re-deciding per feature.
- Bad, because every critical write pays a transaction commit (and its fsync)
  rather than a cheaper in-memory update; correctness of state is chosen over
  write latency.
- Neutral, because in-memory caches remain allowed but are demoted to
  rebuildable derived state, never authoritative.
- Neutral, because the guarantee is contingent on the operator not disabling
  `fsync`.

### Confirmation

Enforced as the `backend/CLAUDE.md` data-access invariants: **"Stateless
application: no critical state in process memory; durable state lives in
Postgres"** and **"Atomic transactions for multi-write invariants."** New
list/detail and mutation paths are reviewed against both; a multi-write change
without an enclosing transaction is a review block.

## Pros and Cons of the Options

### A: Postgres-backed state, atomic writes, WAL/fsync crash-safety

- Good, because durability and atomicity are delegated to a database built for
  exactly that.
- Good, because crash recovery requires no Reverie-specific code.
- Bad, because it pays the commit/fsync cost on every critical write.

### B: critical state in memory / local files, rebuilt on restart

- Good, because in-memory reads and writes are faster.
- Bad, because a crash between mutation and the next persistence point loses or
  corrupts state, and reconstruction logic is exactly the bespoke recovery code
  this decision avoids.
- Bad, because it does not survive a power-cut, which is the requirement.

### C: Postgres-backed but no enclosing transaction

- Good, because individual statements are marginally simpler to write.
- Bad, because a crash partway through a multi-statement change leaves the
  invariant half-applied: the failure mode the decision exists to prevent.

## More Information

- Existing instances of this rule (not re-derived here):
  [persisted-settings ADR](2026-05-26-persisted-settings.md) (DB-backed
  settings with a rebuildable in-memory cache) and
  [first-party session layer ADR](2026-06-04-first-party-session-layer.md)
  (sessions in a Postgres table).
- [Durable job queue ADR](2026-06-08-durable-job-queue-crash-only.md): owns
  the crash-safety of in-flight work, the complement to this ADR's
  committed-state durability.
- [Scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md):
  owns statelessness as a scaling enabler; this ADR owns it as a durability
  property.
- Revisit trigger: if a future feature has a genuine need for authoritative
  in-memory or non-Postgres durable state (e.g. an embedded cache that must
  survive restart), it gets its own ADR rather than an exception here.
