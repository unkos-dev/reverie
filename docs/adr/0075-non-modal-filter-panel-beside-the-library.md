---
type: ADR
profile-version: 1
id: "REV-ADR-0075"
title: "Non-modal filter panel beside the library"
status: "accepted"
recorded-on: "2026-11-13"
decided-on: "2026-11-14"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0062"
---

# Non-modal filter panel beside the library

## Context and problem statement

Library filters edit a live query through one URL-state owner. The filter editor is a modal drawer: it covers part of
the collection, blocks the table behind a scrim, and closes on Escape, the close button or a press outside it.

Curation alternates between the two surfaces. A curator narrows the collection, corrects cells in the rows that remain,
then narrows again. The modal drawer makes each round a close and a reopen, and it hides the right-hand columns while
the filters change. Should the filter editor stay modal, or should it stay open beside the collection?

## Decision drivers

- A filter change is most useful when the curator can watch the collection respond to it.
- Inline cell editing and filtering are used together, so neither should block the other.
- Dismissal should have one outcome across keyboard and pointer input.
- Filter changes keep one owner, including pending writes and explicit clears.
- The filter editor should not charge the collection a column of width while filtering is not in use.
- Narrow screens have no room for a panel beside the collection.

## Considered options

- **Non-modal panel beside the collection.** The panel opens from the Filters control and the collection narrows to make
  room. The panel stays open until the curator closes it. Narrow screens keep a modal overlay.
- **Modal drawer at every width.** The current behaviour: the drawer overlays the collection and any outside press
  closes it.
- **Permanent filter rail.** The filter editor is always visible and always takes a column.

## Decision outcome

Chosen option: **non-modal panel beside the collection**, because it lets filtering and inline editing run together
while every column stays visible, and it takes width only while the panel is open.

Where the panel fits beside the collection, it has no scrim and does not trap focus. A press outside the panel does not
close it. The Filters control and the panel's own close control toggle it, and closing from inside the panel returns
focus to the Filters control. Where the panel does not fit, the filter editor is a modal overlay, and Escape, the close
control and an outside press all close it.

Dismissal keeps one outcome at every width: valid edits stay applied. Valid edits keep their write timing through the
existing filter owner, and numeric and date inputs keep their blur and Enter validation. Closing is not an Apply
operation and is not a persistence boundary for incomplete text. Removable chips and clear-all stay outside the panel.

### Consequences

- Positive: the curator filters and edits cells without closing and reopening the filter editor.
- Positive: no column is hidden while filters change.
- Positive: the non-modal panel needs no overlay, scrim or focus trap.
- Negative: the filter editor has two presentations, non-modal and modal, that must stay behaviourally aligned.
- Negative: the open panel narrows the collection, so fewer or narrower columns fit.
- Negative: Escape cannot undo a filter edit; the curator clears or changes the condition explicitly.
- Negative: the open panel is one more region in the keyboard order between the toolbar and the collection.

## More information

The width at which the filter editor changes presentation is a layout detail for the Design that describes the library
filter surface. The native interaction foundation is recorded in REV-ADR-0060.
