---
type: ADR
profile-version: 1
id: "REV-ADR-0039"
title: "Single filter home in the library right rail"
status: "superseded"
recorded-on: "2026-09-05"
decided-on: "2026-07-10"
decision-makers:
  - "John Unkovich"
superseded-by:
  - "REV-ADR-0041"
---

# Single filter home in the library right rail

## Context and problem statement

The library page rendered three view modes (grid, list, table) over one URL-driven filter and sort contract: the
typed filter grammar and the multi-column sort stack. The editing surfaces for that contract had grown up per view
mode. The table view carried a toolbar (quick search, a popover condition builder, a chip row) and a sort-chip bar;
the grid and list views carried masthead filter chips that recognised only a fraction of the grammar, a sort-preset
menu, and a shelf picker; a right-hand filter rail offered facet checkboxes in grid and list only.

The same URL state therefore presented differently in each mode. A condition set in the table view rendered no chip
in grid or list, so switching modes narrowed the visible collection by conditions the user could no longer see or
remove individually. The shelf filter had no surface at all in table mode. Each new filterable field would have to
be wired into several surfaces to stay consistent, and the surfaces had already drifted.

Where should filter and sort editing live, and how does active filter state stay visible in every view mode,
including when the editing surface is hidden?

## Decision drivers

- **One editing surface for the whole grammar.** Every filterable field, facet or typed condition, should be edited
  in one place that behaves the same in all three view modes, so surfaces cannot drift per mode again.
- **State visibility survives hiding the editor.** Active filters must stay visible and recoverable in every mode,
  including when the editing surface is collapsed. An invisibly narrowed collection misrepresents the library.
- **Room for the full grammar.** The filter surface spans facets, text operators, numeric and date ranges, status,
  rating, the tag, genre, mood, and author families with per-family any/all/none match modes, and a reorderable
  sort stack. A single toolbar or chip row does not scale to that breadth; a vertical column of collapsible
  sections does.
- **Keep per-family match modes.** The per-family any/all/none grammar is more expressive than a global AND/OR/NOT
  toggle and stays as is.
- **Compact masthead.** The masthead should carry orientation and view switching, not a growing row of filter
  chrome.

## Considered options

- **Toolbar convergence.** Promote the table view's toolbar (quick search, popover condition builder, chip row) to
  all view modes and make it the filter home.
- **Split rail/masthead model.** The rail becomes the sole editor; the masthead renders a chip row mirroring active
  conditions, each chip removable in one click.
- **Rail owns editing and state; masthead summarises.** The rail is the sole editor and the primary display of
  active state; the masthead carries a compact, always-visible, read-only summary plus a rail toggle.

## Decision outcome

Chosen option: **Rail owns editing and state; masthead summarises**, because it was the only option that gave the
full grammar one scalable home while keeping active state visible in every mode without duplicating an editing or
removal surface. The known cost was accepted: removing a single condition took two clicks (open the rail, untick or
clear the section) instead of one click on a chip.

The shape of the decision:

- **The filter rail was the sole filter editing surface in all view modes.** It carried a quick-search input at the
  top (writing the same quick-search filter the toolbar offered), one collapsible section per filterable field: the
  existing facet sections (series and author) plus inline section editors for the typed grammar (page-count range,
  rating range, added-date range, status, tags, genres, moods, and title, subtitle, and ISBN text operators), reusing
  the per-column editors the table view already shipped. Per-family any/all/none match modes stayed. Active values
  were highlighted in their section, and each section had a clear affordance.
- **The rail gained a sort section.** The multi-sort stack (add, remove, reorder, direction) was edited in the rail.
  The table view kept header click and ctrl-click as the primary sort gesture; the rail section was the sort home
  for grid and list and the full-stack editor everywhere. Table-header sorting was the one deliberate exception to
  the rail's ownership of edit gestures.
- **The masthead carried a compact always-visible summary.** A short active-filter readout (for example
  "Author (1) · Pages ≥ 300") plus a rail show/hide toggle with an active-filter badge, rendered in every view mode.
  The summary was read-only; editing happened in the rail.
