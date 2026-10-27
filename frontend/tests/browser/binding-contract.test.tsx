import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { useState, type ReactElement, type UIEvent } from "react";
import { describe, expect, test, vi } from "vite-plus/test";

import { ReactDataGridBinding } from "@/lib/grid/ReactDataGridBinding";
import type {
  CellEditReport,
  FocusReport,
  GridColumn,
  GridEditorProps,
  SortState,
} from "@/lib/grid/types";

type TestRow = { id: string; title: string; author: string; status: string };

const ROWS: readonly TestRow[] = Array.from({ length: 20 }, (_, index) => ({
  id: `row-${String(index)}`,
  title: `Title ${String(index)}`,
  author: `Author ${String(index)}`,

  status: `status-${String(index)}`,
}));

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
    await render(<Harness />);
    await expect.element(page.getByRole("grid")).toBeInTheDocument();
  });

  test("renders column headers", async () => {
    await render(<Harness />);
    page.getByRole("grid");
    await expect.element(page.getByRole("columnheader", { name: "Title" })).toBeInTheDocument();
    await expect.element(page.getByRole("columnheader", { name: "Author" })).toBeInTheDocument();
  });

  test("fires onSortChange when a sortable header is clicked", async () => {
    const onSortChange = vi.fn();
    const user = userEvent;
    await render(<Harness onSortChange={onSortChange} />);
    const header = page.getByRole("columnheader", { name: "Title" });
    await user.click(header);
    await vi.waitFor(() => {
      expect(onSortChange).toHaveBeenCalledWith([{ columnKey: "title", direction: "asc" }]);
    });
  });

  test("ctrl-click on a second header appends a level in insertion order", async () => {
    const onSortChange = vi.fn();
    const user = userEvent;
    await render(<Harness onSortChange={onSortChange} />);
    const titleHeader = page.getByRole("columnheader", { name: "Title" });
    await user.click(titleHeader);
    const authorHeader = page.getByRole("columnheader", { name: "Author" });
    await user.keyboard("{Control>}");
    await user.click(authorHeader);
    await user.keyboard("{/Control}");
    await vi.waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([
        { columnKey: "title", direction: "asc" },
        { columnKey: "author", direction: "asc" },
      ]);
    });
  });

  test("a plain click mid-stack collapses the stack to that single column", async () => {
    const onSortChange = vi.fn();
    const user = userEvent;
    await render(<Harness onSortChange={onSortChange} />);
    const titleHeader = page.getByRole("columnheader", { name: "Title" });
    await user.click(titleHeader);
    const authorHeader = page.getByRole("columnheader", { name: "Author" });
    await user.keyboard("{Control>}");
    await user.click(authorHeader);
    await user.keyboard("{/Control}");
    await vi.waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([
        { columnKey: "title", direction: "asc" },
        { columnKey: "author", direction: "asc" },
      ]);
    });

    await user.click(authorHeader);
    await vi.waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([{ columnKey: "author", direction: "desc" }]);
    });
  });

  test("a sort-priority indicator exposes an accessible sort-level label", async () => {
    const user = userEvent;
    await render(<Harness />);
    const titleHeader = page.getByRole("columnheader", { name: "Title" });
    await user.click(titleHeader);
    const authorHeader = page.getByRole("columnheader", { name: "Author" });
    await user.keyboard("{Control>}");
    await user.click(authorHeader);
    await user.keyboard("{/Control}");
    await expect.element(page.getByText("sort level 2", { exact: false })).toBeInTheDocument();
  });

  test("reports focus when a cell is activated", async () => {
    const onCellFocus = vi.fn();
    const user = userEvent;
    await render(<Harness onCellFocus={onCellFocus} />);
    const firstTitle = page.getByText(ROWS[0].title);
    await user.click(firstTitle);
    await vi.waitFor(() => {
      expect(onCellFocus).toHaveBeenCalledWith({ row: ROWS[0], rowIdx: 0, columnKey: "title" });
    });
  });

  test("third header activation clears the sort back to an empty stack", async () => {
    const onSortChange = vi.fn();
    const user = userEvent;
    await render(<Harness onSortChange={onSortChange} />);
    const header = page.getByRole("columnheader", { name: "Title" });
    await user.click(header);
    await user.click(header);
    await user.click(header);
    await vi.waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([]);
    });
  });

  test("forwards native scroll events via onScroll", async () => {
    const onScroll = vi.fn();
    await render(<Harness onScroll={onScroll} />);
    const grid = page.getByRole("grid");
    grid.element().scrollTo({ top: grid.element().scrollHeight });
    await expect.poll(() => onScroll.mock.calls.length).toBeGreaterThan(0);
    expect(onScroll).toHaveBeenCalled();
  });
});

