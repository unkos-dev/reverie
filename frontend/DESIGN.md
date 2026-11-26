---
name: Reverie
description: A household ebook collection, ready to curate.
colors:
  shell: "var(--shell-ground)"
  sheet: "var(--sheet-ground)"
  surface: "var(--surface)"
  surface-2: "var(--surface-2)"
  surface-3: "var(--surface-3)"
  hairline: "var(--hairline)"
  control-border: "var(--border-control)"
  foreground: "var(--fg)"
  muted: "var(--fg-muted)"
  faint: "var(--fg-faint)"
  accent: "var(--accent)"
  accent-soft: "var(--accent-soft)"
  accent-strong: "var(--accent-strong)"
  accent-text: "var(--accent-text)"
  on-accent: "var(--fg-on-accent)"
  danger: "var(--danger)"
  danger-soft: "var(--danger-soft)"
  danger-text: "var(--danger-text)"
  bar: "var(--bar)"
  bar-raised: "var(--bar-raised)"
  overlay: "var(--overlay)"
  neutral-dark: "#eef1fa"
  accent-dark: "#8fa8ff"
  danger-dark: "#ff8a93"
  neutral-light: "#121831"
  accent-light: "#3d55d8"
  danger-light: "#b91c1c"
  neutral-monochrome: "#f3f3f5"
  accent-monochrome: "#f3f3f5"
  danger-monochrome: "#ff8a93"
typography:
  display:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "2.25rem"
    fontWeight: 800
    lineHeight: 1.111111
    letterSpacing: "-0.025em"
  title:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.5
  body:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.5
  table-cell:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.428571
  label:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "0.75rem"
    fontWeight: 500
    lineHeight: 1.333333
  metadata:
    fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace'
    fontSize: "0.75rem"
    fontWeight: 400
    lineHeight: 1.333333
  wordmark:
    fontFamily: '"Satoshi Variable", "Satoshi", system-ui, sans-serif'
    fontSize: "1rem"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "0.32em"
  cover-title:
    fontFamily: '"Author Variable", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontWeight: 600
    lineHeight: 1.06
    letterSpacing: "0.005em"
rounded:
  sm: "9px"
  md: "12px"
  lg: "16px"
  xl: "18px"
spacing:
  '1': "4px"
  '2': "8px"
  '3': "12px"
  '4': "16px"
  '5': "20px"
  '6': "24px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-outline:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-destructive:
    backgroundColor: "{colors.danger-soft}"
    textColor: "{colors.danger-text}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-link:
    backgroundColor: "transparent"
    textColor: "{colors.accent-text}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  input:
    textColor: "{colors.foreground}"
    rounded: "{rounded.sm}"
    padding: "4px 10px"
    height: "34px"
  card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.xl}"
    padding: "16px"
  navigation-current:
    textColor: "{colors.foreground}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "36px"
  filter-chip:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    rounded: "{rounded.lg}"
    padding: "4px 10px"
---

# Design System: Reverie

## Overview

Creative North Star: "The book collection".

The collection supplies the imagery and colour. A floating navigation bar sits above one rounded catalogue sheet, with
compact controls and ruled table rows. Covers retain their own artwork rather than adopting the interface palette.

Mona Sans carries the interface. Periwinkle marks primary actions, selection, focus, and reading progress. The dark
palette places a near-black sheet inside an ink-navy shell; light uses cool pale grounds, and Monochrome uses graphite.
The interface has no decorative texture. Existing generated covers retain their cloth textures and metallic detail.

**Key Characteristics:**

- One catalogue sheet beneath a translucent navigation bar.
- Compact controls, quiet rows, and visible book covers.
- Theme-following semantic colours with separate, fixed cover artwork.
- Local edit feedback and reduced-motion alternatives.

Functional behaviour is governed by the [specification index](../docs/specs/index.md). This guide records the built
visual system.

## Colors

The theme-specific neutral, accent, and error anchors record the generated sRGB palette families. Component colour
assignments use the semantic roles.

### Primary

