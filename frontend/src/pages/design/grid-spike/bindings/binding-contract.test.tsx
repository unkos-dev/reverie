/**
 * Adapter contract, executed against the bindings (design "Testing").
 *
 * jsdom has no layout engine, so the two grids virtualize differently against a
 * zero-size viewport: RDG renders only the first column and refuses offscreen
 * headers; AG renders all headers but will not open an in-place editor without
 * real geometry. The shared `describe.each` suite therefore asserts only what
 * both grids reliably produce in jsdom (mount, a data window, focus reporting),
 * and each grid gets one deeper test where it tolerates jsdom: AG for the full
 * column mapping, RDG for the edit commit/cancel lifecycle. AG's editor and the
 * grids' virtualized rendering at scale are verified in the browser QA session
 * (design "Testing": no perf/virtualization assertions in CI).
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { describe, expect, test, vi } from "vite-plus/test";

import { SPIKE_COLUMNS } from "../columns";
import { generateRows } from "../data/generator";
import { AgGridBinding } from "./ag-grid-binding";
import { ReactDataGridBinding } from "./react-data-grid-binding";
import { GRID_BINDINGS } from "./registry";
import type { CellEdit, FocusReport, GridBinding, SortState } from "../types";

const ROWS = generateRows(7, 20);

type HarnessProps = {
  binding: GridBinding;
  onCellEdit?: (edit: CellEdit) => void;
  onCellFocus?: (report: FocusReport) => void;
};

function BindingHarness({ binding, onCellEdit, onCellFocus }: HarnessProps): ReactElement {
  const [sort, setSort] = useState<SortState>(null);
  const [rows, setRows] = useState(ROWS);
  const { Component } = binding;
  return (
    <Component
      rows={rows}
      columns={SPIKE_COLUMNS}
      sort={sort}
      onSortChange={setSort}
      onCellEdit={(edit) => {
        setRows((prev) => prev.map((r) => (r.id === edit.rowId ? { ...r, title: edit.value } : r)));
        onCellEdit?.(edit);
      }}
      onCellFocus={(report) => {
        onCellFocus?.(report);
      }}
      height={400}
    />
  );
}

describe.each(GRID_BINDINGS)("GridBindingProps contract — $label", (binding) => {
  test("mounts an ARIA grid", async () => {
    render(<BindingHarness binding={binding} />);
    expect(await screen.findByRole("grid")).toBeInTheDocument();
  });

  test("renders the first column header", async () => {
    render(<BindingHarness binding={binding} />);
    await screen.findByRole("grid");
    const first = SPIKE_COLUMNS[0];
    expect(
      await screen.findByRole("columnheader", { name: new RegExp(first.name) }),
    ).toBeInTheDocument();
  });

  test("renders a data window", async () => {
    render(<BindingHarness binding={binding} />);
    await screen.findByRole("grid");
    expect(await screen.findByText(ROWS[0].title)).toBeInTheDocument();
  });

  test("reports focus when a cell is activated", async () => {
    const onCellFocus = vi.fn();
    const user = userEvent.setup();
    render(<BindingHarness binding={binding} onCellFocus={onCellFocus} />);
    await screen.findByRole("grid");
    const firstTitle = await screen.findByText(ROWS[0].title);
    await user.click(firstTitle);
    await waitFor(() => {
      expect(onCellFocus).toHaveBeenCalled();
    });
  });
});

describe("AG Grid — full column mapping", () => {
  test("renders a header for every shared column", async () => {
    render(
      <BindingHarness
        binding={{ id: "ag-grid", label: "AG Grid Community", Component: AgGridBinding }}
      />,
    );
    await screen.findByRole("grid");
    for (const col of SPIKE_COLUMNS) {
      expect(
        await screen.findByRole("columnheader", { name: new RegExp(col.name) }),
      ).toBeInTheDocument();
    }
  });
});

describe("react-data-grid — edit lifecycle", () => {
  test("commits an edit on Enter and discards on Escape", async () => {
    const onCellEdit = vi.fn();
    const user = userEvent.setup();
    render(
      <BindingHarness
        binding={{
          id: "react-data-grid",
          label: "react-data-grid",
          Component: ReactDataGridBinding,
        }}
        onCellEdit={onCellEdit}
      />,
    );
    await screen.findByRole("grid");

    const cell = await screen.findByText(ROWS[0].title);
    await user.dblClick(cell);
    const editor = await screen.findByRole("textbox");
    await user.clear(editor);
    await user.type(editor, "Edited Title{Enter}");
    await waitFor(() => {
      expect(onCellEdit).toHaveBeenCalledWith(expect.objectContaining({ value: "Edited Title" }));
    });

    onCellEdit.mockClear();
    const grid = await screen.findByRole("grid");
    const editedCell = await within(grid).findByText("Edited Title");
    await user.dblClick(editedCell);
    const editor2 = await screen.findByRole("textbox");
    await user.type(editor2, "Discarded{Escape}");
    expect(onCellEdit).not.toHaveBeenCalled();
  });
});
