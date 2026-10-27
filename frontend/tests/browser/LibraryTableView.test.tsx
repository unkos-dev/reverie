import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { useState, type ComponentProps, type ReactElement } from "react";

import type { BookListItem, SortLevelParam } from "@/api";
import { getBookMetadata } from "@/api/books";
import { getReadingState, updateReadingState } from "@/api/reading";
import { queryKeys } from "@/lib/query/keys";

import { LibraryTableView } from "@/pages/library/table/LibraryTableView";

vi.mock("@/api/books", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, getBookMetadata: vi.fn(), updateBookMetadata: vi.fn() };
});
vi.mock("@/api/reading", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, getReadingState: vi.fn(), updateReadingState: vi.fn() };
});
vi.mock("@/api/metadata", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, revertField: vi.fn() };
});
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getBookMetadata).mockResolvedValue({
    title: "Book Title",
    subtitle: null,
    description: null,
    language: null,
    publisher: null,
    pub_date: null,
    isbn_10: null,
    isbn_13: null,
    pages: null,
    content_rating: null,
    genres: [],
    moods: [],
    tags: [],
    contributors: { author: [], editor: [], translator: [] },
    work_identifiers: {},
    manifestation_identifiers: {},
  });
  vi.mocked(getReadingState).mockResolvedValue({
    status: null,
    rating: null,
    notes: null,
    progress_pct: null,
    started_at: null,
    finished_at: null,
    last_read_at: null,
  });
});

type TableProps = ComponentProps<typeof LibraryTableView>;

function rowFixture(index: number, overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    id: `book-${String(index)}`,
    work_id: `work-${String(index)}`,
    title: `Book Title ${String(index)}`,
    subtitle: `Subtitle ${String(index)}`,
    authors: [`Author ${String(index)}`],
    contributors: [],
    series: null,
    isbn_13: `9780000${String(index).padStart(6, "0")}`,
    pages: 100 + index,
    tags: [],
    genres: [],
    moods: [],
    content_rating: null,
    cover_url: "",
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    reading_state: null,
    created_at: `2026-01-${String((index % 28) + 1).padStart(2, "0")}T00:00:00Z`,
    ...overrides,
  };
}

const ROWS: BookListItem[] = Array.from({ length: 30 }, (_, index) =>
  index === 0
    ? rowFixture(index, { subtitle: null, isbn_13: null, pages: null, reading_state: null })
    : rowFixture(index),
);

function TableHarness({ sort, onSortChange, ...rest }: TableProps): ReactElement {
  const [stack, setStack] = useState<readonly SortLevelParam[]>(sort);
  return (
    <LibraryTableView
      {...rest}
      sort={stack}
      onSortChange={(next) => {
        setStack(next);
        onSortChange(next);
      }}
    />
  );
}

async function renderTableView(overrides: Partial<TableProps> = {}): Promise<TableProps> {
  const props: TableProps = {
    items: ROWS,
    sort: [],
    onSortChange: vi.fn(),
    hasNextPage: false,
    isFetchingNextPage: false,
    isFetchNextPageError: false,
    onLoadMore: vi.fn(),
    listQueryKey: queryKeys.books.list({}),
    density: "comfortable",
    hiddenColumns: new Set<string>(),
    selectedIds: new Set<string>(),
    onSelectionChange: vi.fn(),
    onRowActivate: vi.fn(),
    ...overrides,
  };
  const routes: RouteObject[] = [{ path: "/library", element: <TableHarness {...props} /> }];
  const router = createMemoryRouter(routes, { initialEntries: ["/library"] });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <div data-testid="pilot-container" className="w-[640px]">
          <RouterProvider router={router} />
        </div>
      </QueryClientProvider>
    );
  }
  await render(<Wrapper />);
  return props;
}