Periwinkle is the interface accent in dark and light. Monochrome replaces it with neutral light tones. Primary actions
use `accent` with `on-accent` text; hover uses `accent-strong`. Selection uses `accent-soft`, and readable accent text
uses `accent-text`.

### Neutral

`shell` surrounds the darker `sheet` in dark mode. Light keeps the same structure with a pale blue shell and near-white
sheet. `surface`, `surface-2`, and `surface-3` supply control fills and state changes. `hairline` separates content;
`control-border` identifies fields, checkboxes, radio buttons, switches, and selected controls. Foreground, muted, and
faint text retain distinct roles.

The native theme menu offers System, Light, Dark, and Monochrome. System follows the device preference with a dark
fallback. The first three options use periwinkle. Monochrome selects the dark account theme and stores its palette in a
browser-local cookie. Both choices resolve before the application module loads. Generated colour scales include sRGB and
Display P3 values.

### Error and artwork colours

The reserved red identifies errors that need action, with a message or icon. It does not replace neutral status styling.
The retained slot mark uses ochre (`#c9a961`); the Satoshi wordmark uses cream (`#e8e0d0`) in dark and near-black
(`#0e0d0a`) in light. Generated covers keep their theme-fixed cream, tan (`#e8dcc2`), ochre, and six dark binding tones.
These artwork colours do not become interface accents.

**The semantic colour rule.** Interface elements use semantic roles; fixed artwork keeps its own palette.

## Typography

Mona Sans is the self-hosted interface family, with system sans-serif fallbacks and tabular figures. JetBrains Mono
identifies ISBNs, identifiers, and format codes. Satoshi remains in the uppercase wordmark. Author remains in generated
cover titles, in italic at 600 weight.

The Library heading uses the display role. Filter headings use the title role. Table cells and standard buttons use 14px
text; compact labels and cover metadata use 12px. Cover titles below artwork use 13px Mona Sans at weight 600.
Generated-cover titles use 14, 17, or 22 SVG units according to line length, within a 240 by 360 viewBox.

Text fields use 16px below the 768px breakpoint and 14px above it. Long table values truncate within their columns.

## Layout

The desktop shell has a 12px outer inset and a sticky 52px top bar. Its three columns place navigation on the left,
search in a centre column up to 560px wide, and account controls on the right. The catalogue sheet fills the remaining
width, without a desktop maximum width.

Below 1024px, the top bar keeps the mark, search button, and account control. Primary navigation moves into a floating
bottom bar. The sheet remains rounded, and shell padding reserves room for the bottom bar. Admin destinations appear in
the account menu at these widths.

The Library header stays at full size while its collection scrolls. Shelves are chosen through Filters or the Shelves
page; there is no shelf-tab row. The toolbar wraps when needed. Its page search keeps a 240px width.

From 1600px, Filters opens a 316px panel beside the collection with a 12px gap. The collection narrows as the panel
opens. Below that breakpoint, Filters uses a right-hand modal sheet. The panel scrolls internally beneath its header.

Table rows are 52px in comfortable density and 40px in compact density. Native column widths share available space with
minimums before horizontal scrolling. Title has the largest flexible share. At 1024px the minimum-width table retains a
small horizontal scroll.

Covers keep a 2:3 aspect ratio. Their chosen pixel width stays fixed as the grid fits fewer or more columns, with 24px
horizontal and 28px vertical gaps. The default 180px width gives nine columns at 1920px with Filters closed. The batch
bar takes real space beneath the collection, keeping pagination clear when its contents wrap.

## Elevation & Depth

Cards rest flat with hairline borders. Floating navigation, batch bars, menus, dialogs, and `TooltipContent` use the
raised shadow. The default raised shadow is `0 20px 60px rgb(0 0 0 / 35%)`; light uses `0 4px 16px rgb(0 0 0 / 10%)`.
Navigation, batch bars, and the detached filter panel use a 22px backdrop blur. `TooltipContent` uses an opaque surface
rather than blurred bar material.

The normal modal scrim uses navy at 88% opacity. Monochrome derives the same opacity from its sheet ground. Cover
artwork has its own light-theme shadow and no dark-theme shadow.

