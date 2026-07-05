---
title: Navigating Reverie
description: "How the app shell is organised: the navigation rail, contextual filters, search, and the thinking behind them."
---

Every screen in Reverie wears the same chrome: a navigation rail on the
left and, on browse surfaces, a contextual filter rail on the right.
This page describes how to move around and why the shell is shaped the
way it is.

## The dual-rail shell

Reverie separates **where you are** from **what you're looking at**:

- The **left rail** is global. It always shows the same destinations:
  Library, Shelves (with your shelves nested beneath), and, for
  administrators, the admin cluster, so wayfinding never depends on
  the current screen.
- The **right rail** is contextual. It appears only on browse surfaces
  (the Library today) and carries filters for the books in view. Detail
  pages, admin screens, and the reader don't show it.

Two rails beat one because the jobs are different: global navigation
must be stable and learnable, while filters must change with the
content. Folding both into a single sidebar forces one of them to
misbehave.

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

## One search surface

Search lives in one place: the command palette. A search button at the
top of the filter rail opens it; so do `⌘K` / `Ctrl K` and the `/` key
from anywhere outside a text field. There is intentionally no second
inline search box; one surface means search behaves identically
wherever you invoke it, and the palette can grow into a broader command
surface later without a migration.

## Three library views

The Library shows the same collection three ways: a cover **grid**, a
compact **list**, and a spreadsheet-style **table**. The toggle sits
with the sort control above the books. Grid is for recognising books by
cover; the table is for scanning dense metadata (subtitle, ISBN, pages,
reading state) across many books without opening each one.

The table carries a full keyboard model: arrow keys move cell by cell,
`Home` and `End` jump within a row, `Ctrl Home` and `Ctrl End` jump to
the first and last loaded row, and `PageUp`/`PageDown` move a viewport
at a time. Press `?` inside the table for the complete shortcut list.

Rows stream in as you scroll rather than loading all at once, which is
why the table says "loaded": with a large library the first screen
appears immediately and the count grows as you go. Sorting by title or
author happens on the server through a column header press, so the
order is correct across the whole collection, not just the rows on
screen.

Your view choice travels in the URL (`?view=table`), so a shared link
opens exactly what you see; Reverie also remembers your last choice
and uses it the next time you open the Library without one.

## Cinematic mode

On the Library page, press `F` (outside any text field) to dissolve the
surrounding chrome; the navigation rail, filter rail, and controls fade
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

## Small screens keep every feature

Below 1280px, the filter rail collapses into a **Refine** button that
opens the same controls in a sheet. Below 1024px, the navigation rail
becomes a drawer behind a floating menu button in the top-left corner.
Nothing is desktop-only: every destination and every filter is reachable
at every size, just behind one more press.

## Accessibility

The shell targets WCAG 2.2 AA. Concretely: a skip-to-content link is
the first thing in tab order; the rail, primary nav, admin cluster, and
filter rail are labelled landmarks; the active destination carries
`aria-current="page"`; disabled entries are described, not just dimmed;
focus rings are consistent (gold) across every interactive element; and
all motion (including route crossfades) flattens under
`prefers-reduced-motion`.
