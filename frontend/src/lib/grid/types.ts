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
 * This is the read-only tranche of the contract: there is no edit hook. A
 * write surface (cell editors, commit/cancel) is a later addition to this
 * same contract, not a replacement for it.
 */
import type { UIEvent } from "react";

/**
 * Binding-agnostic column definition for a row of type `R`. `key` is the
 * stable column identifier and doubles as the sort key when the column is
 * sortable. `accessor` returns the display string for the column, so the
 * caller owns projection and every binding renders identical cell text.
 */
export type GridColumn<R> = {
  key: string;
  name: string;
  sortable: boolean;
  width?: number;
  accessor: (row: R) => string;
};

/** Single-column sort state. Multi-column sort is out of scope for this contract. */
export type SortState = { columnKey: string; direction: "asc" | "desc" } | null;

/** Selected-cell report emitted on focus change. */
export type FocusReport = { rowIdx: number; columnKey: string };

/**
 * The prop contract a grid binding satisfies for row type `R`. The binding
 * owns no state of its own beyond what the vendor grid requires internally:
 * it is a controlled view that reports sort and focus changes back to its
 * caller, and forwards native scroll events for fetch-on-scroll paging.
 */
export type GridBindingProps<R> = {
  rows: readonly R[];
  columns: readonly GridColumn<R>[];
  sort: SortState;
  onSortChange: (sort: SortState) => void;
  onCellFocus: (report: FocusReport) => void;
  /** Passthrough for the binding's native scroll container; drives fetch-on-scroll paging. */
  onScroll?: (event: UIEvent<HTMLDivElement>) => void;
  /** Wrapper class carrying the Reverie-token theme bridge. */
  className?: string;
  /** Fixed pixel height; the binding needs an explicit scroll viewport. */
  height: number;
};