describe("LibraryTableView", () => {
  test("mounts an ARIA grid labeled Library books, with the title linking to /b/{id}", async () => {
    await renderTableView();
    const grid = page.getByRole("grid", { name: "Library books" });
    const link = grid.getByRole("link", { name: ROWS[0].title });
    await expect.element(link).toHaveAttribute("href", `/b/${ROWS[0].id}`);
    await page.screenshot({ path: "test-results/browser/library-table.png" });
  });

  test("hosts no filter surface of its own", async () => {
    await renderTableView();
    await expect.element(page.getByTestId("filter-bar")).not.toBeInTheDocument();
    await expect.element(page.getByLabelText("Quick search")).not.toBeInTheDocument();
  });

  test("a row with every nullable field null renders the em-dash placeholder", async () => {
    await renderTableView();
    await expect.element(page.getByRole("grid")).toBeInTheDocument();
    await expect.element(page.getByText("—", { exact: true }).first()).toBeInTheDocument();
  });

  test("clicking the Title header sorts by title", async () => {
    const onSortChange = vi.fn();
    await renderTableView({ onSortChange });
    const user = userEvent;
    const header = page.getByRole("columnheader", { name: "Title (all editions)" });
    await user.click(header);
    expect(onSortChange).toHaveBeenCalledWith([{ field: "title", desc: false }]);
  });

  describe("selection", () => {
    test("renders the loaded-rows select-all label and reports row selection", async () => {
      const onSelectionChange = vi.fn();
      await renderTableView({ onSelectionChange });
      const grid = page.getByRole("grid");
      await expect.element(grid).toHaveAttribute("aria-multiselectable", "true");
      await expect
        .element(page.getByRole("checkbox", { name: "Select all loaded books" }))
        .toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("checkbox", { name: "Select", exact: true }).first());
      expect(onSelectionChange).toHaveBeenCalledWith(new Set([ROWS[0].id]));
    });

    test("select-all covers every loaded row", async () => {
      const onSelectionChange = vi.fn();
      await renderTableView({ onSelectionChange });
      await expect.element(page.getByRole("grid")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("checkbox", { name: "Select all loaded books" }));
      expect(onSelectionChange).toHaveBeenCalledWith(new Set(ROWS.map((row) => row.id)));
    });

    test("selected rows carry aria-selected", async () => {
      await renderTableView({ selectedIds: new Set([ROWS[0].id]) });
      await expect.element(page.getByRole("grid")).toBeInTheDocument();
      await expect.poll(() => page.getByRole("row", { selected: true }).elements()).toHaveLength(1);
    });
  });

  test("the details button opens the drawer for its row", async () => {
    const onRowActivate = vi.fn();
    await renderTableView({ onRowActivate });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: `Open details for ${ROWS[0].title}` }));
    expect(onRowActivate).toHaveBeenCalledTimes(1);
    expect(onRowActivate).toHaveBeenCalledWith(ROWS[0]);
  });
  test("clicking the Authors header sorts by author", async () => {
    const onSortChange = vi.fn();
    await renderTableView({ onSortChange });
    await expect.element(page.getByRole("grid")).toBeInTheDocument();
    const grid = page.getByRole("grid").element();
    expect(grid.scrollWidth).toBeGreaterThan(grid.clientWidth);
    const wideWidth = grid.clientWidth;
    page.getByTestId("pilot-container").element().setAttribute("class", "w-[320px]");
    await expect.poll(() => grid.clientWidth).toBeLessThan(wideWidth);
    const before = page.getByRole("columnheader", { name: "Authors (all editions)" }).query();
    if (before !== null)
      expect(before.getBoundingClientRect().right).toBeGreaterThan(
        grid.getBoundingClientRect().right,
      );
    grid.scrollTo({ left: grid.clientWidth / 2 });
    await expect.poll(() => grid.scrollLeft).toBeGreaterThan(0);
    const header = page.getByRole("columnheader", { name: "Authors (all editions)" });
    await expect
      .poll(() => header.element().getBoundingClientRect().right)
      .toBeLessThanOrEqual(grid.getBoundingClientRect().right);
    await userEvent.click(header);
    expect(onSortChange).toHaveBeenCalledWith([{ field: "author", desc: false }]);
  });

  test.each([
    ["available", true, false, false, 1],
    ["in flight", true, true, false, 0],
    ["no next page", false, false, false, 0],
    ["failed fetch", true, false, true, 0],
  ])(
    "real bottom scrolling respects paging state: %s",
    async (_, hasNextPage, isFetchingNextPage, isFetchNextPageError, calls) => {
      const onLoadMore = vi.fn();
      await renderTableView({ hasNextPage, isFetchingNextPage, isFetchNextPageError, onLoadMore });
      await expect.element(page.getByRole("grid")).toBeInTheDocument();
      const grid = page.getByRole("grid").element();
      expect(grid.scrollHeight).toBeGreaterThan(grid.clientHeight);
      const scrolled = new Promise<void>((resolve) => {
        grid.addEventListener(
          "scroll",
          () => {
            resolve();
          },
          { once: true },
        );
      });
      grid.scrollTo({ top: grid.scrollHeight });
      await scrolled;
      await expect.poll(() => grid.scrollTop).toBeGreaterThan(0);
      await expect
        .poll(() => grid.scrollHeight - grid.clientHeight - grid.scrollTop)
        .toBeLessThanOrEqual(1);
      await expect.element(page.getByRole("link", { name: ROWS[29].title })).toBeInTheDocument();
      expect(onLoadMore).toHaveBeenCalledTimes(calls);
    },
  );

  test("resizing the real container keeps trailing content reachable", async () => {
    const onRowActivate = vi.fn();
    await renderTableView({ onRowActivate });
    await expect.element(page.getByRole("grid")).toBeInTheDocument();
    const grid = page.getByRole("grid").element();
    const initialWidth = grid.clientWidth;
    const title = page.getByRole("columnheader", { name: "Title (all editions)" });
    const initialTitleWidth = title.element().getBoundingClientRect().width;
    const container = page.getByTestId("pilot-container").element();
    container.setAttribute("class", "w-[1100px]");
    await expect.poll(() => grid.clientWidth).toBeGreaterThan(initialWidth);
    await expect
      .poll(() => title.element().getBoundingClientRect().width)
      .toBeGreaterThan(initialTitleWidth);
    await userEvent.click(page.getByRole("button", { name: `Open details for ${ROWS[0].title}` }));
    expect(onRowActivate).toHaveBeenCalledWith(ROWS[0]);
  });
});

