---
status: "proposed"
date: 2026-06-08
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Scale stance: stateless application, operator-enabled HA, no first-party distributed infrastructure

## Context and Problem Statement

Reverie's supported deployment is a single-instance Docker Compose service
([migration model ADR](2026-06-02-hybrid-migration-entrypoints-and-role.md)).
That topology is a single point of failure: one app process, one Postgres. Some
operators will want to remove that — run several app instances behind a load
balancer, or a hot-standby for availability.

The question is how far Reverie goes to meet them. It could build first-party
high-availability machinery (leader election, node discovery, clustering, a
distributed work scheduler), or it could stay a stateless application that an
operator is free to scale out using infrastructure they already run. Reverie's
purpose is a library manager, not a distributed-systems runtime, and its
audience self-hosts. Where is the line between _enabling_ scale and _owning_ it?

## Decision Drivers

- **Bounded scope.** Owning HA means owning leader election, failover,
  split-brain handling, and their entire test and support surface — a different
  product. Every such component is also operational burden on the common
  single-instance self-hoster who will never use it.
- **Don't architecturally preclude scale-out.** Statelessness is cheap
  insurance; painting the app into a single-instance corner would be hard to
  undo later.
- **The HA-wanting operator already runs the infrastructure.** A load balancer
  and a highly-available Postgres are things that audience operates already;
  Reverie re-implementing them adds no value.
- **"Enable, don't own"** — the same philosophy applied to integrations
  ([standards-first integrations ADR](2026-06-08-standards-first-integrations.md))
  and to connection pooling
  ([pooling ADR](2026-06-08-connection-pooling-pgpool.md)).

## Considered Options

- **A — Stateless application enables operator-run multi-instance / HA; Reverie
  owns no distributed infrastructure.**
- **B — Build first-party HA: leader election, clustering, node discovery,
  distributed scheduling.**
- **C — Single-instance only; allow stateful shortcuts that actively preclude
  scale-out.**

## Decision Outcome

Chosen option: **A**.

- **The application is stateless.** All durable state lives in Postgres
  ([crash-safe state ADR](2026-06-08-postgres-backed-crash-safe-state.md)),
  sessions included
  ([first-party session layer ADR](2026-06-04-first-party-session-layer.md)),
  so no instance holds authoritative in-memory state and no sticky-session
  affinity is required. An operator can therefore run N app instances behind
  their own load balancer, or a standby, all pointing at one Postgres — which
  they may make highly available by their own means (managed Postgres,
  replication, etc.).
- **Reverie owns no distributed infrastructure.** No first-party leader
  election, node discovery, clustering, failover orchestration, or distributed
  work scheduler. Background workers are designed _durable, not distributed_
  ([durable job queue ADR](2026-06-08-durable-job-queue-crash-only.md)).
- **Single-instance is the supported default; multi-instance is operator-owned.**
  When an operator scales out, they own the load balancer, the Postgres HA, and
  the connection-budget sizing the
  [pooling ADR](2026-06-08-connection-pooling-pgpool.md) deferred (N instances ×
  pool size against one Postgres).

This stance does require a small standing guardrail: features must stay _safe_
under concurrent instances even though one is the default — startup migration is
already advisory-locked so two instances can't double-migrate
([migration model ADR](2026-06-02-hybrid-migration-entrypoints-and-role.md)),
and job claim is concurrency-safe
([durable job queue ADR](2026-06-08-durable-job-queue-crash-only.md)). These are
"don't preclude scale" guardrails, not "own HA."

### Consequences

- Good, because an operator who needs availability can scale out without forking
  Reverie; statelessness is the only precondition and it is cheap.
- Good, because Reverie's scope and failure surface stay bounded — no
  distributed-systems code to maintain, fewer moving parts for the
  single-instance majority.
- Good, because it is consistent with the project-wide "enable, don't own"
  philosophy.
- Bad, because Reverie ships no turnkey HA; the operator assembles a load
  balancer and Postgres HA themselves. Acceptable — that audience runs that
  infrastructure regardless.
- Bad, because the default single-instance deployment remains a single point of
  failure; Reverie does not eliminate it, only makes elimination possible for
  operators who need it.
- Neutral, because the "safe under N instances" guardrails (advisory-locked
  migration, concurrency-safe job claim) are a small ongoing design tax even
  while single-instance is the default.

### Confirmation

Enforced as the `backend/CLAUDE.md` **"Stateless application"** invariant — no
critical state in process memory. No first-party clustering, leader-election, or
distributed-coordination code exists in the tree; the only cross-instance
coordination used is Postgres advisory locks (migration) and `FOR UPDATE SKIP
LOCKED` job claim.

## Pros and Cons of the Options

### A — stateless app, operator-enabled HA, no owned distributed infra

- Good, because it unlocks scale-out for the operators who want it at the cost
  of one architectural property (statelessness) Reverie wants anyway.
- Good, because it keeps the maintained surface small.
- Neutral, because it pushes HA assembly onto the operator.
- Bad, because it offers no built-in availability story out of the box.

### B — first-party HA

- Good, because it would give a turnkey multi-node experience.
- Bad, because it is a different product: leader election, split-brain, failover
  testing, and support — enormous scope for a self-hosted library manager.
- Bad, because it burdens the single-instance majority with components they
  never use.

### C — single-instance only, stateful shortcuts

- Good, because it is marginally simpler in the near term.
- Bad, because it forecloses scale-out permanently and would be expensive to
  reverse once stateful assumptions spread through the code.

## More Information

- [Crash-safe state ADR](2026-06-08-postgres-backed-crash-safe-state.md) —
  statelessness as a durability property; this ADR is its scaling complement.
- [Pooling ADR](2026-06-08-connection-pooling-pgpool.md) — owns the
  multi-instance connection-budget sizing referenced above.
- [Durable job queue ADR](2026-06-08-durable-job-queue-crash-only.md) —
  durable-not-distributed worker design.
- [Standards-first integrations ADR](2026-06-08-standards-first-integrations.md)
  — the same "enable, don't own" philosophy on the integrations axis.
- Revisit trigger: if first-party HA ever becomes a goal (e.g. a hosted Reverie
  offering), this stance gets a superseding ADR — it is not amended by exception.
