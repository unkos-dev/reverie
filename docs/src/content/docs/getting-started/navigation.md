---
title: Navigating Reverie
description: "How the app shell is organised: the navigation rail, contextual filters, search, and the thinking behind them."
---

Every screen in Reverie wears the same chrome: a navigation rail on the
left and, on browse surfaces, contextual filters in a drawer on the
right. This page describes how to move around and why the shell is
shaped the way it is.

## Global rail, contextual drawer

Reverie separates **where you are** from **what you're looking at**:

- The **left rail** is global. It always shows the same destinations:
  Library, Shelves (with your shelves nested beneath), and, for
  administrators, the admin cluster, so wayfinding never depends on
  the current screen.
- **Filters** are contextual. On browse surfaces (the Library today)
  the toolbar's **Filters** button opens a drawer from the right edge
  carrying every filter for the books in view. Detail pages, admin
  screens, and the reader don't have one.

Keeping the two apart matters because the jobs are different: global
navigation must be stable and learnable, while filters must change
with the content. Folding both into a single sidebar forces one of
them to misbehave. The filter drawer overlays the books rather than
claiming a permanent column, so the collection keeps the full width
while you browse; the chips row under the toolbar (below) keeps active
filters visible while the drawer is closed.

Your shelves live in the left rail under **Shelves**, capped at seven
rows with an "All shelves" overflow link; shelves are destinations you
return to, not filters you apply.

## Planned entries are visible

The rail shows a few destinations that aren't built yet; Home and
Stats render dimmed with a "Planned: not in this release" note.
Settings is not a rail destination: it lives in the user menu at the
foot of the rail, carrying the same planned treatment.
This is deliberate: the rail communicates the product's intended
shape, and a self-hosting operator evaluating Reverie can see where it
is going without reading a roadmap. Planned entries are skipped by
keyboard navigation and never act as links.

## Two search jobs, two surfaces

Reverie separates finding a book from narrowing the books in view.

The **command palette** navigates: pick a book and it takes you there.
Open it with `⌘K` / `Ctrl K` or the `/` key from anywhere outside a
text field. The **search box** in the Library toolbar filters: type
two or more characters and the collection you are browsing shrinks to
the matches, in whatever view and sort order you already had. It never
navigates, and clearing it brings everything back.

The split keeps each surface honest. A single box would have to guess
whether you meant "go to this book" or "show me fewer books", and the
palette stays free to grow into a broader command surface without
dragging filter semantics along.

## One query, two views

The Library shows the same collection two ways: a cover **grid** and a
ledger-style **table**. The view toggle sits in the toolbar. Grid is
for recognising books by cover; the table is for managing them:
scanning dense metadata, editing in place, and acting on many books at
once. Filters, search, and sort belong to the query, not the view, so
switching views never changes which books you're looking at, only how
they're drawn.

The table carries a full keyboard model: arrow keys move cell by cell,
`Home` and `End` jump within a row, `Ctrl Home` and `Ctrl End` jump to
the first and last loaded row, and `PageUp`/`PageDown` move a viewport
at a time. Press `?` inside the table for the complete shortcut list.

Two toolbar controls shape the table to your taste. The **density
toggle** switches between comfortable rows (with a small cover beside
each title) and compact ones. The **Columns** popover shows or hides
individual columns; Title stays put because every row needs its
anchor. Both are personal display preferences, and so are your view
choice and your sort: they belong to your account rather than to one
browser, so they follow you to a second machine and survive signing out
and back in. They never travel in a shared link, which carries the
query (filters and search) instead: someone opening your link sees the
same books, arranged their own way.

**Reset to default** at the foot of the Columns popover clears your
column choice rather than merely showing everything, so the table goes
back to whatever this installation ships as its default and stays with
that default if it later changes. It touches columns only: density,
view, and sort keep whatever you chose. The control is greyed out when
your columns already match the default.

A preference set in one tab does not reach another tab that is already
open; whichever you change last is what sticks, and reloading the other
tab catches it up.

Rows stream in as you scroll rather than loading all at once, which is
why the table says "loaded": with a large library the first screen
appears immediately and the count grows as you go. A **Load more**
button beneath the rows is the keyboard-reachable fallback for when
scrolling can't trigger loading, such as a viewport tall enough to
show a whole page without a scrollbar. Sorting happens on the server
through a column header press (see below), so the order is correct
across the whole collection, not just the rows on screen.

