---
title: Navigating Reverie
description: "How the app shell is organised: the navigation rail, contextual filters, search, and the thinking behind them."
---

Every screen in Reverie wears the same chrome: a navigation rail on the left and, on browse surfaces, contextual filters
in a drawer on the right. This page describes how to move around and why the shell is shaped the way it is.

## Global rail, contextual drawer

Reverie separates **where you are** from **what you're looking at**:

- The **left rail** is global. It always shows the same destinations: Library, Shelves (with your shelves nested
  beneath), and, for administrators, the admin cluster, so wayfinding never depends on the current screen.
- **Filters** are contextual. On browse surfaces (the Library today) the toolbar's **Filters** button opens a drawer
  from the right edge carrying every filter for the books in view. Detail pages, admin screens, and the reader don't
  have one.

Keeping the two apart matters because the jobs are different: global navigation must be stable and learnable, while
filters must change with the content. Folding both into a single sidebar forces one of them to misbehave. The filter
drawer overlays the books rather than claiming a permanent column, so the collection keeps the full width while you
browse; the chips row under the toolbar (below) keeps active filters visible while the drawer is closed.

Your shelves live in the left rail under **Shelves**, capped at seven rows with an "All shelves" overflow link; shelves
are destinations you return to, not filters you apply.

## Planned entries are visible

The rail shows a few destinations that aren't built yet; Home and Stats render with a visible **Planned** label. This is
deliberate: the rail communicates the product's intended shape, and a self-hosting operator evaluating Reverie can see
where it is going without reading a roadmap. Planned entries are skipped by keyboard navigation and never act as links.

Settings is not a rail destination: it lives in the user menu at the foot of the rail, and only administrators see it.
Everyone else sees no Settings entry at all. See [Instance settings](/reverie/guides/instance-settings/).

## Two search jobs, two surfaces

Reverie separates finding a book from narrowing the books in view.

The **command palette** navigates: pick a book and it takes you there. Open it with `⌘K` / `Ctrl K` while focus is in
the app shell. The shortcut does not open it from dialogs or popovers. The **search box** in the Library toolbar
filters: type two or more characters and the collection you are browsing shrinks to the matches, in whatever view and
sort order you already had. It never navigates, and clearing it brings everything back.

The split keeps each surface honest. A single box would have to guess whether you meant "go to this book" or "show me
fewer books", and the palette stays free to grow into a broader command surface without dragging filter semantics along.

## One query, two views

The Library shows the same collection two ways: a cover **grid** and a ledger-style **table**. The view toggle sits in
the toolbar. Grid is for recognising books by cover; the table is for managing them: scanning dense metadata, editing in
place, and acting on many books at once. Filters, search, and sort belong to the query, not the view, so switching views
never changes which books you're looking at, only how they're drawn.

The table carries a full keyboard model: arrow keys move cell by cell, `Home` and `End` jump within a row, `Ctrl Home`
and `Ctrl End` jump to the first and last loaded row, and `PageUp`/`PageDown` move a viewport at a time. Use the table's
**Help** control for the complete shortcut list.

Two toolbar controls shape the table to your taste. The **density toggle** switches between comfortable rows (with a
small cover beside each title) and compact ones. The **Columns** popover shows or hides individual columns; Title stays
put because every row needs its anchor. Both are personal display preferences, and so are your view choice and your
sort: they belong to your account rather than to one browser, so they follow you to a second machine and survive signing
out and back in. Filters and search belong to the current Library visit and do not travel in shared links. Column widths
are not saved: drag a header's edge to resize a column until the next reload, and titles and author lists wrap rather
than being cut off, so their row grows to fit.

**Reset to default** at the foot of the Columns popover clears your column choice rather than merely showing everything,
so the table goes back to whatever this installation ships as its default and stays with that default if it later
changes. It touches columns only: density, view, and sort keep whatever you chose. The control is greyed out when your
columns already match the default.

A preference set in one tab does not reach another tab that is already open; whichever you change last is what sticks,
and reloading the other tab catches it up.

Rows stream in as you scroll rather than loading all at once, which is why the table says "loaded": with a large library
the first screen appears immediately and the count grows as you go. A **Load more** button beneath the rows is the
keyboard-reachable fallback for when scrolling can't trigger loading, such as a viewport tall enough to show a whole
page without a scrollbar. Sorting happens on the server through a column header press (see below), so the order is
correct across the whole collection, not just the rows on screen.

Your current Library visit retains its view choice. Reverie also remembers that choice on your account for fresh Library
visits, on this machine or any other; a shared link carries no view override.

## Book details without leaving the list

Each table row starts with a details control; pressing it opens the book's details in a drawer from the right edge:
cover, facts, and real actions, **View full record** and **Add to shelf**. `Escape` closes it and puts focus back where
you were. The drawer exists so a triage pass over many books doesn't cost a round trip through the book page for each
one; the grid keeps its different habit, where a card click goes straight to the full record, because browsing covers
ends in reading, not managing. The filter drawer and the details drawer share the right edge and never stack: opening
one closes the other.