- **The rail could fully collapse at desktop widths.** At the existing rail breakpoint (≥1280px) the rail collapsed;
  the masthead summary and badge kept the state visible and one click from editable. Below the breakpoint the
  existing sheet pattern was unchanged.
- **The quick-search input replaced the rail's command-palette trigger.** The command palette stayed reachable
  through its global shortcut and was a navigation surface, not a filtering one.
- **Shelf became a rail facet section.** This also gave the table view a shelf surface, which it never had.
- **Superseded components were deleted, not kept in parallel.** The table toolbar (quick search, popover builder,
  chip row), the sort-chip bar, the masthead sort-preset menu, the masthead filter chips, and the masthead shelf
  picker were all removed. The "(all editions)" suffix on work-scoped table column headers moved into a header
  tooltip.
- **Facet counts stayed deferred.** Sections shipped without counts until the aggregation endpoints existed; the
  rail structure did not wait for them.

### Consequences

- Positive: one surface knew the whole grammar: a new filterable field was wired once, and every view mode got it
  at once.
- Positive: active state survived every mode and the collapsed rail; the summary and badge were rendered
  unconditionally, so no condition could narrow the collection invisibly.
- Positive: the masthead and table chrome shrank: five components (toolbar, sort-chip bar, sort-preset menu, filter
  chips, shelf picker) collapsed into one rail and a one-line summary.
- Positive: the table view gained the shelf filter and the grid and list views gained the typed grammar, closing
  the per-mode capability gaps.
- Negative: removing a single condition took two clicks instead of one, which deviated from the prevailing
  faceted-browse convention of individually removable applied-filter tokens. The trade was made knowingly; a chip
  row that could carry the full grammar would have crowded the masthead it was meant to keep compact.
- Negative: the rail got long. Collapsible sections, in-section highlighting, and per-section clear affordances
  were load-bearing for it to stay navigable, not polish.
- Negative: collapsing the rail also put quick search and the grid and list sort controls one toggle click away;
  the masthead summary kept state visible but was not an input surface.
- Neutral: the URL contract was untouched: this decision moved surfaces, not grammar. Deep links and cursors
  behaved exactly as before.

## Pros and cons of the options

### Toolbar convergence

- Positive: chips gave one-click removal per condition and the toolbar pattern was familiar from spreadsheet-style
  data grids.
- Negative: the toolbar was designed against the table view; stretched across grid and list it stacks a second
  chrome row above the collection in exactly the modes meant to stay immersive.
- Negative: it does not resolve the split: the rail still exists for facets, so the grammar keeps two homes and the
  drift this decision is meant to end continues.
- Negative: a chip row carrying the full typed grammar wraps to several lines on a moderately filtered view, and the
  popover builder nests editors inside a transient surface with no room for a sort stack.

### Split rail/masthead model

- Positive: it keeps a single editor while preserving one-click chip removal.
- Negative: the chip row duplicates state the rail already displays, and the two surfaces must stay visually
  synchronised: every rail section needs a chip renderer, which reintroduces the per-surface wiring cost.
- Negative: the full grammar as chips crowds the masthead: text operators, ranges, dates, status, rating, and the
  tag, genre, mood, and author families produce a chip row that competes with the collection for vertical space,
  against the compact-masthead driver.

### Rail owns editing and state; masthead summarises

- Positive: the rail was the single home: one wiring point per field, identical behaviour in all modes, and a
  vertical column of collapsible sections scales to the grammar where a horizontal row cannot.
- Positive: the read-only summary was constant-size: it never grew past one line regardless of how many conditions
  were active.
- Negative: removal cost two clicks; the summary showed that state existed but did not edit it.

## More information

- [Typed filter grammar on list endpoints](./0038-typed-filter-grammar-on-list-endpoints.md): the URL grammar this
  surface edited. Unchanged by this decision. Not superseded.
- [Multi-column sort stack on the keyset list contract](./0037-multi-column-sort-stack-on-the-keyset-list-contract.md):
  the sort semantics behind the rail's sort section. Unchanged by this decision. Not superseded.
- Revisit trigger: if two-click removal proved a recurring friction point in real use, an amending ADR would add
  per-condition removal affordances to the masthead summary rather than reintroducing a chip row wholesale.