Your view choice travels in the URL (`?view=table`), so a shared link
opens exactly what you see; Reverie also remembers your last choice on
your account and uses it the next time you open the Library without
one, on this machine or any other.

## Book details without leaving the list

Each table row starts with a details control; pressing it (or clicking
any read-only cell, or pressing `Enter` on one) opens the book's
details in a drawer from the right edge: cover, facts, and real
actions, **View full record** and **Add to shelf**. `Escape` closes it
and puts focus back where you were. The drawer exists so a triage pass
over many books doesn't cost a round trip through the book page for
each one; the grid keeps its different habit, where a card click goes
straight to the full record, because browsing covers ends in reading,
not managing. The filter drawer and the details drawer share the right
edge and never stack: opening one closes the other.

## Selecting books and acting on them

Table rows carry checkboxes. Select a few (shift-click selects a
range; `Shift Space` toggles the current row) and a bar rises from the
bottom with the count and the actions that exist today: **Add to
shelf** and **Clear selection**. The header checkbox selects the
**loaded** rows, and says so: with a large library the rows still
streaming in can't be part of a selection, and a control that silently
meant "everything matching the filter" would be worse than one that
names its limit. Changing the query (filters, search, sort) or
switching views clears the selection, because acting on rows you can
no longer see is the kind of surprise a management surface exists to
prevent.

## Sorting the library

The library always has an order. Before you have chosen one it is the
order Reverie ships with, newest additions first, and the Sort section
shows exactly that, so what you see is always what is arranging the
list. There is no "unsorted": a sort gesture changes the order, never
switches ordering off.

The filter drawer carries a **Sort** section in every view. Add up to
three levels from its field picker, flip a level's direction, reorder
levels, or remove one; none of it needs a modifier key. The grid sorts
from here, and the section always shows the full stack, whichever
surface built it. In the table, only the active sort shows a direction
indicator on its header, so a glance at the header row tells you what
orders the list without decoding every column.

The table keeps its faster gesture on top. Click a column header to
sort by that column, and click again to reverse it. `⌘-click` /
`Ctrl-click` a header adds it as another sort level instead of
replacing the current one, so you can sort by author and then, within
each author, by newest first. A plain click on any header resets the
sort back down to that single column; the drawer's Sort section is the
recovery surface when a stray click collapses a stack you built.

Books with nothing to sort on, no page count, no author yet, always
sort to the end, whichever direction you're sorting in.

The sort you choose sticks, on your account rather than on this
browser: the library keeps that order until you sort differently,
here or on any other machine. **Reset** beside the Sort section's
title takes you back to the order Reverie ships with, and so does
removing the stack's last level; the control only appears while you
have a sort of your own to remove.

Multi-level sort exists so that a large library can be scanned in exactly
the order you want, author then most recent within each author, for
example, while paging stays stable: rows don't get skipped or repeated
as you load more.

## Filtering the library

The filter drawer is the single home for every filter, in every view.
Grid and table express the identical grammar, because one surface
edits the same URL state for both; a condition set while browsing
covers never disappears when you switch to the table, or the other
way around.

The drawer stacks one collapsible section per condition. Shelf and
series are pick-one facets. Authors, tags, genres, and moods each get
a typeahead that accepts several values at once, in any-of, all-of, or
none-of mode, so you can ask for books tagged both X and Y, in any of
three genres, or in none of a set of moods. Reading status filters by
state, including **Unread** for books you have not started. Text
columns (title, subtitle, ISBN) filter by contains, equals, or
is-empty; page count and rating filter by range; **added** filters by
a date range.

A section with something active opens on mount, shows a count beside
its name, and carries its own **Clear**. Everything else stays folded,
so a long drawer reads as a table of contents rather than a wall of
form controls.

Under the toolbar, in every view, each active condition renders as a
**chip** ("Author: Le Guin", "Pages ≥ 300") with its own remove
control, plus a **Clear all**. Exclusions read as exclusions: a
none-of condition shows as "Author: not Le Guin", and a mixed set
splits the count ("Tags (2, 1 not)"), so a chip never states the
opposite of what the filter does. The chips keep state visible while
the drawer is closed, so the collection can never be silently narrower
than it looks, and dropping one condition is one click instead of a
trip into the drawer.

