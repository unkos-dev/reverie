import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement, type UIEvent } from "react";
import { describe, expect, test, vi } from "vite-plus/test";

import { ReactDataGridBinding } from "./ReactDataGridBinding";
import type { CellEditReport, FocusReport, GridColumn, GridEditorProps, SortState } from "./types";

type TestRow = { id: string; title: string; author: string; status: string };

const ROWS: readonly TestRow[] = Array.from({ length: 20 }, (_, index) => ({
  id: `row-${String(index)}`,
  title: `Title ${String(index)}`,
  author: `Author ${String(index)}`,
  // Unique per row (rather than a shared "reading") so tests can locate one
  // row's status cell unambiguously with findByText.
  status: `status-${String(index)}`,
}));

/** Uncontrolled text editor: stages a draft on every keystroke via `update`. */
function TitleEditor({ row, update }: GridEditorProps<TestRow>): ReactElement {
  return (
    <input
      aria-label="Title editor"
      defaultValue={row.title}
      onChange={(event) => {
        update({ ...row, title: event.target.value });
      }}
    />
  );
}

/** Select-and-done editor: commits immediately on change, mirroring StatusCellEditor. */
function StatusEditor({ row, commit }: GridEditorProps<TestRow>): ReactElement {
  return (
    <select
      aria-label="Status editor"
      defaultValue={row.status}
      onChange={(event) => {
        commit({ ...row, status: event.target.value });
      }}
    >
      <option value="reading">reading</option>
      <option value="read">read</option>
    </select>
  );
}

const COLUMNS: readonly GridColumn<TestRow>[] = [
  {
    key: "title",
    name: "Title",
    sortable: true,
    width: 200,
    accessor: (row) => row.title,
    renderEditCell: TitleEditor,
  },
  { key: "author", name: "Author", sortable: true, width: 160, accessor: (row) => row.author },
  {
    key: "status",
    name: "Status",
    sortable: false,
    width: 120,
    accessor: (row) => row.status,
    renderEditCell: StatusEditor,
  },
  {
    key: "locked",
    name: "Locked Title",
    sortable: false,
    width: 200,
    accessor: (row) => `${row.title} [locked]`,
    renderEditCell: TitleEditor,
    editable: () => false,
  },
];

type HarnessProps = {
  onSortChange?: (sort: SortState) => void;
  onCellFocus?: (report: FocusReport<TestRow>) => void;
  onCellEdit?: (report: CellEditReport<TestRow>) => void;
  onScroll?: (event: UIEvent<HTMLDivElement>) => void;
};

