import type * as DataGridModule from "react-data-grid";
import { render } from "vitest-browser-react";
import { expect, test, vi } from "vite-plus/test";
import { ReactDataGridBinding } from "@/lib/grid/ReactDataGridBinding";

type Row = { id: string; title: string };
type RowsChange = (
  rows: readonly Row[],
  data: { indexes: number[]; column: { key: string } },
) => void;
const seam = vi.hoisted(() => ({ onRowsChange: undefined as RowsChange | undefined }));

vi.mock("react-data-grid", async (importOriginal) => {
  const original = await importOriginal<typeof DataGridModule>();
  return {
    ...original,
    DataGrid: (props: { onRowsChange: RowsChange }) => {
      seam.onRowsChange = props.onRowsChange;
      return null;
    },
  };
});

test("a multi-index rows-change (fill/paste) never fires onCellEdit", async () => {
  const onCellEdit = vi.fn();
  const rows = [
    { id: "first", title: "First" },
    { id: "second", title: "Second" },
  ];
  await render(
    <ReactDataGridBinding<Row>
      rows={rows}
      columns={[{ key: "title", name: "Title", sortable: false, accessor: (row) => row.title }]}
      rowKey={(row) => row.id}
      sort={[]}
      onSortChange={() => {}}
      onCellFocus={() => {}}
      onCellEdit={onCellEdit}
      height={400}
    />,
  );
  const onRowsChange = seam.onRowsChange;
  if (onRowsChange === undefined)
    throw new Error("Binding did not supply its rows-change callback");
  const updated = [
    { id: "first", title: "Edited first" },
    { id: "second", title: "Edited second" },
  ];
  onRowsChange(updated, { indexes: [0, 1], column: { key: "title" } });
  expect(onCellEdit).not.toHaveBeenCalled();
  onRowsChange(updated, { indexes: [0], column: { key: "title" } });
  expect(onCellEdit).toHaveBeenCalledExactlyOnceWith({
    row: updated[0],
    previousRow: rows[0],
    columnKey: "title",
  });
});
