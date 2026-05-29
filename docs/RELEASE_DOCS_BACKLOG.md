# Release Documentation Backlog

Operator- and user-facing documentation deferred until the relevant
product surface is built out. Each item names the decision or feature
whose rationale needs a proper Starlight page before a public release,
rather than a half-built page written ahead of the surface it documents.

This file is the holding area; items graduate into `docs/` Starlight
pages when their surface lands.

## Items

### `validation_status` operator semantics

**Source:** [`adr/2026-05-28-validation-status-vocabulary.md`](../adr/2026-05-28-validation-status-vocabulary.md) (UNK-276)

The `validation_status` enum is `pending | clean | repaired | degraded`.
The distinction is not self-evident to an operator reading the value:

- `pending` — the manifestation row exists but structural validation has
  not run yet.
- `clean` — validation found no issues.
- `repaired` — validation found issues that were automatically repaired;
  the file is ingested, stored, and served.
- `degraded` — validation found issues that are tolerated; the file is
  still served.

The load-bearing point operators need: `clean`, `repaired`, and
`degraded` are **all** stored-and-served outcomes on one quality tier —
`clean` means _no issues found_, not _the only valid state_. A
quarantined file is never represented here because quarantine deletes the
file and writes no row.

Write an operator-facing Starlight page covering these states (and how
quarantine differs) when the library/validation UI surface that exposes
them lands. The dev-facing reference in
[`docs/schema.md`](./schema.md) is already corrected.
