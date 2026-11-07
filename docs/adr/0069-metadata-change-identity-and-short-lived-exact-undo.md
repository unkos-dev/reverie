---
type: ADR
profile-version: 1
id: "REV-ADR-0069"
title: "Metadata change identity and short-lived exact Undo"
status: "accepted"
recorded-on: "2026-11-07"
decided-on: "2026-11-07"
decision-makers:
  - "John Unkovich"
---

# Metadata change identity and short-lived exact Undo

## Context and problem statement

Catalogue metadata combines shared work fields, edition fields and mutable source observations. Equal displayed values
do not establish that a field has stayed untouched: it can change away and back, or acquire different attribution.
Likewise, a source observation's identifier can survive changes to the observation itself.

Selective Undo needs a different answer from request freshness. A request can be stale because another field changed,
while the selected field remains safe to restore. The choice concerns how to distinguish those cases and retain enough
evidence for exact restoration without creating a permanent history feature.

## Decision drivers

- Detect intervening changes across every canonical writer, including automation and relational changes.
- Preserve unrelated changes when restoring one field, including its attribution and ordered contributors.
- Keep normal editing immediate while preventing writes from an unrelated baseline.
- Resolve lost acknowledgements from committed evidence rather than apparent value equality.
- Bound retained evidence and reuse PostgreSQL's transaction and session ownership.

## Considered options

- Visible-value hashes with client counter-patches or historical source pointers.
- Application-maintained aggregate revisions with whole-save Undo.
- Database-maintained aggregate and field change identity with short-lived exact Undo.
- An immutable event log with permanent revision history.

## Decision outcome

Chosen option: **database-maintained aggregate and field change identity with short-lived exact Undo**, because it
distinguishes freshness from selected-field eligibility across all writers, while retaining exact restoration evidence
only for the editing context that needs it. Database ownership avoids relying on every application writer to remember an
additional invalidation call. Short-lived evidence fits a saved-edit Undo action without committing the product to a
history browser or general event replay.

### Consequences

- Positive: returning to an earlier value cannot revive an old editing baseline or Undo operation.
- Positive: unrelated fields can change without invalidating the selected field's restoration evidence.
- Positive: canonical changes and their evidence can share one database commit, including acknowledgement recovery.
- Negative: relational and attribution changes need trigger coverage, which adds database coupling and lock contention.
- Negative: exact restoration retains more state than a historical pointer, including sensitive draft values until
  expiry.
- Negative: expiry deliberately limits recovery. This choice provides neither permanent audit history nor multiple Undo
  steps for the same field.

## Pros and cons of the options

### Visible-value hashes with client counter-patches or historical source pointers

- Positive: hashes need no stored counters, and counter-patches can reuse the ordinary editing API.
- Negative: equal values conceal intervening changes. Mutable observations and incomplete previous pointers cannot
  establish an exact prior state.

### Application-maintained aggregate revisions with whole-save Undo

- Positive: revision updates remain beside application writes, and one aggregate comparison simplifies eligibility.
- Negative: missed writers retain stale identities. An unrelated change invalidates the entire save's Undo, even when
  restoring the selected field would preserve that change.

### An immutable event log with permanent revision history

- Positive: immutable events support audit, reconstruction and multi-step history beyond the current editing session.
- Negative: event retention, replay semantics and migration become permanent product concerns. Those costs exceed the
  bounded saved-edit recovery this decision addresses.
