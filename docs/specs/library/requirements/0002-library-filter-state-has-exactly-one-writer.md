---
type: REQ
profile-version: 1
id: "REV-REQ-0002"
title: "Library filter state has exactly one writer"
governed-by:
  - "REV-ADR-0066"
---

# Library filter state has exactly one writer

## Statement

Library filter state MUST have exactly one owner for each visit. Every editing surface MUST dispatch published filter
intent to that owner, and reload or history traversal MUST restore the destination visit's published state. A fresh
Library entry MUST start clean, including navigation to the same route while its component remains mounted. Filter edits
MUST NOT add history entries.

## Rationale

Independent writers can overwrite sibling edits or restore a departed visit into a fresh destination. A reader returning
from a book depends on recovering the context they left, while a fresh visit needs no stale narrowing conditions.

## Acceptance criteria

- Quick search, rail sections, chip removal and all clear affordances dispatch to the same page-level owner.
- Same-turn sibling edits both survive.
- Blur followed by Clear cannot resurrect the cleared condition.
- Reload after rapid edits restores their final published state before the first list request.
- Library to book to Back restores filters, chips and results.
- Forward restores its destination.
- Back past Library leaves the route without stepping through filter edits.
- A fresh same-route entry and a fresh direct visit start without filters.
- Traversing to an earlier entry restores that entry, including when the page component is reused.
- An absent or malformed owned entry payload starts a clean visit as a unit, preserving unrelated entry data without
  adding recovery UI or a separate recovery write.
- Clear removes every filter condition while preserving view and sort.
- Clearing one section preserves sibling sections.
- Raw text and the selected operator remain editable when they contribute no effective predicate.
- Native unfinished input remains with its control until that control publishes a value.
