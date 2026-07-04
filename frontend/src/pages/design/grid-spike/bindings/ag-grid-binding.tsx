/**
 * AG Grid Community binding of the shared `GridBindingProps` contract.
 *
 * AG covers the full requirement set inside MIT, but owns its look in JS: the
 * theme comes from `buildSpikeAgTheme()` rather than CSS variables, and modules
 * must be registered explicitly (modular imports only, design D1). Client-side
 * row model holds the full generated set so the scroll and Ctrl+End budgets run
 * against all rows; lazy loading is demonstrated through the harness query.
 */
import { useMemo, type ReactElement } from "react";
import { AgGridReact } from "ag-grid-react";
import {
  ClientSideRowModelModule,
  ModuleRegistry,
  ValidationModule,
  type CellFocusedEvent,
  type CellValueChangedEvent,
  type ColDef,
  type ValueGetterParams,
} from "ag-grid-community";

import { buildSpikeAgTheme, HEADER_HEIGHT, ROW_HEIGHT } from "../theme";
import { COLUMN_KEYS, type ColumnKey, type GridBindingProps, type SpikeBookRow } from "../types";

ModuleRegistry.registerModules([ClientSideRowModelModule, ValidationModule]);

const SPIKE_THEME = buildSpikeAgTheme();

function isColumnKey(key: string): key is ColumnKey {
  return COLUMN_KEYS.some((k) => k === key);
}

function toAgColumns(columns: GridBindingProps["columns"]): ColDef<SpikeBookRow>[] {
  return columns.map((col) => ({
    colId: col.key,
    field: col.key,
    headerName: col.name,
    sortable: col.sortable,
    editable: col.editable,
    width: col.width,
    valueGetter: (params: ValueGetterParams<SpikeBookRow>): string =>
      params.data === undefined ? "" : col.accessor(params.data),
  }));
}

export function AgGridBinding(props: GridBindingProps): ReactElement {
  const { rows, columns, onCellEdit, onCellFocus, className, height } = props;

  const colDefs = useMemo(() => toAgColumns(columns), [columns]);
  // AG's client-side model owns its row array; hand it a stable copy keyed by id.
  const rowData = useMemo(() => [...rows], [rows]);

  function handleCellValueChanged(event: CellValueChangedEvent<SpikeBookRow>): void {
    const key = event.colDef.field;
    if (key === undefined || !isColumnKey(key)) return;
    const column = columns.find((c) => c.key === key);
    if (column === undefined) return;
    onCellEdit({ rowId: event.data.id, columnKey: key, value: column.accessor(event.data) });
  }

  function handleCellFocused(event: CellFocusedEvent<SpikeBookRow>): void {
    if (event.rowIndex === null || event.column === null) return;
    const key = typeof event.column === "string" ? event.column : event.column.getColId();
    if (isColumnKey(key)) onCellFocus({ rowIdx: event.rowIndex, columnKey: key });
  }

  const wrapperClass = className === undefined ? "spike-grid-ag" : `spike-grid-ag ${className}`;

  return (
    // Height is a dynamic, prop-driven scroll viewport (cardinal-rule exception).
    <div className={wrapperClass} style={{ height }}>
      <AgGridReact<SpikeBookRow>
        theme={SPIKE_THEME}
        columnDefs={colDefs}
        rowData={rowData}
        getRowId={(params) => params.data.id}
        onCellValueChanged={handleCellValueChanged}
        onCellFocused={handleCellFocused}
        rowHeight={ROW_HEIGHT}
        headerHeight={HEADER_HEIGHT}
      />
    </div>
  );
}
