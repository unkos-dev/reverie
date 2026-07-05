/**
 * react-data-grid binding of the shared `GridBindingProps<R>` contract.
 *
 * This is the only file in `frontend/src` allowed to import `react-data-grid`;
 * everything else in the app depends on `./types`, never on RDG's own types.
 * RDG ships native ARIA grid semantics and CSS-variable theming; the theme
 * half of the bridge is `grid-theme.css`, imported here alongside RDG's own
 * stylesheet. Read-only tranche: no editor wiring.
 */
import { useMemo, type ReactElement, type ReactNode } from "react";
import { DataGrid, type Column, type SortColumn } from "react-data-grid";
import "react-data-grid/lib/styles.css";
import "./grid-theme.css";

import type { GridBindingProps } from "./types";

const ROW_HEIGHT = 36;
const HEADER_HEIGHT = 40;

/**
 * The RDG binding's own props: the shared contract plus a row-identity
 * extractor. RDG needs a stable per-row key for its keyed reconciliation, but
 * the shared contract makes no assumption about what shape `R` is, so the
 * extractor is a binding-specific addition rather than part of `GridBindingProps`.
 */
export type ReactDataGridBindingProps<R> = GridBindingProps<R> & {
  rowKey: (row: R) => string;
};

function toRdgColumns<R>(columns: GridBindingProps<R>["columns"]): readonly Column<R>[] {
  return columns.map((col) => ({
    key: col.key,
    name: col.name,
    sortable: col.sortable,
    width: col.width,
    renderCell: ({ row }: { row: R }): ReactNode =>
      col.renderCell === undefined ? col.accessor(row) : col.renderCell(row),
  }));
}

export function ReactDataGridBinding<R>(props: ReactDataGridBindingProps<R>): ReactElement {
  const {
    rows,
    columns,
    label,
    sort,
    onSortChange,
    onCellFocus,
    onScroll,
    rowKey,
    className,
    height,
  } = props;

  const rdgColumns = useMemo(() => toRdgColumns(columns), [columns]);

  const sortColumns: readonly SortColumn[] =
    sort === null
      ? []
      : [{ columnKey: sort.columnKey, direction: sort.direction === "asc" ? "ASC" : "DESC" }];

  function handleSortColumnsChange(next: SortColumn[]): void {
    if (next.length === 0) {
      onSortChange(null);
      return;
    }
    const first = next[0];
    onSortChange({
      columnKey: first.columnKey,
      direction: first.direction === "ASC" ? "asc" : "desc",
    });
  }

  const wrapperClass = className === undefined ? "rv-grid" : `rv-grid ${className}`;

  return (
    // Height is a dynamic, prop-driven scroll viewport (cardinal-rule
    // exception); when omitted the caller's className must size the wrapper.
    <div className={wrapperClass} style={height === undefined ? undefined : { height }}>
      <DataGrid
        aria-label={label}
        columns={rdgColumns}
        rows={rows}
        rowKeyGetter={rowKey}
        sortColumns={sortColumns}
        onSortColumnsChange={handleSortColumnsChange}
        onSelectedCellChange={({ rowIdx, column }) => {
          onCellFocus({ rowIdx, columnKey: column.key });
        }}
        onScroll={onScroll}
        rowHeight={ROW_HEIGHT}
        headerRowHeight={HEADER_HEIGHT}
      />
    </div>
  );
}