function Harness({ onSortChange, onCellFocus, onCellEdit, onScroll }: HarnessProps): ReactElement {
  const [sort, setSort] = useState<SortState>([]);
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
      onCellEdit={onCellEdit}
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
      expect(onSortChange).toHaveBeenCalledWith([{ columnKey: "title", direction: "asc" }]);
    });
  });

  test("ctrl-click on a second header appends a level in insertion order", async () => {
    const onSortChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onSortChange={onSortChange} />);
    const titleHeader = await screen.findByRole("columnheader", { name: "Title" });
    await user.click(titleHeader);
    const authorHeader = await screen.findByRole("columnheader", { name: "Author" });
    await user.keyboard("{Control>}");
    await user.click(authorHeader);
    await user.keyboard("{/Control}");
    await waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([
        { columnKey: "title", direction: "asc" },
        { columnKey: "author", direction: "asc" },
      ]);
    });
  });

  test("a plain click mid-stack collapses the stack to that single column", async () => {
    const onSortChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onSortChange={onSortChange} />);
    const titleHeader = await screen.findByRole("columnheader", { name: "Title" });
    await user.click(titleHeader);
    const authorHeader = await screen.findByRole("columnheader", { name: "Author" });
    await user.keyboard("{Control>}");
    await user.click(authorHeader);
    await user.keyboard("{/Control}");
    await waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([
        { columnKey: "title", direction: "asc" },
        { columnKey: "author", direction: "asc" },
      ]);
    });
    // A subsequent plain click (no modifier) documents RDG's own behavior at
    // this contract seam: it silently drops the rest of the stack (advancing
    // the clicked column's asc/desc cycle) rather than just reordering the
    // clicked column to the front.
    await user.click(authorHeader);
    await waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([{ columnKey: "author", direction: "desc" }]);
    });
  });

  test("a sort-priority indicator exposes an accessible sort-level label", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const titleHeader = await screen.findByRole("columnheader", { name: "Title" });
    await user.click(titleHeader);
    const authorHeader = await screen.findByRole("columnheader", { name: "Author" });
    await user.keyboard("{Control>}");
    await user.click(authorHeader);
    await user.keyboard("{/Control}");
    expect(await screen.findByText("sort level 2", { exact: false })).toBeInTheDocument();
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

  test("third header activation clears the sort back to an empty stack", async () => {
    const onSortChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onSortChange={onSortChange} />);
    const header = await screen.findByRole("columnheader", { name: "Title" });
    await user.click(header);
    await user.click(header);
    await user.click(header);
    await waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([]);
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

describe("ReactDataGridBinding edit surface", () => {
  test("opens an editor on Enter for an editable column", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const cell = await screen.findByText(ROWS[0].title);
    await user.click(cell);
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("textbox", { name: "Title editor" })).toBeInTheDocument();
  });

  test("opens an editor when typing a printable key", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const cell = await screen.findByText(ROWS[0].title);
    await user.click(cell);
    await user.keyboard("x");
    expect(await screen.findByRole("textbox", { name: "Title editor" })).toBeInTheDocument();
  });

  test("update then Enter commits the draft and reports both row snapshots", async () => {
    const onCellEdit = vi.fn();
    const user = userEvent.setup();
    render(<Harness onCellEdit={onCellEdit} />);
    const cell = await screen.findByText(ROWS[0].title);
    await user.click(cell);
    await user.keyboard("{Enter}");
    const input = await screen.findByRole("textbox", { name: "Title editor" });
    await user.clear(input);
    await user.type(input, "Edited Title");
    await user.keyboard("{Enter}");
    await waitFor(() => {
      expect(onCellEdit).toHaveBeenCalledWith({
        row: { ...ROWS[0], title: "Edited Title" },
        previousRow: ROWS[0],
        columnKey: "title",
      });
    });
  });

  test("commit fires onCellEdit immediately, without a separate Enter", async () => {
    const onCellEdit = vi.fn();
    const user = userEvent.setup();
    render(<Harness onCellEdit={onCellEdit} />);
    const cell = await screen.findByText(ROWS[2].status);
    await user.click(cell);
    await user.keyboard("{Enter}");
    const select = await screen.findByRole("combobox", { name: "Status editor" });
    await user.selectOptions(select, "read");
    await waitFor(() => {
      expect(onCellEdit).toHaveBeenCalledWith({
        row: { ...ROWS[2], status: "read" },
        previousRow: ROWS[2],
        columnKey: "status",
      });
    });
  });

  test("Escape discards the draft without firing onCellEdit", async () => {
    const onCellEdit = vi.fn();
    const user = userEvent.setup();
    render(<Harness onCellEdit={onCellEdit} />);
    const cell = await screen.findByText(ROWS[1].title);
    await user.click(cell);
    await user.keyboard("{Enter}");
    const input = await screen.findByRole("textbox", { name: "Title editor" });
    await user.type(input, "Should not commit");
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("textbox", { name: "Title editor" })).not.toBeInTheDocument();
    });
    expect(onCellEdit).not.toHaveBeenCalled();
  });

  test("a column without renderEditCell never opens an editor", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const cell = await screen.findByText(ROWS[0].author);
    await user.click(cell);
    await user.keyboard("{Enter}");
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  test("editable: () => false blocks opening the editor", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const cell = await screen.findByText(`${ROWS[0].title} [locked]`);
    await user.click(cell);
    await user.keyboard("{Enter}");
    expect(screen.queryByRole("textbox", { name: "Title editor" })).not.toBeInTheDocument();
  });

  test("a multi-index rows-change (fill/paste) never fires onCellEdit", async () => {
    const onCellEdit = vi.fn();
    let capturedOnRowsChange:
      | ((rows: readonly TestRow[], data: { indexes: number[]; column: { key: string } }) => void)
      | undefined;

    // RDG's own fill-drag and multi-cell paste interactions aren't
    // reproducible through jsdom pointer events, so this seam is exercised
    // by swapping in a stub DataGrid that hands back the exact
    // `onRowsChange` callback the binding wires up, then invoking it
    // directly with a synthetic multi-index event.
    vi.doMock("react-data-grid", () => ({
      DataGrid: (props: {
        onRowsChange?: (
          rows: readonly TestRow[],
          data: { indexes: number[]; column: { key: string } },
        ) => void;
      }) => {
        capturedOnRowsChange = props.onRowsChange;
        return null;
      },
    }));
    vi.resetModules();
    const { ReactDataGridBinding: MockedBinding } = await import("./ReactDataGridBinding");

    render(
      <MockedBinding<TestRow>
        rows={ROWS}
        columns={COLUMNS}
        rowKey={(row) => row.id}
        sort={[]}
        onSortChange={() => undefined}
        onCellFocus={() => undefined}
        onCellEdit={onCellEdit}
        height={400}
      />,
    );

    if (capturedOnRowsChange === undefined) {
      throw new Error("expected the stub DataGrid to receive onRowsChange");
    }
    capturedOnRowsChange([...ROWS], { indexes: [0, 1], column: { key: "title" } });

    expect(onCellEdit).not.toHaveBeenCalled();

    vi.doUnmock("react-data-grid");
    vi.resetModules();
  });
});

