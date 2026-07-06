/**
 * Binding-agnostic grid contract.
 *
 * One `GridBindingProps<R>` surface, implemented by a grid binding component
 * for whatever concrete row type `R` a caller supplies. Consumers depend on
 * this contract, never on a vendor grid's own types, so the grid library
 * underneath a binding can change without touching caller code.
 *
 * Invariant: no vendor grid type may appear in this file.
 *
 * This tranche of the contract adds a write surface: a column becomes
 * editable by supplying `renderEditCell`, and a caller receives committed
 * edits through `onCellEdit`. Editors are handed `GridEditorProps`, never a
 * vendor grid's own editor props, so an editor stays portable across bindings.
 */
import type { ReactNode, UIEvent } from "react";

/**
 * Props handed to a cell editor. `update` stages a draft row without
 * committing it (the editor container commits whatever draft was last
 * staged on Enter); `commit` commits the given row immediately; `cancel`
 * discards the in-progress edit. Editors depend only on this type, never on
 * a vendor grid's editor props.
 */
export type GridEditorProps<R> = {
  row: R;
  update: (row: R) => void;
  commit: (row: R) => void;
  cancel: () => void;
};

/**
 * Binding-agnostic column definition for a row of type `R`. `key` is the
 * stable column identifier and doubles as the sort key when the column is
 * sortable. `accessor` returns the display string for the column, so the
 * caller owns projection and every binding renders identical cell text.
 * `renderCell`, when present, fully replaces `accessor` for rendering and
 * lets a column carry markup (a link, a badge). `accessor` stays mandatory
 * as the column's canonical plain-text projection so a later export or
 * clipboard surface has a markup-free value to read.
 *
 * `renderEditCell`, when present, makes the column editable. `editable`
 * further gates editing per row (for example a locked field); omit it for a
 * column that is editable on every row. `editorOptions` forwards the small
 * set of binding-level editor behaviors a column can tune without reaching
 * into a vendor type.
 */
export type GridColumn<R> = {
  key: string;
  name: string;
  sortable: boolean;
  width?: number;
  accessor: (row: R) => string;
  renderCell?: (row: R) => ReactNode;
  editable?: (row: R) => boolean;
  renderEditCell?: (props: GridEditorProps<R>) => ReactNode;
  editorOptions?: { commitOnOutsideClick?: boolean };
};

/** Single-column sort state. Multi-column sort is out of scope for this contract. */
export type SortState = { columnKey: string; direction: "asc" | "desc" } | null;

/**
 * Selected-cell report emitted on focus change. Carries the row itself, not
 * just its position: with fetch-on-scroll paging the loaded window grows and
 * can reorder, so a positional index captured at focus time does not reliably
 * name the same row later. Any future edit-commit event must key on `row`.
 */
export type FocusReport<R> = { row: R; rowIdx: number; columnKey: string };

/**
 * Committed cell edit. Carries both the edited row and the row as it stood
 * before the edit, so a caller can diff the edited column or capture undo
 * state without a positional lookup into a rows array that may have shifted.
 */
export type CellEditReport<R> = { row: R; previousRow: R; columnKey: string };

/**
 * The prop contract a grid binding satisfies for row type `R`. The binding
 * owns no state of its own beyond what the vendor grid requires internally:
 * it is a controlled view that reports sort and focus changes back to its
 * caller, and forwards native scroll events for fetch-on-scroll paging.
 */
export type GridBindingProps<R> = {
  rows: readonly R[];
  columns: readonly GridColumn<R>[];
  /** Accessible name announced for the grid region. */
  label?: string;
  sort: SortState;
  onSortChange: (sort: SortState) => void;
  onCellFocus: (report: FocusReport<R>) => void;
  /** Fired when an editor commits. Omit for a caller with no editable columns. */
  onCellEdit?: (report: CellEditReport<R>) => void;
  /** Passthrough for the binding's native scroll container; drives fetch-on-scroll paging. */
  onScroll?: (event: UIEvent<HTMLDivElement>) => void;
  /** Wrapper class carrying the Reverie-token theme bridge. */
  className?: string;
  /**
   * Fixed pixel height for the scroll viewport. Omit when the caller sizes
   * the wrapper through CSS instead; the binding needs a definite height
   * from one of the two.
   */
  height?: number;
};
