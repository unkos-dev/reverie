---
type: ADR
profile-version: 1
id: "REV-ADR-0057"
title: "Supervise background workers with task-supervisor"
status: "proposed"
recorded-on: "2026-10-15"
decision-makers:
  - "John Unkovich"
---

# Supervise background workers with task-supervisor

## Context and problem statement

Settings reload, ingestion, enrichment, session expiry and writeback depend on long-lived Tokio workers. A worker
failure can leave HTTP serving while background work has stopped. Recovery needs to distinguish a stopped worker from an
individual job failure or a dependency retry already scheduled by the worker.

## Decision drivers

- Restart failed workers in-process with exponential backoff and a bounded per-worker restart window.
- Escalate an exhausted worker to process failure so the container restart policy can recover the instance.
- Share one shutdown mechanism across workers, including the ingestion filesystem watcher.
- Keep restart scheduling, panic handling and drain deadlines in a maintained library with a small dependency surface.

## Considered options

- Supervise background workers with task-supervisor.
- Maintain a first-party supervisor over Tokio task primitives.
- Exit the process on every worker failure and rely entirely on container restarts.

## Decision outcome

Chosen option: **supervise background workers with task-supervisor**, because its per-task sliding restart window,
exponential backoff, panic handling, dead-task threshold and cooperative stop timeout fit the required recovery model.
The crate owns those mechanisms; Reverie owns worker construction, persistent command receivers, unexpected-success
classification and process-error propagation.

One supervisor manages the five service workers. Each task allows fifteen restarts within a thirty-minute sliding
window; its sixteenth failure in that window marks it dead. Backoff starts at one second, doubles to a sixty-second cap
and uses the crate's default ten percent random delay reduction. The fifteen delays total 603 seconds before that
reduction. Instant continuous failure therefore exhausts the limit after about nine to ten minutes. Thirty-second failed
attempts add sixteen times thirty seconds, giving about seventeen to eighteen minutes. Both cases remain inside the
accounting window. A dead-task threshold of zero escalates any dead worker. Cancellation allows thirty seconds for each
run to drain concurrently. Individual job retries and ingestion dependency probes remain with their existing scheduling
owners.

The coordinator owns its filesystem watcher on the same run token. It registers the watch after its folder check passes
and before discovery. Watcher failure repeats that folder check: a failed check uses the existing dependency pause and
probe schedule, while a successful check returns the unexplained watcher failure to supervision. Recovery registers the
watch before discovery so files arriving during downtime are found.

### Consequences

- Positive: a worker error, panic or unexpected successful return triggers the same recovery policy.
- Positive: one worker exhausting its restart limit returns an error through the server entry point to a non-zero exit.
- Positive: the dependency uses `tokio`, `tokio-util`, `thiserror` and `tracing`, all already present in the dependency
  graph.
- Negative: a prolonged database outage can exhaust the settings listener and restart the whole process.
- Negative: cancellation drops a run that exceeds its stop timeout; it cannot stop a blocking operating-system call.
- Negative: integration must preserve receivers and drain admitted work before a restarted worker can reclaim it.

## Pros and cons of the options

### Maintain a first-party supervisor

- Positive: lifecycle semantics can be tailored directly to each worker.
- Negative: the application owns restart-window accounting, panic capture, backoff and shutdown races as well as jobs.

### Rely entirely on container restarts

- Positive: application lifecycle code is smaller.
- Negative: every isolated worker failure interrupts HTTP and all other workers, even when an in-process retry succeeds.

## More information

The [task-supervisor source and documentation](https://github.com/akhercha/task-supervisor) expose the lifecycle and
configuration directly. Version 0.5.2 fixes restart-limit accounting and shutdown acknowledgements; its test suite
covers lifecycle, thresholds, handle ownership and shutdown. Maintaining this integration includes reviewing those
semantics on updates. The crate is MIT licensed, resolves through crates.io and adds no new transitive packages to
Reverie's lockfile.
