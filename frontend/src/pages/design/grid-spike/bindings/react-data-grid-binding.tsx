/**
 * react-data-grid binding of the shared `GridBindingProps` contract.
 *
 * RDG ships native ARIA grid semantics, a real editor API, and CSS-variable
 * theming; the theme half is `theme.css`. Fetch-on-scroll and multi-column
 * sort are the app's job either way, so the spike keeps single-column sort and
 * one editable text cell (design D6).
 */
import { useMemo, type ReactElement, type ReactNode } from "react";
import {
  DataGrid,
  renderTextEditor,
  type Column,
  type RowsChangeData,
  type SortColumn,
} from "react-data-grid";
import "react-data-grid/lib/styles.css";

import { HEADER_HEIGHT, ROW_HEIGHT } from "../theme";
import "../theme.css";
import { COLUMN_KEYS, type ColumnKey, type GridBindingProps, type SpikeBookRow } from "../types";

function isColumnKey(key: string): key is ColumnKey {
  return COLUMN_KEYS.some((k) => k === key);
}

function toRdgColumns(columns: GridBindingProps["columns"]): readonly Column<SpikeBookRow>[] {
  return columns.map((col) => ({
    key: col.key,
    name: col.name,
    sortable: col.sortable,
    width: col.width,
    renderCell: ({ row }): ReactNode => col.accessor(row),
    renderEditCell: col.editable ? renderTextEditor : undefined,
  }));
}

export function ReactDataGridBinding(props: GridBindingProps): ReactElement {
  const { rows, columns, sort, onSortChange, onCellEdit, onCellFocus, className, height } = props;

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
    if (!isColumnKey(first.columnKey)) {
      onSortChange(null);
      return;
    }
    onSortChange({
      columnKey: first.columnKey,
      direction: first.direction === "ASC" ? "asc" : "desc",
    });
  }

  function handleRowsChange(nextRows: SpikeBookRow[], data: RowsChangeData<SpikeBookRow>): void {
    const key = data.column.key;
    if (!isColumnKey(key)) return;
    const updated = nextRows[data.indexes[0]];
    const column = columns.find((c) => c.key === key);
    if (column === undefined) return;
    onCellEdit({ rowId: updated.id, columnKey: key, value: column.accessor(updated) });
  }

  const wrapperClass = className === undefined ? "spike-grid-rdg" : `spike-grid-rdg ${className}`;

  return (
    // Height is a dynamic, prop-driven scroll viewport (cardinal-rule exception).
    <div className={wrapperClass} style={{ height }}>
      <DataGrid
        columns={rdgColumns}
        rows={rows}
        rowKeyGetter={(row) => row.id}
        onRowsChange={handleRowsChange}
        sortColumns={sortColumns}
        onSortColumnsChange={handleSortColumnsChange}
        onSelectedCellChange={({ rowIdx, column }) => {
          if (isColumnKey(column.key)) onCellFocus({ rowIdx, columnKey: column.key });
        }}
        rowHeight={ROW_HEIGHT}
        headerRowHeight={HEADER_HEIGHT}
      />
    </div>
  );
}
