---
type: ADR
profile-version: 1
id: "REV-ADR-0062"
title: "Uniform dismissal for library filter edits"
status: "accepted"
recorded-on: "2026-10-21"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0041"
---

# Uniform dismissal for library filter edits

## Context and problem statement

Library filters edit a live query through one URL-state owner. Valid typed values may wait briefly before reaching the
URL; incomplete numeric and date text stays in the editor until validation. The filter drawer uses React Aria, whose
close callback reports whether the overlay is open without identifying the dismissal gesture.

Giving Escape a cancellation meaning while the close button and scrim retain edits requires application keyboard
handling and a separate rollback path. Is that distinction worth maintaining alongside the native overlay lifecycle?

## Decision drivers

- Supported React Aria interactions reduce application-owned keyboard and focus code.
- Filter changes have one owner, including pending writes and explicit clears.
- Dismissal should have a consistent effect across keyboard and pointer input.
- The drawer and removable chips serve both library projections without reserving a permanent rail column.

## Considered options

- **Uniform dismissal retaining valid edits.** Escape, the close button and the scrim close the drawer while valid edits
  settle through the existing URL-state owner.
- **Escape-only cancellation.** Escape rolls pending edits back; pointer dismissal retains them. This offers a quick
  cancellation gesture but needs application handling to distinguish it from other closes.
- **Explicit Apply and Cancel.** A separate draft transaction makes cancellation visible, but changes the live-filter
  workflow and introduces another state lifetime.

## Decision outcome

Chosen option: **uniform dismissal retaining valid edits**, because it fits the live-query model and supported native
closure without an application keyboard adapter or rollback path.

Filters remain in a right-side drawer at every width, with removable chips and clear-all outside it. Search stays in the
shared toolbar. Filter and book-detail drawers share one overlay slot. Native dismissal returns focus to the opener.
Valid edits retain their existing write timing; numeric and date inputs keep their blur and Enter validation. Closing is
not an explicit Apply operation or a new persistence boundary for incomplete text.

### Consequences

- Positive: keyboard and pointer dismissal share one outcome and native overlay handling.
- Positive: pending writes and explicit clears remain with the existing filter owner.
- Positive: removable chips retain one-click removal without a permanent rail column.
- Negative: Escape cannot undo a filter edit; the reader clears or changes the condition explicitly.
- Negative: the shared overlay slot allows only one of the filter and book-detail drawers to be open.

## More information

React Aria's [Modal documentation](https://react-aria.adobe.com/Modal) describes native dismissal and controlled open
state. The native interaction foundation is recorded in REV-ADR-0060.
