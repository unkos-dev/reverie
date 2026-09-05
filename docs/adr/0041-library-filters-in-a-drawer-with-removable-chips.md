---
type: ADR
profile-version: 1
id: "REV-ADR-0041"
title: "Library filters in a drawer with removable chips"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-07-14"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0039"
---

# Library filters in a drawer with removable chips

## Context and problem statement

The library page held one filter home in a persistent right rail: the rail was the sole editor and display of
active filter state, and the masthead carried only a compact, read-only summary of it. Removing a single condition
took two clicks (open the rail, then untick or clear the section), and the rail column consumed width at every
screen size, whether or not it was in use.

The library was separately being redesigned onto one route with two projections, a cover grid and a table, sharing
one query so that filters, search, and sort apply identically to both. That redesign needed a filter surface that
belongs to the shared query rather than to either projection, and that does not cost the collection a permanent
column of width. Where should filter editing and active-filter display live under the merged route, and how does a
user remove one active condition without a fixed multi-click cost?

## Decision drivers

- Removing one active filter condition should cost one click, not the two clicks the rail's edit-then-clear model
  required.
- The filter surface must work identically for the cover grid and the table, since both now render from the same
  query rather than per-view-mode state.
- A persistent rail column claims layout width at every screen size regardless of whether filtering is in use.
- Search is not filter-specific; it belongs in a toolbar shared by both projections rather than inside the filter
  editing surface.
- The filter surface and the book-detail surface each need a right-side overlay; introducing two independent
  overlay mechanisms would duplicate scrim, escape, and focus-return handling.

## Considered options

- **Filters in a right-side drawer with removable chips.** Active conditions render as a chip row with per-chip
  removal and a clear-all affordance; editing moves into a drawer that opens on demand at every width.
- **Single filter home in the library right rail.** The rail stays the sole editor and display of active state at
  every width, with a read-only masthead summary (the superseded design; see REV-ADR-0039).

## Decision outcome

Chosen option: **filters in a right-side drawer with removable chips**, because it gives one-click removal per
condition, stops charging the collection a permanent column of width, and works identically for the cover grid and
the table since it is keyed to the one query both projections share.

Active filters render as chips built from the same summary projection the rail's read-only masthead summary used,
except each chip now carries its own removal patch, plus a clear-all affordance. The filter drawer renders at every
width rather than only below a breakpoint, and it shares one overlay slot with the book-detail drawer opened from
the table's details column: the two are mutually exclusive by construction, share one scrim, share Escape handling,
and return focus to whichever control opened them. Closing the drawer distinguishes how it closed: Escape abandons
any pending filter drafts, while the scrim or the close button applies them. Search moves out of the filter surface
entirely into a view-neutral toolbar shared by both projections, alongside the grid/table switcher and the Filters
trigger; the view switcher changes which projection renders, never the underlying query.

### Consequences

- Positive: removing one active filter condition now costs one click on its chip, reversing the two-click cost
  accepted under the superseded rail model.
- Positive: the library page no longer reserves a permanent rail column; the collection has the full width at every
  size, and the drawer only occupies space while open.
- Positive: the filter drawer and the book-detail drawer reuse one overlay slot, so the redesign needed only one set
  of scrim, Escape, and focus-return handling rather than two.
- Negative: because the filter drawer and the book-detail drawer share one overlay slot, opening one closes the
  other; the two surfaces cannot be open at the same time.

## Pros and cons of the options

### Filters in a right-side drawer with removable chips

- Positive: chips give one-click removal per condition.
- Positive: the drawer works identically for the cover grid and the table, since both share the query it edits.
- Neutral: sharing an overlay slot with the book-detail drawer means the two are mutually exclusive by construction.

### Single filter home in the library right rail

- Positive: the rail gave one editing surface for the whole filter grammar, visible at every width.
- Negative: removing a single condition took two clicks.
- Negative: the rail column consumed layout width at every screen size, whether or not it was in use.
