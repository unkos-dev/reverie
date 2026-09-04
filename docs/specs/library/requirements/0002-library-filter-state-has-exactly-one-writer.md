---
type: REQ
profile-version: 1
id: "REV-REQ-0002"
title: "Library filter state has exactly one writer"
governed-by:
  - "REV-ADR-0001"
---

# Library filter state has exactly one writer

## Statement

The library filter state (the `q`, vocabulary, text, range, date, status, series, and shelf conditions carried in the
route's URL search parameters) MUST have exactly one writer. Every editing surface on the library route MUST dispatch
its intent to that writer, and a component on the library route MUST NOT read or write a library filter parameter by
any other path.

## Rationale

A shared URL parameter set with more than one independent writer lets two surfaces build on stale copies of the same
key and clobber each other's edits, which is the hazard the project's shared-mutable-state rule exists to close. Naming a
single writer removes the possibility of a read split or a lost write by construction and confines cross-surface
coordination to that one component. The mechanism that makes a second reader unsafe here is described in the Design
that satisfies this Requirement.

## Acceptance criteria

- Every filter-editing component under the library route (the filter rail's per-field sections, the quick-search input,
  the filter chips, and the clear-all and clear-filters affordances) changes filter state by calling a commit function
  supplied by the one filter writer; none writes a filter key through a `URLSearchParams` instance it constructs itself.
- No component under the library route calls React Router's `useSearchParams` to read or write a library filter key.
- A gesture that edits one filter slice (for example the pages range) writes only that slice's URL keys, so a sibling
  slice mid-edit is never overwritten by a stale snapshot of its current value.
- An immediate write (a clear affordance) cancels a delayed write already queued on the same keys, so a queued keystroke
  cannot resurrect a condition the clear removed.
- The clear-all affordance removes every filter key the codec recognises, including a key that no longer carries a wire
  predicate, so no filter condition is left unclearable; checked by comparing the URL after clear-all against the
  codec's full key list.

## More information

- [Single filter home in the library right rail](../../../../adr/2026-07-10-library-filter-home-right-rail.md): the
  decision that the filter rail is the primary editing surface.
- Satisfied by [Library filter and sort state](../design/0001-library-filter-and-sort-state.md).
