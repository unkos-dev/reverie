---
version: 1
slug: "src-pages-library-librarypage-tsx"
primary_target: "src/pages/library/LibraryPage.tsx"
related_targets: ["src/components/layout"]
---

# Library and app shell

Scope: the Library route and the app shell every signed-in route shares. Mode: Operate.

Audience and job: a household's curator on a desktop, correcting and organising a large ebook collection; other members
browse and open books, often on a phone. Detailed curation is desktop-first.

Constraints: conventional structure and controls; inline editing with automatic saving; as few dialogs and popovers as
possible; WCAG 2.2 AA; dark is the default theme and light the alternative; colour is a replaceable pair of inputs
(neutral ramp and accent ramp) so further themes are a palette change. The wordmark face and mark are placeholders
pending the brand work.

## Direction contract

THESIS: A library management screen with the finish of a current premium product, whose character comes from what a book
collection has: covers and reading progress. It refuses the category default of a near-black cover grid beside a sidebar
of shelves with one blue accent.

OWN-WORLD: A deep ink-navy shell with the catalogue on a darker rounded sheet. Periwinkle accent, used for focus,
selection, progress and the primary action only. Navigation and batch actions float as translucent bars; the filter
panel is a detached card in the same material. Hairline rules, 9 to 18px concentric radii, soft depth, no gradients or
texture. Mona Sans for interface text with tabular figures. Monochrome on graphite is the second theme.

STORY: The curator sees which shelf is open and how much of it is read, finds the books that need attention, and
corrects them in place without leaving the table.

FIRST VIEWPORT: At 1920 by 1080: a floating top bar with wordmark, primary navigation, centred search and account
controls. Below it one sheet: shelf tabs with counts; the shelf name at display size with its book count; a
reading-state bar with read, reading and unread counts on the left and needs-attention counts on the right; one control
row holding filter chips, result count, save state, undo, Table or Covers, row density, Columns and Filters; then the
table with sortable headers and 52px rows. The primary action, Add to shelf, sits in a floating batch bar at bottom
centre when rows are selected.

SIGNATURE: Reading progress, drawn as the same bar in the shelf header, in each table row and along the bottom edge of
each cover. Motion grammar: panels and bars slide and fade on a short ease-out; the table narrows beside the filter
panel; save state moves from Saving to saved in place; all of it stops under reduced motion.

FORM: The category standard taken at the user's request after three dealt hands, refined over three options in
conversation; not a position on the dealt list. seed:00f19a41251c3fb1

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and
every shipping raster carrying its provenance

## Unresolved

- Which server-dependent parts ship first: reading-state totals, needs-attention counts, filter counts, sorting on every
  column.
- Shelf tab overflow rule; shelf header collapse on scroll; cover-size control in the Covers view.
- Reduced-effects setting is a follow-up.