type SelectionHarnessProps = {
  onRowActivate?: (row: TestRow) => void;
  onSelectionChange?: (keys: ReadonlySet<string>) => void;
};

function SelectionHarness({
  onRowActivate,
  onSelectionChange,
}: SelectionHarnessProps): ReactElement {
  const [sort, setSort] = useState<SortState>([]);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  return (
    <ReactDataGridBinding<TestRow>
      rows={ROWS}
      columns={COLUMNS}
      rowKey={(row) => row.id}
      sort={sort}
      onSortChange={setSort}
      onCellFocus={() => undefined}
      onRowActivate={onRowActivate}
      selection={{
        selectedKeys: selected,
        onSelectionChange: (keys) => {
          setSelected(keys);
          onSelectionChange?.(keys);
        },
        selectAllLabel: "Select all loaded books",
      }}
      height={400}
    />
  );
}

describe("ReactDataGridBinding selection and activation", () => {
  test("renders a selection column with the caller's select-all label", async () => {
    render(<SelectionHarness />);
    await screen.findByRole("grid");
    expect(screen.getByRole("checkbox", { name: "Select all loaded books" })).toBeInTheDocument();
  });

  test("marks the grid multiselectable and reports row selection changes", async () => {
    const onSelectionChange = vi.fn();
    const user = userEvent.setup();
    render(<SelectionHarness onSelectionChange={onSelectionChange} />);
    const grid = await screen.findByRole("grid");
    expect(grid).toHaveAttribute("aria-multiselectable", "true");
    const rowChecks = screen.getAllByRole("checkbox", { name: "Select" });
    await user.click(rowChecks[0]);
    await waitFor(() => {
      expect(onSelectionChange).toHaveBeenCalledWith(new Set(["row-0"]));
    });
    const selectedRow = screen
      .getAllByRole("row")
      .find((row) => row.getAttribute("aria-selected") === "true");
    expect(selectedRow).toBeDefined();
  });

  test("select-all header checkbox selects every loaded row", async () => {
    const onSelectionChange = vi.fn();
    const user = userEvent.setup();
    render(<SelectionHarness onSelectionChange={onSelectionChange} />);
    await screen.findByRole("grid");
    await user.click(screen.getByRole("checkbox", { name: "Select all loaded books" }));
    await waitFor(() => {
      expect(onSelectionChange).toHaveBeenCalledWith(new Set(ROWS.map((row) => row.id)));
    });
  });

  test("clicking a read-only cell activates the row", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent.setup();
    render(<SelectionHarness onRowActivate={onRowActivate} />);
    await screen.findByRole("grid");
    await user.click(screen.getByText("Author 3"));
    expect(onRowActivate).toHaveBeenCalledTimes(1);
    expect(onRowActivate).toHaveBeenCalledWith(ROWS[3]);
  });

  test("clicking an editable cell selects it without activating the row", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent.setup();
    render(<SelectionHarness onRowActivate={onRowActivate} />);
    await screen.findByRole("grid");
    await user.click(screen.getByText("Title 2"));
    expect(onRowActivate).not.toHaveBeenCalled();
  });

  test("clicking the selection checkbox does not activate the row", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent.setup();
    render(<SelectionHarness onRowActivate={onRowActivate} />);
    await screen.findByRole("grid");
    await user.click(screen.getAllByRole("checkbox", { name: "Select" })[1]);
    expect(onRowActivate).not.toHaveBeenCalled();
  });

  test("Enter on a read-only cell activates the row", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent.setup();
    render(<SelectionHarness onRowActivate={onRowActivate} />);
    await screen.findByRole("grid");
    await user.click(screen.getByText("Author 5"));
    onRowActivate.mockClear();
    await user.keyboard("{Enter}");
    expect(onRowActivate).toHaveBeenCalledWith(ROWS[5]);
  });

  test("Enter on an editable cell opens the editor, not the drawer", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent.setup();
    render(<SelectionHarness onRowActivate={onRowActivate} />);
    await screen.findByRole("grid");
    await user.click(screen.getByText("Title 4"));
    await user.keyboard("{Enter}");
    expect(await screen.findByLabelText("Title editor")).toBeInTheDocument();
    expect(onRowActivate).not.toHaveBeenCalled();
  });

  test("a disabled-editable cell behaves as read-only for activation", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent.setup();
    render(<SelectionHarness onRowActivate={onRowActivate} />);
    await screen.findByRole("grid");
    await user.click(screen.getByText("Title 6 [locked]"));
    expect(onRowActivate).toHaveBeenCalledWith(ROWS[6]);
  });
});
