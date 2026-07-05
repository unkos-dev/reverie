import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement, type UIEvent } from "react";
import { describe, expect, test, vi } from "vite-plus/test";

import { ReactDataGridBinding } from "./ReactDataGridBinding";
import type { FocusReport, GridColumn, SortState } from "./types";

type TestRow = { id: string; title: string; author: string };

const ROWS: readonly TestRow[] = Array.from({ length: 20 }, (_, index) => ({
  id: `row-${String(index)}`,
  title: `Title ${String(index)}`,
  author: `Author ${String(index)}`,
}));

const COLUMNS: readonly GridColumn<TestRow>[] = [
  { key: "title", name: "Title", sortable: true, width: 200, accessor: (row) => row.title },
  { key: "author", name: "Author", sortable: true, width: 160, accessor: (row) => row.author },
];

type HarnessProps = {
  onSortChange?: (sort: SortState) => void;
  onCellFocus?: (report: FocusReport<TestRow>) => void;
  onScroll?: (event: UIEvent<HTMLDivElement>) => void;
};

function Harness({ onSortChange, onCellFocus, onScroll }: HarnessProps): ReactElement {
  const [sort, setSort] = useState<SortState>(null);
  return (
    <ReactDataGridBinding<TestRow>
      rows={ROWS}
      columns={COLUMNS}
      rowKey={(row) => row.id}
      sort={sort}
      onSortChange={(next) => {
        setSort(next);
        onSortChange?.(next);
      }}
      onCellFocus={(report) => {
        onCellFocus?.(report);
      }}
      onScroll={onScroll}
      height={400}
    />
  );
}

describe("ReactDataGridBinding contract", () => {
  test("mounts an ARIA grid", async () => {
    render(<Harness />);
    expect(await screen.findByRole("grid")).toBeInTheDocument();
  });

  test("renders column headers", async () => {
    render(<Harness />);
    await screen.findByRole("grid");
    expect(await screen.findByRole("columnheader", { name: "Title" })).toBeInTheDocument();
    expect(await screen.findByRole("columnheader", { name: "Author" })).toBeInTheDocument();
  });

  test("fires onSortChange when a sortable header is clicked", async () => {
    const onSortChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onSortChange={onSortChange} />);
    const header = await screen.findByRole("columnheader", { name: "Title" });
    await user.click(header);
    await waitFor(() => {
      expect(onSortChange).toHaveBeenCalledWith({ columnKey: "title", direction: "asc" });
    });
  });

  test("reports focus when a cell is activated", async () => {
    const onCellFocus = vi.fn();
    const user = userEvent.setup();
    render(<Harness onCellFocus={onCellFocus} />);
    const firstTitle = await screen.findByText(ROWS[0].title);
    await user.click(firstTitle);
    await waitFor(() => {
      expect(onCellFocus).toHaveBeenCalledWith({ row: ROWS[0], rowIdx: 0, columnKey: "title" });
    });
  });

  test("third header activation clears the sort back to null", async () => {
    const onSortChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onSortChange={onSortChange} />);
    const header = await screen.findByRole("columnheader", { name: "Title" });
    await user.click(header);
    await user.click(header);
    await user.click(header);
    await waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith(null);
    });
  });

  test("forwards native scroll events via onScroll", async () => {
    const onScroll = vi.fn();
    render(<Harness onScroll={onScroll} />);
    const grid = await screen.findByRole("grid");
    fireEvent.scroll(grid);
    expect(onScroll).toHaveBeenCalled();
  });
});
