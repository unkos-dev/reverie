/**
 * The two bake-off candidates behind the shared contract. The harness and the
 * contract test both iterate this list, so adding a candidate is one entry.
 */
import type { GridBinding } from "../types";
import { AgGridBinding } from "./ag-grid-binding";
import { ReactDataGridBinding } from "./react-data-grid-binding";

export const GRID_BINDINGS: readonly GridBinding[] = [
  { id: "react-data-grid", label: "react-data-grid", Component: ReactDataGridBinding },
  { id: "ag-grid", label: "AG Grid Community", Component: AgGridBinding },
];
