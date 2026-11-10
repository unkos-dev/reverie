---
type: ADR
profile-version: 1
id: "REV-ADR-0071"
title: "Library table uses React Aria Table"
status: "accepted"
recorded-on: "2026-11-10"
decided-on: "2026-11-10"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0070"
---

# Library table uses React Aria Table

## Context and problem statement

The Library table is Reverie's editing surface for the collection, and it renders on react-data-grid (RDG) through the
interfaces RDG ships, as recorded in
[Library table uses react-data-grid directly](./0070-library-table-uses-react-data-grid-directly.md).

Columns that resize, and Title and Authors that wrap, need three things the interfaces RDG ships do not provide:

- RDG takes each row's height from the app as a number or a function of the row, and never from the rendered content.
  Rows that fit their wrapped text need a measurement layer the app owns.
- RDG sizes auto-width columns from the rows it has rendered and sizes them again after each page loads. Title and
  Authors shrink to their minimums while other columns grow, and the table overflows sideways. Fixed or default widths
  are the only stable alternative, and they do not fill the table at every viewport.
- React Aria's press handling stops the `mousedown` RDG uses to activate a cell, so every cell holding a React Aria
  control needs a bridge between the two interaction models.

The rest of the frontend already uses React Aria Components for interaction, under
[Use React Aria for the interactive UI foundation](./0060-use-react-aria-for-the-interactive-ui-foundation.md), which
left the grid engine outside its scope.

Which component should the Library table render on?

## Decision drivers

- The Library table is a showpiece surface, so its rendered look and behaviour come first.
- Columns fill the available width at every viewport, and Title and Authors stay fully visible as they wrap.
- The frontend uses one interaction foundation and keeps custom code only where a tool has a demonstrated gap.

## Considered options

- **React Aria Table**: React Aria's `Table` with `Virtualizer` and `TableLayout`, `ResizableTableContainer` and
  `ColumnResizer`, with cell editing composed from React Aria controls.
- **Keep react-data-grid**: keep RDG, own the row-height measurement and give columns fixed or default widths.
- **AG Grid Community**: AG Grid's free edition.
- **TanStack Table with a virtualiser**: headless table logic with a separate row virtualiser.

## Decision outcome

Chosen option: **React Aria Table**, because its documented column model and virtualiser deliver the layout RDG could
not: column widths default to `1fr` with minimums and resizing, and `TableLayout` supports variable row heights from an
estimate. The table also shares the interaction model of every other React Aria control in the app, so no bridge sits
between them.

### Consequences

- Positive: columns fill the table and resize through React Aria's own components, and rows grow to fit wrapped text
  with no app-owned measurement code.
- Positive: focus, press, keyboard and overlay behaviour in the table is React Aria's, as everywhere else in the app.
- Positive: the `react-data-grid` dependency and its dark-mode token bridge can be removed once nothing imports them.
- Negative: React Aria's `Table` has no cell-editing model. The app owns which key or press opens an editor, how an
  editor commits and cancels, and where it is placed.
- Negative: React Aria's `sortDescriptor` holds one column, so multi-level sorting and its Shift modifier are app code.
- Negative: with variable row heights, the scrollbar is sized from `estimatedRowHeight` and can shift as rows render.

## Pros and cons of the options

### React Aria Table

- Positive: column sizing, resizing and the virtualiser are documented React Aria API.
- Negative: editing, multi-level sorting and the rating control are built from React Aria parts rather than supplied.

### Keep react-data-grid

- Positive: its editing keys and lifecycle are native.
- Negative: row heights need an app-owned `ResizeObserver` layer, and auto-width columns overflow after a page loads
  unless widths are fixed.
- Negative: React Aria controls inside a cell need the `mousedown` bridge.

### AG Grid Community

- Negative: its cell editing did not work when tried.

### TanStack Table with a virtualiser

- Negative: grid semantics, keyboard navigation and focus management would all be built by hand. React Aria's `Table`
  supplies them.

## More information

- [React Aria Table](https://react-aria.adobe.com/Table): column resizing, sorting and the `1fr` default width.
- [React Aria `Virtualizer`](https://react-aria.adobe.com/Virtualizer): `TableLayout` and `estimatedRowHeight`.