Closing the drawer does not cancel a submitted shelf addition. Reopening the same book shows any retained progress,
result or **Try again** action. This feedback is temporary and can expire five minutes after you close the drawer.

The full record's **Versions** tab restores retained progress or failures for **Accept**, **Reject** and **Clear** when
you return to the book. Returning does not repeat an action. A failed Clear keeps its error beside the field; use
**Clear** and confirm again to retry. Feedback is temporary and can expire five minutes after you leave.

## Selecting books and acting on them

Table rows carry checkboxes. Select a few (shift-click selects a range; `Shift Space` toggles the current row) and a bar
rises from the bottom with the count and the actions that exist today: **Add to shelf** and **Clear selection**. The
header checkbox selects the **loaded** rows, and says so: with a large library the rows still streaming in can't be part
of a selection, and a control that silently meant "everything matching the filter" would be worse than one that names
its limit. Changing the query (filters, search, sort) or switching views clears the selection, because acting on rows
you can no longer see is the kind of surprise a management surface exists to prevent.

A submitted batch keeps the books and shelf you chose. Its progress, result and **Retry failed books** action remain
available if you switch views or clear the selection. Retry sends only the books that could not be added.

## Sorting the library

The library always has an order. Before you have chosen one it is the order Reverie ships with, newest additions first,
and the Sort section shows exactly that, so what you see is always what is arranging the list. There is no "unsorted": a
sort gesture changes the order, never switches ordering off.

The filter drawer carries a **Sort** section in every view. Add up to three levels from its field picker, flip a level's
direction, reorder levels, or remove one; none of it needs a modifier key. The grid sorts from here, and the section
always shows the full stack, whichever surface built it. In the table, only the active sort shows a direction indicator
on its header, so a glance at the header row tells you what orders the list without decoding every column.

The table keeps its faster gesture on top. Clicking a header steps that column through its sort cycle: an unsorted
column sorts in its first direction, the next click reverses it, and the click after that returns to the library's
default order. A column that is already sorting continues from where it stands, so in the default order a click on Added
reverses it straight away. `⌘-click` / `Ctrl-click` a header to add it as another sort level instead of replacing the
current one, so you can sort by author and then, within each author, by newest first. On a column already in the sort,
`⌘-click` / `Ctrl-click` steps only that level, reversing or removing it. A plain click replaces the whole sort with
that one column, or returns to the default order when it ends that column's cycle; the drawer's Sort section is the
recovery surface when a stray click collapses a stack you built.

Books with nothing to sort on, no page count, no author yet, always sort to the end, whichever direction you're sorting
in.

The sort you choose sticks, on your account rather than on this browser: the library keeps that order until you sort
differently, here or on any other machine. **Reset** beside the Sort section's title takes you back to the order Reverie
ships with, and so does removing the stack's last level; the control only appears while you have a sort of your own to
remove.

Multi-level sort exists so that a large library can be scanned in exactly the order you want, author then most recent
within each author, for example, while paging stays stable: rows don't get skipped or repeated as you load more.

## Filtering the library

The filter drawer is the single home for every filter, in every view. Grid and table express the identical grammar,
because both use the same visit state. A condition set while browsing covers remains when you switch to the table, or
the other way around.

The drawer stacks one collapsible section per condition. Select several shelves or series to match any selection within
that section. Shelf and series conditions combine with each other and with the other filters. Their choices remain
available when the results narrow; each section has its own **Clear**. Authors, tags, genres, and moods each get a
typeahead that accepts several values at once, in any-of, all-of, or none-of mode, so you can ask for books tagged both
X and Y, in any of three genres, or in none of a set of moods. Reading status filters by state, including **Unread** for
books you have not started. Title offers contains, equals and does-not-equal; subtitle offers contains and is-empty.
ISBN-13 offers contains and is-empty. Page count and rating filter by range; **added** filters by a date range.

ISBN-13 narrows results as you type, without requiring a complete number or valid checksum. Spaces and hyphens do not
change a partial match: `978163`, `978-163` and `978 163` all find `9781639739134`. Enter a full ISBN through the same
contains control. Input containing only spaces and hyphens stays editable without an active ISBN-13 filter. Switching to
is-empty keeps your entered text for when you return to contains. Text changes publish immediately; the controls stay
usable while results update.

A section with something active opens on mount, shows a count beside its name, and carries its own **Clear**. Everything
else stays folded, so a long drawer reads as a table of contents rather than a wall of form controls.

Under the toolbar, in every view, each active condition renders as a **chip** ("Author: Le Guin", "Pages ≥ 300") with
its own remove control, plus a **Clear all**. Exclusions read as exclusions: a none-of condition shows as "Author: not
Le Guin", and a mixed set splits the count ("Tags (2, 1 not)"), so a chip never states the opposite of what the filter
does. The chips keep state visible while the drawer is closed, so the collection can never be silently narrower than it
looks, and dropping one condition is one click instead of a trip into the drawer.

