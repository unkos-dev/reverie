---
status: "accepted"
date: 2026-07-10
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Single filter home in the library right rail

## Context and Problem Statement

The library page renders three view modes (grid, list, table) over one
URL-driven filter and sort contract: the typed filter grammar and the
multi-column sort stack. The editing surfaces for that contract grew up
per view mode. The table view carries a toolbar (quick search, a popover
condition builder, a chip row) and a sort-chip bar; the grid and list views
carry masthead filter chips that recognise only a fraction of the grammar,
a sort-preset menu, and a shelf picker; a right-hand filter rail offers
facet checkboxes in grid and list only.

The same URL state therefore presents differently in each mode. A condition
set in the table view renders no chip in grid or list, so switching modes
narrows the visible collection by conditions the user can no longer see or
remove individually. The shelf filter has no surface at all in table mode.
Each new filterable field would have to be wired into several surfaces to
stay consistent, and the surfaces have already drifted.

Where should filter and sort editing live, and how does active filter state
stay visible in every view mode, including when the editing surface is
hidden?

## Decision Drivers

- **One editing surface for the whole grammar.** Every filterable field,
  facet or typed condition, should be edited in one place that behaves the
  same in all three view modes, so surfaces cannot drift per mode again.
- **State visibility survives hiding the editor.** Active filters must stay
  visible and recoverable in every mode, including when the editing surface
  is collapsed. An invisibly narrowed collection misrepresents the library.
- **Room for the full grammar.** The filter surface spans facets, text
  operators, numeric and date ranges, status, rating, three vocabulary
  families with any/all/none match modes, and a reorderable sort stack.
  A single toolbar or chip row does not scale to that breadth; a vertical
  column of collapsible sections does.
- **Keep per-family match modes.** The per-family any/all/none grammar is
  more expressive than a global AND/OR/NOT toggle and stays as is.
- **Compact masthead.** The masthead should carry orientation and view
  switching, not a growing row of filter chrome.

## Considered Options

- **A: Toolbar convergence.** Promote the table view's toolbar (quick
  search, popover condition builder, chip row) to all view modes and make
  it the filter home.
- **B: Split rail/masthead model.** The rail becomes the sole editor; the
  masthead renders a chip row mirroring active conditions, each chip
  removable in one click.
- **C: Rail owns editing and state; masthead summarises.** The rail is the
  sole editor and the primary display of active state; the masthead carries
  a compact, always-visible, read-only summary plus a rail toggle.

## Decision Outcome

Chosen option: **C**, because it is the only option that gives the full
grammar one scalable home while keeping active state visible in every mode
without duplicating an editing or removal surface. The known cost is
accepted: removing a single condition takes two clicks (open the rail,
untick or clear the section) instead of one click on a chip.

The shape of the decision:

- **The filter rail is the sole filter editing surface in all view modes.**
  It carries a quick-search input at the top (writing the same quick-search
  filter the toolbar offered), one collapsible section per filterable
  field: the existing facets plus inline section editors for the typed
  grammar (page-count range, rating range, added-date range, status, tags,
  genres, moods, and title, subtitle, and ISBN text operators), reusing the
  per-column editors the table view already ships. Per-family any/all/none
  match modes stay. Active values are highlighted in their section, and
  each section has a clear affordance.
- **The rail gains a sort section.** The multi-sort stack (add, remove,
  reorder, direction) is edited in the rail. The table view keeps header
  click and ctrl-click as the primary sort gesture; the rail section is the
  sort home for grid and list and the full-stack editor everywhere.
- **The masthead carries a compact always-visible summary.** A short
  active-filter readout (for example "Author (1) · Pages ≥ 300") plus a
  rail show/hide toggle with an active-filter badge, rendered in every view
  mode. The summary is read-only; editing happens in the rail.
- **The rail can fully collapse at desktop widths.** At the existing rail
  breakpoint (≥1280px) the rail collapses; the masthead summary and badge
  keep the state visible and one click from editable. Below the breakpoint
  the existing sheet pattern is unchanged.
- **The quick-search input replaces the rail's command-palette trigger.**
  The command palette stays reachable through its global shortcut and is a
  navigation surface, not a filtering one.