Filters and search live in the URL, so a filtered library is
bookmarkable and shareable, and reloading the page brings back exactly
the filters you had. Whoever opens the link sees those same books in
their own sort order, because ordering belongs to the account, not the
address.

## Editing from the table

Most cells in the table edit in place. Title, subtitle, ISBN-13, pages,
and authors write to the book's canonical metadata; reading status and
rating write to your own reading state. Series stays read-only: no
metadata field backs that column, so it never opens an editor.

Press `Enter`, press `F2`, or start typing while a cell is selected to
open its editor. `Enter` commits whatever you typed; `Escape` discards
it and leaves the cell as it was. `F2` only opens the editor here; it
does not also close one the way the WAI-ARIA grid pattern describes.
The grid library Reverie is built on doesn't implement that half of the
toggle, so `Escape` or `Enter` are what close an editor regardless of
how you opened it.

Title, subtitle, and authors describe the work rather than one edition
of it, so editing any of them from one row updates every edition of
that work currently loaded in the table, not just the row you touched.
Those three column headers carry an info control explaining that reach,
so you know what an edit touches before you commit: hover it for the
tooltip (dismiss with `Escape`), and screen readers hear the same note
as the column's description when the header takes focus. The control
stays out of the keyboard path on purpose, so pressing `Enter` or
`Space` on the header sorts the column the same as any other. ISBN-13
and pages describe one edition and stay put. Status and rating are
yours alone: nobody else browsing the same library sees them, and they
never touch the book's version history.

### Undo

Every commit ends in a toast with an Undo button, and `Ctrl Z` undoes
the most recent edit without needing the toast still on screen. The
table remembers your last ten edits, so pressing undo repeatedly steps
back through them in order.

Undo works differently depending on what you edited. Title, subtitle,
ISBN-13, pages, and author edits undo by restoring whichever metadata
version was current immediately before your edit, the same version
history the book page's Versions tab reads from. Status and rating have
no version history behind them, so undoing one of those just writes
your previous value back.

### Why edits are staged, not overwritten

The table records a canonical metadata edit as a new entry in the same
version history the Versions tab shows, rather than overwriting the row.
That is what makes undo possible: the value you had before is still
there to restore. It also means a run of quick edits across a dense
table stays reviewable afterward on the book page, not just the value
that happens to be current.

### While a write is in flight

A cell shows what you typed the moment you commit, marked as busy until
the server confirms it. On success the cell settles on the value the
server stored, which can differ slightly from what you typed: an
ISBN-13 you enter with hyphens, for example, comes back normalised.
On failure the cell snaps back to the value it held before your edit,
and a toast explains what went wrong.

## Cinematic mode

On the Library page, press `F` (outside any text field) to dissolve the
surrounding chrome; the navigation rail, toolbar, and controls fade
out so the cover art fills the screen. Press `F` again or `Escape` to
exit. After two seconds of pointer stillness the cursor hides too;
moving the pointer brings it back.

## Admin is a different room

Admin screens (Users, Ingestion) shift the canvas tone slightly darker.
The chrome stays identical; only the surface changes, signalling
"operational room" rather than "reading room". The admin cluster in the
rail is only rendered for administrators; for everyone else it is
structurally absent rather than locked. Authorization is enforced
server-side regardless; the rail's gating is presentation, not
security.

## Every size keeps every feature

The toolbar's **Filters** button opens the filter drawer at every
width; the chips row keeps active state visible while it's closed. On
narrow screens the table trims itself to what fits: selection, the
details control, the title (with the author stacked beneath it), and
the added date; every dropped column's data is one press away in the
details drawer, and the full column set returns with the room to show
it. Below 1024px, the navigation rail becomes a drawer behind a
floating menu button in the top-left corner. Nothing is desktop-only:
every destination, filter, and action is reachable at every size, just
behind one more press.

## Accessibility

The shell targets WCAG 2.2 AA. Concretely: a skip-to-content link is
the first thing in tab order; the rail, primary nav, admin cluster, and
filter drawer are labelled landmarks; the active destination carries
`aria-current="page"`; disabled entries are described, not just dimmed;
focus rings are consistent (gold) across every interactive element; and
all motion (including route crossfades) flattens under
`prefers-reduced-motion`.
