---
type: REQ
profile-version: 1
id: "REV-REQ-0076"
title: "A fresh Library visit starts without filters"
governed-by:
  - "REV-ADR-0066"
---

# A fresh Library visit starts without filters

## Statement

WHEN a navigation starts a fresh Library visit, as
[REV-REQ-0075](./0075-a-resumed-library-visit-restores-its-filter-state.md) defines a visit, the Library MUST start that
visit with no filter conditions, whatever filters an earlier visit held.

## Rationale

Library filters describe a reader's current visit, not a standing preference. A reader who opens the Library afresh,
possibly days later, expects the whole collection, and conditions left over from an earlier visit would hide books they
never asked to hide. That holds even when the new visit reuses a page component the earlier visit left mounted.

## Acceptance criteria

- A fresh same-route entry, made while the Library page component remains mounted, starts without filters.
- A fresh direct visit starts without filters.