Reloading or returning with Back or Forward restores that Library visit's filters and search before requesting books. A
fresh navigation to Library starts without filters. Edits add no history entries, so Back returns to the preceding page
rather than stepping through filter changes. Shared links and bookmarks carry no filter state.

## Editing from the table

Most cells in the table edit in place. Title, subtitle, ISBN-13, pages, and authors write to the book's canonical
metadata; reading status and rating write to your own reading state. Series stays read-only: no metadata field backs
that column, so it never opens an editor.

Press `Enter`, press `F2`, or start typing while a cell is selected to open its editor. `Enter` commits whatever you
typed; `Escape` discards it and leaves the cell as it was. `F2` only opens the editor here; it does not also close one
the way the WAI-ARIA grid pattern describes. The grid library Reverie is built on doesn't implement that half of the
toggle, so `Escape` or `Enter` are what close an editor regardless of how you opened it.

Title, subtitle, and authors describe the work rather than one edition of it, so editing any of them from one row
updates every edition of that work currently loaded in the table, not just the row you touched. ISBN-13 and pages
describe one edition and stay put. Status and rating are yours alone: nobody else browsing the same library sees them,
and they never touch the book's version history.

### Undo

The **Undo last edit** button beside table Help names the book and field it will restore. `⌘Z` or `Ctrl Z` while focus
is in the table invokes the same action when no editor is active. Editors retain their native typing Undo and Redo.

Undo restores the latest saved change to a metadata field while leaving unrelated changes intact. Saving the same field
again replaces its earlier Undo. Status and rating have no saved Undo, and the book view offers no saved-edit Undo.

Undo remains available through sorting, filtering and loading. It can restore a book hidden by the current filter; the
outcome names the book and field. Leaving Library or switching away from table editing clears the available edits.

### While a write is in flight

A cell shows what you typed the moment you commit, marked as busy until the server confirms it. On success the cell
settles on the value the server stored, which can differ slightly from what you typed: an ISBN-13 you enter with
hyphens, for example, comes back normalised. On failure the cell keeps its saved value. Local feedback above the results
identifies the book and field, shows the submitted value, and offers **Reload** to read the latest server value.
Feedback can remain visible when the table is replaced by grid, empty or loading results.

A refused ISBN-13 edit keeps the saved cell value and shows: "ISBN-13 wasn’t saved. Check the 13-digit number; spaces
and hyphens are allowed." Correct the number by editing the cell again.

## Inspecting and correcting ISBNs

The Book page and Edit metadata show this warning beside the saved ISBN pair when both numbers are individually valid
but do not correspond: "These ISBN-10 and ISBN-13 values do not correspond. Check both values." You can save a valid
mismatched pair. Unsaved typing does not change the warning; correcting or clearing either saved value removes it after
refresh.

Edit metadata shows invalid ISBN feedback beside the named field and retains your submitted values for correction. Only
edited fields are saved, and clearing a saved value requires confirmation. Each ISBN stays independent; a successful
formatted edit displays the compact number returned by the server.

While saving or checking a lost response, the fields are read-only. You can still select their text. After the request
settles, Save can retry the unchanged draft. A confirmed save closes the dialog. If the book changed elsewhere,
**Discard edits and reload** replaces the draft with the latest values.

## Cinematic mode

On the Library page, use **Cinematic view** at the bottom centre to dissolve the surrounding chrome; the navigation
rail, toolbar, and controls fade out so the cover art fills the screen. The toggle stays available as
**Exit cinematic view**. After two seconds of pointer stillness the cursor hides too; moving the pointer brings it back.

## Admin is a different room

Admin screens (Users, Ingestion) shift the canvas tone slightly darker. The chrome stays identical; only the surface
changes, signalling "operational room" rather than "reading room". The admin cluster in the rail is only rendered for
administrators; for everyone else it is structurally absent rather than locked. Authorization is enforced server-side
regardless; the rail's gating is presentation, not security.

## Every size keeps every feature

The toolbar's **Filters** button opens the filter drawer at every width; the chips row keeps active state visible while
it's closed. On narrow screens the table trims itself to what fits: selection, the details control, the title (with the
author stacked beneath it), and the added date; every dropped column's data is one press away in the details drawer, and
the full column set returns with the room to show it. Below 1024px, the navigation rail becomes a drawer behind a
floating menu button in the top-left corner. Nothing is desktop-only: every destination, filter, and action is reachable
at every size, just behind one more press.

## Accessibility

The shell targets WCAG 2.2 AA. Concretely: a skip-to-content link is the first thing in tab order; the rail, primary
nav, admin cluster, and filter drawer are labelled landmarks; the active destination carries `aria-current="page"`;
disabled entries are described, not just dimmed; focus rings are consistent (gold) across every interactive element; and
all motion (including route crossfades) flattens under `prefers-reduced-motion`.
