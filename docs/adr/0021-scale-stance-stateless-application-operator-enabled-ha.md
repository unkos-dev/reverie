---
type: ADR
profile-version: 1
id: "REV-ADR-0021"
title: "Scale stance: stateless application, operator-enabled HA"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-08"
decision-makers:
  - "John Unkovich"
---

# Scale stance: stateless application, operator-enabled HA

## Context and problem statement

Reverie's supported deployment is a single-instance Docker Compose service
([migration model ADR](./0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md)). That topology is a
single point of failure: one app process, one Postgres. Some operators will want to remove that: run several app
instances behind a load balancer, or a hot standby for availability.

The question is how far Reverie goes to meet them. It could build first-party high-availability machinery (leader
election, node discovery, clustering, a distributed work scheduler), or it could stay a stateless application that an
operator is free to scale out using infrastructure they already run. Reverie's purpose is a library manager, not a
distributed-systems runtime, and its audience self-hosts. Where is the line between enabling scale and owning it?

## Decision drivers

- Owning HA means owning leader election, failover, split-brain handling, and their entire test and support surface:
  a different product. Every such component is also operational burden on the common single-instance self-hoster who
  will never use it.
- Statelessness is cheap insurance against precluding scale-out; painting the app into a single-instance corner would
  be hard to undo later.
- The HA-wanting operator already runs a load balancer and a highly available Postgres; Reverie re-implementing them
  adds no value.
- The same "enable, don't own" philosophy already governs integrations
  ([standards-first integrations ADR](../../adr/2026-06-08-standards-first-integrations.md)) and connection pooling
  ([pooling ADR](./0017-in-process-sqlx-pgpool-as-the-sole-pooling-layer.md)).

## Considered options

- Stateless, operator-enabled scale-out: the application holds no authoritative state, so an operator can run
  multiple instances or a standby against one Postgres; Reverie owns no distributed infrastructure.
- First-party high availability: build leader election, clustering, node discovery, and a distributed scheduler.
- Single-instance only with stateful shortcuts that actively preclude scale-out.

## Decision outcome

Chosen option: **stateless, operator-enabled scale-out**, because it unlocks scale for operators who want it at the
cost of one architectural property Reverie wants regardless, while keeping the maintained surface bounded to a
library manager rather than a distributed-systems runtime.

The application is stateless. All durable state lives in Postgres
([crash-safe state ADR](./0020-durable-crash-safe-state-in-postgres-via-atomic-transactions.md)), sessions included
([first-party session layer ADR](./0015-first-party-session-layer-on-the-tower-sessions-core.md)), so no instance
holds authoritative in-memory state and no sticky-session affinity is required. An operator can therefore run
multiple app instances behind their own load balancer, or a standby, all pointing at one Postgres, which they may
make highly available by their own means.

Reverie owns no distributed infrastructure: no first-party leader election, node discovery, clustering, failover
orchestration, or distributed work scheduler. Background workers are designed durable, not distributed
([durable job queue ADR](./0018-durable-job-queue-postgres-backed-skip-locked-crash-only.md)).

Single-instance is the supported default; multi-instance is operator-owned. When an operator scales out, they own
the load balancer, the Postgres HA, and the connection-budget sizing this decision defers to the pooling ADR:
instance count multiplied by pool size against one Postgres.

This stance requires a small standing guardrail: features must stay safe under concurrent instances even though one
is the default. Startup migration is advisory-locked so two instances cannot double-migrate
([migration model ADR](./0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md)), and job claim is
concurrency-safe ([durable job queue ADR](./0018-durable-job-queue-postgres-backed-skip-locked-crash-only.md)). These
guardrails keep scale-out from being precluded; they are not first-party HA.

### Consequences

- Positive: an operator who needs availability can scale out without forking Reverie; statelessness is the
  architectural precondition and it is cheap. It is necessary but not sufficient: a reclaim lease bounded only by
  restart is unsafe across concurrent instances, so that remains the outstanding multi-instance lift.
- Positive: Reverie's scope and failure surface stay bounded: no distributed-systems code to maintain, fewer moving
  parts for the single-instance majority.
- Positive: the stance is consistent with the project-wide "enable, don't own" philosophy applied elsewhere.
- Negative: Reverie ships no turnkey HA; the operator assembles a load balancer and Postgres HA themselves. This is
  acceptable, as that audience already operates that infrastructure.
- Negative: the default single-instance deployment remains a single point of failure; Reverie does not eliminate it,
  only makes elimination possible for operators who need it.
- Negative: the "safe under multiple instances" guardrails (advisory-locked migration, concurrency-safe job claim)
  are a small ongoing design tax even while single-instance is the default.

## Pros and cons of the options

### Stateless, operator-enabled scale-out

- Positive: it unlocks scale-out for the operators who want it at the cost of one architectural property
  (statelessness) Reverie wants anyway.
- Positive: it keeps the maintained surface small.
- Neutral: it pushes HA assembly onto the operator.
- Negative: it offers no built-in availability story out of the box.

### First-party high availability

- Positive: it would give a turnkey multi-node experience.
- Negative: it is a different product: leader election, split-brain, and failover testing are enormous scope for a
  self-hosted library manager.
- Negative: it burdens the single-instance majority with components they never use.

### Single-instance only with stateful shortcuts

- Positive: it is marginally simpler in the near term.
- Negative: it forecloses scale-out permanently and would be expensive to reverse once stateful assumptions spread
  through the code.

## More information

- [Crash-safe state ADR](./0020-durable-crash-safe-state-in-postgres-via-atomic-transactions.md): statelessness as a
  durability property; this decision is its scaling complement.
- [Pooling ADR](./0017-in-process-sqlx-pgpool-as-the-sole-pooling-layer.md): owns the in-process pool; the
  multi-instance connection-budget sizing (instance count multiplied by pool size) is owned here, on the scale axis.
- [Durable job queue ADR](./0018-durable-job-queue-postgres-backed-skip-locked-crash-only.md): durable-not-distributed
  worker design.
- [Standards-first integrations ADR](../../adr/2026-06-08-standards-first-integrations.md): the same "enable, don't
  own" philosophy on the integrations axis.
- Revisit trigger: if first-party HA ever becomes a goal (for example a hosted Reverie offering), this stance gets a
  superseding record; it is not amended by exception.