- **Shelf becomes a rail facet section.** This also gives the table view a
  shelf surface, which it never had.
- **Superseded components are deleted, not kept in parallel.** The table
  toolbar (quick search, popover builder, chip row), the sort-chip bar, the
  masthead sort-preset menu, the masthead filter chips, and the masthead
  shelf picker are all removed. The "(all editions)" suffix on work-scoped
  table column headers moves into a header tooltip.
- **Facet counts stay deferred.** Sections ship without counts until the
  aggregation endpoints exist; the rail structure does not wait for them.

### Consequences

- Good, because one surface knows the whole grammar: a new filterable field
  is wired once, and every view mode gets it at once.
- Good, because active state survives every mode and the collapsed rail;
  the summary and badge are rendered unconditionally, so no condition can
  narrow the collection invisibly.
- Good, because the masthead and table chrome shrink: five components
  (toolbar, sort-chip bar, sort-preset menu, filter chips, shelf picker)
  collapse into one rail and a one-line summary.
- Good, because the table view gains the shelf filter and the grid and list
  views gain the typed grammar, closing the per-mode capability gaps.
- Bad, because removing a single condition takes two clicks instead of one,
  which deviates from the prevailing faceted-browse convention of
  individually removable applied-filter tokens. The trade was made
  knowingly; a chip row that could carry the full grammar would crowd the
  masthead it was meant to keep compact.
- Bad, because the rail gets long. Collapsible sections, in-section
  highlighting, and per-section clear affordances are load-bearing for it
  to stay navigable, not polish.
- Bad, because collapsing the rail also puts quick search and the grid and
  list sort controls one toggle click away; the masthead summary keeps
  state visible but is not an input surface.
- Neutral, because the URL contract is untouched: this decision moves
  surfaces, not grammar. Deep links and cursors behave exactly as before.

### Confirmation

Filter and sort URL parameters are written only from the rail and from
table-header sort gestures; the masthead summary is read-only. No toolbar
or masthead component edits filter state, and the deleted components do not
reappear under new names.

## Pros and Cons of the Options

### A: toolbar convergence

- Good, because chips give one-click removal per condition and the toolbar
  pattern is familiar from spreadsheet-style data grids.
- Bad, because the toolbar was designed against the table view; stretched
  across grid and list it stacks a second chrome row above the collection
  in exactly the modes meant to stay immersive.
- Bad, because it does not resolve the split: the rail still exists for
  facets, so the grammar keeps two homes and the drift this decision is
  meant to end continues.
- Bad, because a chip row carrying the full typed grammar wraps to several
  lines on a moderately filtered view, and the popover builder nests
  editors inside a transient surface with no room for a sort stack.

### B: split rail/masthead model

- Good, because it keeps a single editor while preserving one-click chip
  removal.
- Bad, because the chip row duplicates state the rail already displays, and
  the two surfaces must stay visually synchronised: every rail section
  needs a chip renderer, which reintroduces the per-surface wiring cost.
- Bad, because the full grammar as chips crowds the masthead: text
  operators, ranges, dates, status, rating, and three vocabulary families
  produce a chip row that competes with the collection for vertical space,
  against the compact-masthead driver.

### C: rail owns editing and state; masthead summarises

- Good, because the rail is the single home: one wiring point per field,
  identical behaviour in all modes, and a vertical column of collapsible
  sections scales to the grammar where a horizontal row cannot.
- Good, because the read-only summary is constant-size: it never grows past
  one line regardless of how many conditions are active.
- Bad, because removal costs two clicks; the summary shows that state
  exists but does not edit it.

## More Information

- [Typed filter grammar on list endpoints](2026-07-07-typed-filter-grammar-list-endpoints.md):
  the URL grammar this surface edits. Unchanged by this decision. Not
  superseded.
- [Multi-column sort stack on the keyset list contract](2026-07-07-multi-column-sort-stack.md):
  the sort semantics behind the rail's sort section. Unchanged by this
  decision. Not superseded.
- Revisit trigger: if two-click removal proves a recurring friction point
  in real use, an amending ADR adds per-condition removal affordances to
  the masthead summary rather than reintroducing a chip row wholesale.