function editableRowFixture(): BookListItem {
  return rowFixture(1, {
    series: { id: "s1", name: "Discworld", position: 8 },
    reading_state: {
      status: "want_to_read",
      rating: 3,
      progress_pct: null,
      started_at: null,
      finished_at: null,
    },
  });
}

describe("LibraryTableView", () => {
  test("clicking the Pages header sorts by pages", async () => {
    const onSortChange = vi.fn();
    await renderTableView({
      onSortChange,
      items: [rowFixture(1, { series: { id: "s1", name: "Discworld", position: 8 } })],
    });
    const grid = page.getByRole("grid").element();
    grid.scrollTo({ left: grid.scrollWidth });
    grid.scrollTo({ left: grid.scrollWidth });
    const user = userEvent;
    const header = page.getByRole("grid").getByRole("columnheader", { name: "Pages" });
    await user.click(header);
    expect(onSortChange).toHaveBeenCalledWith([{ field: "pages", desc: false }]);
  });

  test("ctrl-click on a second header appends a level to the sort stack", async () => {
    const onSortChange = vi.fn();
    await renderTableView({
      onSortChange,
      items: [rowFixture(1, { series: { id: "s1", name: "Discworld", position: 8 } })],
    });
    const grid = page.getByRole("grid").element();
    const user = userEvent;
    const titleHeader = page.getByRole("columnheader", { name: "Title (all editions)" });
    await user.click(titleHeader);
    grid.scrollTo({ left: grid.scrollWidth });
    grid.scrollTo({ left: grid.scrollWidth });

    const authorsHeader = page.getByRole("grid").getByRole("columnheader", {
      name: "Authors (all editions)",
    });
    await user.keyboard("{Control>}");
    await user.click(authorsHeader);
    await user.keyboard("{/Control}");

    await vi.waitFor(() => {
      expect(onSortChange).toHaveBeenLastCalledWith([
        { field: "title", desc: false },
        { field: "author", desc: false },
      ]);
    });
  });

  test("exposes an Added column that sorts by created_at", async () => {
    const onSortChange = vi.fn();
    await renderTableView({ onSortChange });
    const grid = page.getByRole("grid").element();

    await expect.element(grid).toHaveAttribute("aria-colcount", "11");
    grid.scrollTo({ left: grid.scrollWidth });
    const user = userEvent;
    const addedHeader = page.getByRole("grid").getByRole("columnheader", { name: "Added" });
    await user.click(addedHeader);
    expect(onSortChange).toHaveBeenCalledWith([{ field: "created_at", desc: false }]);
  });

  test("an initial created_at sort prop marks the Added header as sorted", async () => {
    await renderTableView({ sort: [{ field: "created_at", desc: true }] });
    const grid = page.getByRole("grid").element();
    grid.scrollTo({ left: grid.scrollWidth });
    const addedHeader = page.getByRole("grid").getByRole("columnheader", { name: "Added" });
    await expect.element(addedHeader).toHaveAttribute("aria-sort", "descending");
  });

  test("hosts no sort-chip bar; the rail's sort section owns stack manipulation", async () => {
    await renderTableView({
      sort: [
        { field: "title", desc: false },
        { field: "author", desc: false },
      ],
    });
    await expect.element(page.getByRole("grid")).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: "Move Title later in sort priority" }))
      .not.toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Clear sort" })).not.toBeInTheDocument();
  });

  describe("work-scoped header tooltip", () => {
    test("the info control opens a dismissable tooltip on focus and Esc closes it", async () => {
      await renderTableView();
      await expect.element(page.getByRole("grid")).toBeInTheDocument();
      const user = userEvent;
      const trigger = page.getByRole("button", { name: "(all editions)" }).first();
      await user.tab();
      trigger.element().focus();
      await expect
        .element(page.getByRole("tooltip"))
        .toHaveTextContent("Edits apply to all editions of the work");
      await user.keyboard("{Escape}");
      await expect.element(page.getByRole("tooltip")).not.toBeInTheDocument();
    });

    test("clicking the info control never toggles the column sort", async () => {
      const onSortChange = vi.fn();
      await renderTableView({ onSortChange });
      await expect.element(page.getByRole("grid")).toBeInTheDocument();
      const user = userEvent;
      const trigger = page.getByRole("button", { name: "(all editions)" }).first();
      await user.click(trigger);
      expect(onSortChange).not.toHaveBeenCalled();
    });

    test("keyboard sort works on a tooltip-bearing header", async () => {
      const onSortChange = vi.fn();
      await renderTableView({ onSortChange });
      await expect.element(page.getByRole("grid")).toBeInTheDocument();
      const user = userEvent;

      const subtitleCell = page.getByText(ROWS[1].subtitle ?? "");
      await user.click(subtitleCell);
      await user.keyboard("{ArrowLeft}{ArrowUp}{ArrowUp}");
      await user.keyboard("{Enter}");
      expect(onSortChange).toHaveBeenCalledWith([{ field: "title", desc: false }]);
    });

    test("tooltip-bearing headers expose the note as the column description", async () => {
      await renderTableView();
      const header = page.getByRole("columnheader", { name: "Title (all editions)" });
      const descriptionId = header.element().getAttribute("aria-describedby") ?? "";
      const description = document.getElementById(descriptionId);
      await expect
        .element(description)
        .toHaveTextContent("Edits apply to all editions of the work");
    });
  });

  test("a 4th sorted column is capped to MAX_SORT_LEVELS when applied", async () => {
    const onSortChange = vi.fn();
    await renderTableView({ onSortChange, items: [editableRowFixture()] });
    await userEvent.click(page.getByRole("columnheader", { name: "Title (all editions)" }));
    await userEvent.keyboard("{Control>}");
    await userEvent.click(page.getByRole("columnheader", { name: "Authors (all editions)" }));
    const grid = page.getByRole("grid").element();
    grid.scrollTo({ left: grid.scrollWidth });
    await userEvent.click(page.getByRole("columnheader", { name: "Pages" }));
    await userEvent.click(page.getByRole("columnheader", { name: "Added" }));
    await userEvent.keyboard("{/Control}");
    expect(onSortChange).toHaveBeenLastCalledWith([
      { field: "title", desc: false },
      { field: "author", desc: false },
      { field: "pages", desc: false },
    ]);
  });

  test("shows the end-of-list line once every loaded row has been fetched", async () => {
    await renderTableView({ hasNextPage: false });
    await expect.element(page.getByText(/30 books loaded/)).toBeInTheDocument();
  });

  test("idle state with a next page renders the Load more fallback, which calls onLoadMore", async () => {
    const onLoadMore = vi.fn();
    await renderTableView({ hasNextPage: true, isFetchingNextPage: false, onLoadMore });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Load more" }));
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  test("populated series and status cells render their projections, not placeholders", async () => {
    await renderTableView({
      items: [
        rowFixture(1, {
          series: { id: "s1", name: "Discworld", position: 8 },
          reading_state: {
            status: "want_to_read",
            rating: 3,
            progress_pct: null,
            started_at: null,
            finished_at: null,
          },
        }),
      ],
    });
    const grid = page.getByRole("grid").element();
    grid.scrollTo({ left: grid.scrollWidth });
    await expect.element(page.getByRole("grid").getByText("Discworld · #8")).toBeInTheDocument();
    grid.scrollTo({ left: grid.scrollWidth });
    await expect.element(page.getByRole("grid").getByText("want to read")).toBeInTheDocument();
    await expect.element(page.getByRole("grid").getByText("★★★")).toBeInTheDocument();
  });

  test("a failed next-page fetch renders an alert with a Retry control that calls onLoadMore", async () => {
    const onLoadMore = vi.fn();
    await renderTableView({ isFetchNextPageError: true, onLoadMore });
    const alert = page.getByRole("alert");
    const user = userEvent;
    await user.click(alert.getByRole("button", { name: "Retry" }));
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  test("work-scoped columns carry the tooltip info control; non-scoped columns don't", async () => {
    await renderTableView({
      items: [
        rowFixture(1, {
          series: { id: "s1", name: "Discworld", position: 8 },
          reading_state: {
            status: "want_to_read",
            rating: 3,
            progress_pct: null,
            started_at: null,
            finished_at: null,
          },
        }),
      ],
    });
    const grid = page.getByRole("grid").element();
    const titleHeader = page.getByRole("columnheader", { name: "Title (all editions)" });
    await expect
      .element(titleHeader.getByRole("button", { name: "(all editions)" }))
      .toBeInTheDocument();
    grid.scrollTo({ left: grid.scrollWidth });
    grid.scrollTo({ left: grid.scrollWidth });

    const isbnHeader = page.getByRole("grid").getByRole("columnheader", { name: "ISBN" });
    const pagesHeader = page.getByRole("grid").getByRole("columnheader", { name: "Pages" });
    await expect.element(isbnHeader.getByRole("button")).not.toBeInTheDocument();
    await expect.element(pagesHeader.getByRole("button")).not.toBeInTheDocument();
  });

  describe("cell editing", () => {
    function scrollToRevealRightColumns(grid: Element): void {
      grid.scrollTo({ left: grid.scrollWidth });
    }

    function scrollToRevealTrailingColumns(grid: Element): void {
      grid.scrollTo({ left: grid.scrollWidth });
    }

    test("Enter opens the title text editor prefilled with the current value", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const user = userEvent;

      const subtitleCell = page.getByText(row.subtitle ?? "");
      await user.click(subtitleCell);
      await user.keyboard("{ArrowLeft}");
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("textbox")).toHaveValue(row.title);
    });

    test("entering edit mode on a metadata cell primes the metadata ETag without blocking typing", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const user = userEvent;
      const subtitleCell = page.getByText(row.subtitle ?? "");
      await user.click(subtitleCell);
      await user.keyboard("{Enter}");
      const input = page.getByRole("textbox");

      await user.keyboard("{End}");
      await user.type(input, "z");
      await expect.element(input).toHaveValue(`${row.subtitle ?? ""}z`);
      expect(getBookMetadata).toHaveBeenCalledWith(row.id);
      expect(getReadingState).not.toHaveBeenCalled();
    });

    test("Enter opens the ISBN text editor", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText(row.isbn_13 ?? "");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("textbox")).toHaveValue(row.isbn_13);
    });

    test("Enter opens the pages text editor", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText(String(row.pages));
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("textbox")).toHaveValue(String(row.pages));
    });

    test("Enter opens the status select editor", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText("want to read");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("combobox")).toHaveValue("want_to_read");
    });

    test("entering edit mode on a reading cell primes the reading ETag, not the metadata one", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText("want to read");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("combobox")).toBeInTheDocument();
      expect(getReadingState).toHaveBeenCalledWith(row.id);
      expect(getBookMetadata).not.toHaveBeenCalled();
    });

    test("Enter opens the rating star editor", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText("★★★");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("group", { name: "Rating" })).toBeInTheDocument();
    });

    test("Enter opens the authors popover editor", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText(row.authors.join(", "));
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("textbox", { name: "Author 1" })).toBeInTheDocument();
    });

    test("the series cell never opens an editor", async () => {
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText("Discworld · #8");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("textbox")).not.toBeInTheDocument();
      await expect.element(page.getByRole("combobox")).not.toBeInTheDocument();
    });

    test("a pending cell renders aria-busy with the draft value while its commit is in flight", async () => {
      vi.mocked(updateReadingState).mockImplementation(() => new Promise(() => undefined));
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText("★★★");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("group", { name: "Rating" })).toBeInTheDocument();
      await user.keyboard("5");

      const pendingCell = page.getByRole("grid").getByText("★★★★★");
      await expect.element(pendingCell).toHaveAttribute("aria-busy", "true");
      expect(updateReadingState).toHaveBeenCalled();
    });

    test("editing still works with every hideable column hidden", async () => {
      const row = editableRowFixture();
      await renderTableView({
        items: [row],
        hiddenColumns: new Set([
          "subtitle",
          "authors",
          "series",
          "isbn_13",
          "pages",
          "status",
          "rating",
          "added",
        ]),
      });
      const user = userEvent;
      const detailsButton = page.getByRole("button", {
        name: `Open details for ${row.title}`,
      });
      await user.click(detailsButton);
      await user.keyboard("{ArrowRight}{Enter}");
      await expect.element(page.getByRole("textbox")).toHaveValue(row.title);
    });

    test("a pending cell blocks its own editor from reopening while the commit is in flight", async () => {
      vi.mocked(updateReadingState).mockImplementation(() => new Promise(() => undefined));
      const row = editableRowFixture();
      await renderTableView({ items: [row] });
      const grid = page.getByRole("grid").element();
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent;
      const cell = page.getByRole("grid").getByText("★★★");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await expect.element(page.getByRole("group", { name: "Rating" })).toBeInTheDocument();
      await user.keyboard("5");
      const pendingCell = page.getByRole("grid").getByText("★★★★★");

      await user.click(pendingCell);
      await user.keyboard("{Enter}");

      await expect.element(page.getByRole("group", { name: "Rating" })).not.toBeInTheDocument();
    });
  });

  describe("row activation", () => {
    test("clicking a read-only cell activates the row; an editable cell does not", async () => {
      const onRowActivate = vi.fn();
      const row = rowFixture(1, { series: { id: "s1", name: "Discworld", position: 8 } });
      await renderTableView({ items: [row], onRowActivate });
      const grid = page.getByRole("grid").element();
      grid.scrollTo({ left: grid.scrollWidth });
      const user = userEvent;
      const seriesCell = page.getByRole("grid").getByText("Discworld · #8");
      await user.click(seriesCell);
      expect(onRowActivate).toHaveBeenCalledWith(row);
      onRowActivate.mockClear();
      const authorsCell = page.getByRole("grid").getByText(row.authors.join(", "));
      await user.click(authorsCell);
      expect(onRowActivate).not.toHaveBeenCalled();
    });

    test("the drawer stays reachable with every hideable column hidden in compact density", async () => {
      const onRowActivate = vi.fn();
      await renderTableView({
        onRowActivate,
        density: "compact",
        hiddenColumns: new Set([
          "subtitle",
          "authors",
          "series",
          "isbn_13",
          "pages",
          "status",
          "rating",
          "added",
        ]),
      });
      const grid = page.getByRole("grid").element();

      await expect.element(grid).toHaveAttribute("aria-colcount", "3");
      await expect
        .element(page.getByRole("columnheader", { name: "Subtitle (all editions)" }))
        .not.toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: `Open details for ${ROWS[0].title}` }));
      expect(onRowActivate).toHaveBeenCalledWith(ROWS[0]);
    });
  });

  describe("density and responsive columns", () => {
    test("comfortable density renders the cover mark; compact hides it", async () => {
      await renderTableView({ items: [rowFixture(1)] });
      await expect.element(page.getByRole("grid")).toBeInTheDocument();
      expect(page.getByText("BOO").elements().length).toBeGreaterThan(0);
      await cleanup();
      await renderTableView({ items: [rowFixture(1)], density: "compact" });
      await expect.element(page.getByRole("grid")).toBeInTheDocument();
      await expect.element(page.getByText("BOO")).not.toBeInTheDocument();
    });

    test("the mobile trim keeps selection, details, title (with author stack), and added", async () => {
      await page.viewport(700, 800);
      try {
        await renderTableView({ items: [rowFixture(1)] });
        const grid = page.getByRole("grid").element();

        await expect.element(grid).toHaveAttribute("aria-colcount", "4");
        await expect
          .element(page.getByRole("columnheader", { name: "Authors (all editions)" }))
          .not.toBeInTheDocument();
        await expect
          .element(page.getByRole("columnheader", { name: "Subtitle (all editions)" }))
          .not.toBeInTheDocument();

        await expect.element(page.getByRole("grid").getByText("Author 1")).toBeInTheDocument();
      } finally {
        await page.viewport(1280, 800);
      }
    });
  });
});