Panels and bars enter with a short slide and fade. The Library panel transition lasts 180ms with the emphasised ease.
Reduced motion removes these transitions and the ingestion sweep, pending sweep, and saved fade.

**The floating depth rule.** Resting cards stay flat; floating controls carry the raised shadow.

## Shapes

The main radius scale is concentric, from 9px controls to an 18px sheet and filter panel. Compact segmented controls
nest a 7px item inside a 10px track. Small badges use 8px corners. Thin borders follow these shapes. Cover artwork clips
to its rounded container; progress follows the cover's bottom edge.

## Components

### Buttons and fields

Buttons have compact labels, a hairline boundary, and a 34px default height. Primary buttons use the accent fill.
Outline buttons rest on the sheet, secondary buttons on a surface, and ghost buttons begin transparent with muted text.
Destructive buttons use the soft error fill and readable error text. Link buttons underline on hover.

Hover moves neutral controls up one surface step and pressed moves one step further. Disabled and pending buttons reduce
opacity. Fields have a control boundary and a transparent resting fill in light, with a surface fill in dark. Invalid
fields pair the error boundary with their field message.

The global focus indicator is a 1.5px accent-text outline with a 5px translucent halo. The table uses a 2px inset cell
indicator, and links or buttons inside a cell show their own focus indicator.

### Navigation and selection

The current page has a raised fill and an identifying outline. Dark bar selections use the translucent bar-raised fill
and its dedicated boundary. Segmented controls on the sheet use a neutral track, a raised selected fill, and a control
boundary. Removable filter chips keep a quiet outline, a muted category label, and a foreground value.

### Library shelf header

The header pairs the open shelf name with its total. The reading-state bar uses semantic foreground, accent, surface,
and control-border colours. Each non-zero segment and its legend button selects one exact status. The legend keeps
Finished, Reading, and Unread visible at zero. Pressed controls have an accent-text outline. Counts use tabular figures.

Needs attention sits to the right on the same row. Compact outline buttons use muted labels and foreground counts. The
order is to review, enrichment failed, no ISBN, and no cover. Zero counts and the child account's to review count stay
hidden. The group is hidden below the 640px breakpoint.

### Table and covers

Rows use hairline rules, a surface hover, and a soft-accent selection. Empty ratings and the ISBN editing affordance
appear on hover or focus. The book title link and hover Open control navigate; clicking or pressing Enter on a row does
not. Enter submits and closes a cell editor. The cell shows progress while saving, then briefly marks a successful save.
On failure, one dismissible notice above the table names the book, field and reason. A stale baseline refreshes the
visit automatically. Reopening a cell begins a new edit with the values on screen.

Covers show their selection checkbox on hover, focus, or selection. Selected artwork gains an accent outline. Reading
progress occupies the bottom edge; status and rating sit below the author. Stored cover images take precedence over the
generated binding artwork.

### Filters, sizing, and batch actions

Filters is a detached card with compact labelled sections. Status and Shelf align their facet counts to the right in
muted, tabular figures. The toolbar gives the open shelf's total or the matching count out of that total. The Cover
width slider shows a pixel value and changes cover width live between 120 and 300px in one-pixel steps. Its default is
180px; the browser stores the value when the interaction ends.

The floating batch bar keeps its selection count, Add to shelf, Set status, Refresh metadata and Clear selection
controls together. Set status uses the existing five statuses and No status in the native menu. Refresh metadata is
available to non-child callers and reports scheduled work. Pending and result text stay in the bar. Set status saves
each book in sequence and reports how many books saved and how many could not be saved. The bar sits above bottom
navigation at narrow widths. Subtitle is hidden by default; an explicit column override, including an empty array,
controls its visibility.

## Do's and Don'ts

### Do

- Do keep interface colours on semantic roles so all palettes use the same components.
- Do keep book artwork and its generated-cover texture separate from interface styling.
- Do use the raised shadow for floating controls and flat borders for resting cards.
- Do keep edit feedback in its owning context and preserve reduced-motion alternatives.

### Don't

- Don't apply the generated covers' decorative texture or metallic detail to interface controls.
- Don't use the error hue as a general attention accent.