describe("ReactDataGridBinding edit surface", () => {
  test("opens an editor on Enter for an editable column", async () => {
    const user = userEvent;
    await render(<Harness />);
    const cell = page.getByText(ROWS[0].title);
    await user.click(cell);
    await user.keyboard("{Enter}");
    await expect.element(page.getByRole("textbox", { name: "Title editor" })).toBeInTheDocument();
  });

  test("opens an editor when typing a printable key", async () => {
    const user = userEvent;
    await render(<Harness />);
    const cell = page.getByText(ROWS[0].title);
    await user.click(cell);
    await user.keyboard("x");
    await expect.element(page.getByRole("textbox", { name: "Title editor" })).toBeInTheDocument();
  });

  test("update then Enter commits the draft and reports both row snapshots", async () => {
    const onCellEdit = vi.fn();
    const user = userEvent;
    await render(<Harness onCellEdit={onCellEdit} />);
    const cell = page.getByText(ROWS[0].title);
    await user.click(cell);
    await user.keyboard("{Enter}");
    const input = page.getByRole("textbox", { name: "Title editor" });
    await user.clear(input);
    await user.type(input, "Edited Title");
    await user.keyboard("{Enter}");
    await vi.waitFor(() => {
      expect(onCellEdit).toHaveBeenCalledWith({
        row: { ...ROWS[0], title: "Edited Title" },
        previousRow: ROWS[0],
        columnKey: "title",
      });
    });
  });

  test("commit fires onCellEdit immediately, without a separate Enter", async () => {
    const onCellEdit = vi.fn();
    const user = userEvent;
    await render(<Harness onCellEdit={onCellEdit} />);
    const cell = page.getByText(ROWS[2].status);
    await user.click(cell);
    await user.keyboard("{Enter}");
    const select = page.getByRole("combobox", { name: "Status editor" });
    await user.selectOptions(select, "read");
    await vi.waitFor(() => {
      expect(onCellEdit).toHaveBeenCalledWith({
        row: { ...ROWS[2], status: "read" },
        previousRow: ROWS[2],
        columnKey: "status",
      });
    });
  });

  test("Escape discards the draft without firing onCellEdit", async () => {
    const onCellEdit = vi.fn();
    const user = userEvent;
    await render(<Harness onCellEdit={onCellEdit} />);
    const cell = page.getByText(ROWS[1].title);
    await user.click(cell);
    await user.keyboard("{Enter}");
    const input = page.getByRole("textbox", { name: "Title editor" });
    await user.type(input, "Should not commit");
    await user.keyboard("{Escape}");
    await expect
      .element(page.getByRole("textbox", { name: "Title editor" }))
      .not.toBeInTheDocument();
    expect(onCellEdit).not.toHaveBeenCalled();
  });

  test("a column without renderEditCell never opens an editor", async () => {
    const user = userEvent;
    await render(<Harness />);
    const cell = page.getByText(ROWS[0].author);
    await user.click(cell);
    await user.keyboard("{Enter}");
    await expect.element(page.getByRole("textbox")).not.toBeInTheDocument();
  });

  test("editable: () => false blocks opening the editor", async () => {
    const user = userEvent;
    await render(<Harness />);
    const cell = page.getByText(`${ROWS[0].title} [locked]`);
    await user.click(cell);
    await user.keyboard("{Enter}");
    await expect
      .element(page.getByRole("textbox", { name: "Title editor" }))
      .not.toBeInTheDocument();
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
    await render(<SelectionHarness />);
    page.getByRole("grid");
    await expect
      .element(page.getByRole("checkbox", { name: "Select all loaded books" }))
      .toBeInTheDocument();
  });

  test("marks the grid multiselectable and reports row selection changes", async () => {
    const onSelectionChange = vi.fn();
    const user = userEvent;
    await render(<SelectionHarness onSelectionChange={onSelectionChange} />);
    const grid = page.getByRole("grid");
    await expect.element(grid).toHaveAttribute("aria-multiselectable", "true");
    const rowChecks = page.getByRole("checkbox", { name: "Select", exact: true }).elements();
    await user.click(rowChecks[0]);
    await vi.waitFor(() => {
      expect(onSelectionChange).toHaveBeenCalledWith(new Set(["row-0"]));
    });
    const selectedRow = page
      .getByRole("row")
      .elements()
      .find((row) => row.getAttribute("aria-selected") === "true");
    expect(selectedRow).toBeDefined();
  });

  test("select-all header checkbox selects every loaded row", async () => {
    const onSelectionChange = vi.fn();
    const user = userEvent;
    await render(<SelectionHarness onSelectionChange={onSelectionChange} />);
    page.getByRole("grid");
    await user.click(page.getByRole("checkbox", { name: "Select all loaded books" }));
    await vi.waitFor(() => {
      expect(onSelectionChange).toHaveBeenCalledWith(new Set(ROWS.map((row) => row.id)));
    });
  });

  test("clicking a read-only cell activates the row", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent;
    await render(<SelectionHarness onRowActivate={onRowActivate} />);
    page.getByRole("grid");
    await user.click(page.getByText("Author 3"));
    expect(onRowActivate).toHaveBeenCalledTimes(1);
    expect(onRowActivate).toHaveBeenCalledWith(ROWS[3]);
  });

  test("clicking an editable cell selects it without activating the row", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent;
    await render(<SelectionHarness onRowActivate={onRowActivate} />);
    page.getByRole("grid");
    await user.click(page.getByText("Title 2"));
    expect(onRowActivate).not.toHaveBeenCalled();
  });

  test("clicking the selection checkbox does not activate the row", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent;
    await render(<SelectionHarness onRowActivate={onRowActivate} />);
    page.getByRole("grid");
    await user.click(page.getByRole("checkbox", { name: "Select", exact: true }).elements()[1]);
    expect(onRowActivate).not.toHaveBeenCalled();
  });

  test("Enter on a read-only cell activates the row", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent;
    await render(<SelectionHarness onRowActivate={onRowActivate} />);
    page.getByRole("grid");
    await user.click(page.getByText("Author 5"));
    onRowActivate.mockClear();
    await user.keyboard("{Enter}");
    expect(onRowActivate).toHaveBeenCalledWith(ROWS[5]);
  });

  test("Enter on an editable cell opens the editor, not the drawer", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent;
    await render(<SelectionHarness onRowActivate={onRowActivate} />);
    page.getByRole("grid");
    await user.click(page.getByText("Title 4"));
    await user.keyboard("{Enter}");
    await expect.element(page.getByLabelText("Title editor")).toBeInTheDocument();
    expect(onRowActivate).not.toHaveBeenCalled();
  });

  test("an editor-bearing column never activates, even when the row is locked", async () => {
    const onRowActivate = vi.fn();
    const user = userEvent;
    await render(<SelectionHarness onRowActivate={onRowActivate} />);
    page.getByRole("grid");
    await user.click(page.getByText("Title 6 [locked]"));
    expect(onRowActivate).not.toHaveBeenCalled();
  });
});
