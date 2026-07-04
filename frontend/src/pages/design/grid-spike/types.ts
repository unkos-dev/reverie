/**
 * Shared contract for the grid bake-off.
 *
 * One `GridBindingProps` surface, two bindings (react-data-grid, AG Grid
 * Community). The harness drives both bindings through this identical prop
 * shape so the abstraction boundary is validated before any production code
 * depends on a grid stack. Adapter surface (design D1): column definitions,
 * row window feed, sort state, focus/keyboard surface, edit hook, theme hook.
 *
 * This module is dev-only. It lives under `pages/design/` so the Vite
 * `codeSplitting` group strips it from production builds; see
 * `frontend/vite.config.ts` and `scripts/assert-no-design-chunk.mjs`.
 */
import type { ComponentType } from "react";
import { z } from "zod";

/**
 * Per-user reading summary, mirroring the backend `ReadingStateSummary`
 * projection. Every field is nullable: a book may carry no reading state.
 */
export const ReadingStateSummarySchema = z.object({
  status: z.string().nullable(),
  rating: z.number().int().nullable(),
  progress_pct: z.number().nullable(),
});
export type ReadingStateSummary = z.infer<typeof ReadingStateSummarySchema>;

/**
 * One synthetic row, shaped like the post-#574 `BookListRow` projection the
 * production `/api/v1/books` list returns (design D3). Deliberately
 * self-contained: the spike never touches the real endpoint, so it does not
 * depend on the frontend `BookListItem` schema (which is drifted and lacks
 * `subtitle`/`pages`/`reading_state`).
 */
export const SpikeBookRowSchema = z.object({
  id: z.string(),
  title: z.string(),
  subtitle: z.string().nullable(),
  authors: z.array(z.string()),
  isbn_13: z.string().nullable(),
  pages: z.number().int().nullable(),
  reading_state: ReadingStateSummarySchema.nullable(),
});
export type SpikeBookRow = z.infer<typeof SpikeBookRowSchema>;

/** Column keys the spike renders. Kept as a union, not an enum (cardinal rule). */
export const COLUMN_KEYS = [
  "title",
  "subtitle",
  "authors",
  "isbn_13",
  "pages",
  "reading_state",
] as const;
export type ColumnKey = (typeof COLUMN_KEYS)[number];

/**
 * Binding-agnostic column definition. Each binding maps this to its own
 * native column type. `accessor` returns the display string so the shared
 * model owns projection (authors join, reading-state flattening) and both
 * grids render identically.
 */
export type GridColumn = {
  key: ColumnKey;
  name: string;
  sortable: boolean;
  editable: boolean;
  width?: number;
  accessor: (row: SpikeBookRow) => string;
};

/** Single-column sort state. Multi-column sort is a post-verdict child issue. */
export type SortState = { columnKey: ColumnKey; direction: "asc" | "desc" } | null;

/** Selected-cell report emitted on focus change (focus/keyboard surface). */
export type FocusReport = { rowIdx: number; columnKey: ColumnKey };

/** A committed single-cell text edit (design D6: local state only). */
export type CellEdit = { rowId: string; columnKey: ColumnKey; value: string };

/**
 * The one prop contract both bindings satisfy. The harness owns all state and
 * instrumentation; a binding is a controlled view that reports focus/edit
 * events back up.
 */
export type GridBindingProps = {
  rows: readonly SpikeBookRow[];
  columns: readonly GridColumn[];
  sort: SortState;
  onSortChange: (sort: SortState) => void;
  onCellEdit: (edit: CellEdit) => void;
  onCellFocus: (report: FocusReport) => void;
  /** Wrapper class carrying the Reverie-token theme bridge (theme hook). */
  className?: string;
  /** Fixed pixel height; both grids need an explicit scroll viewport. */
  height: number;
};

/** Candidate identifiers. */
export const BINDING_IDS = ["react-data-grid", "ag-grid"] as const;
export type BindingId = (typeof BINDING_IDS)[number];

/** A registered candidate: its id, human label, and the view component. */
export type GridBinding = {
  id: BindingId;
  label: string;
  Component: ComponentType<GridBindingProps>;
};
