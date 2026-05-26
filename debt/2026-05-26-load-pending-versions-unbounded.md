---
status: active
severity: medium
surfaces: [developer, server-operator]
adopted: 2026-05-24
adopted-because: "11c (PR #316) ships load_pending_versions without a LIMIT clause; scoping the slice to metadata edit flow took priority over pagination of a rarely-large result set"
lift-when-class: internal-refactor
lift-when: PR adds `LIMIT 200` (or cursor pagination) to load_pending_versions query in backend/src/routes/metadata.rs; same PR adds a test asserting the cap
lifted: ~
superseded-by: ~
---

# `load_pending_versions` query has no row limit

## Constraint

`load_pending_versions` in the metadata review flow fetches all
pending metadata versions for a given book with no `LIMIT`. In normal
operation the count is low (single digits), but a bulk enrichment run
or repeated manual edits could accumulate hundreds of pending versions
for one book, producing an unbounded result set.

## Workaround

No workaround applied — the query runs unbounded. Accepted because
the 11c slice needed to ship the metadata edit flow, and the pending
version count is practically small for all current usage patterns.

## Lift trigger

Add `LIMIT 200` (or proper cursor pagination) to the query. If the
count can realistically exceed 200, implement keyset pagination
matching the library list pattern.
