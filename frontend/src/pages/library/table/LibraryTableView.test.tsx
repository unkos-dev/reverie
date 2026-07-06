import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { ComponentProps, ReactElement } from "react";

import type { BookListItem } from "@/api";
import { updateReadingState } from "@/api/reading";
import { queryKeys } from "@/lib/query/keys";

import { LibraryTableView } from "./LibraryTableView";

// Cell-edit orchestration (`useCellEdit`) fires through these clients on
// commit; none of these tests exercise a real network call, so the
// network-calling export of each is stubbed. Real exports are preserved via
// `importOriginal`: `@/api/reading` calls `.nullable()` on `@/api/books`'s
// `ReadingStatusSchema` at module load, which a bare automock would break
// (automocking drops the zod prototype). `sonner` is stubbed too since a
// commit path always calls `toast.success`/`toast.error`.
vi.mock("@/api/books", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, updateBookMetadata: vi.fn() };
});
vi.mock("@/api/reading", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, updateReadingState: vi.fn() };
});
vi.mock("@/api/metadata", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, revertField: vi.fn() };
});
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

beforeEach(() => {
  vi.resetAllMocks();
});

type TableProps = ComponentProps<typeof LibraryTableView>;

function rowFixture(index: number, overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    id: `book-${String(index)}`,
    work_id: `work-${String(index)}`,
    title: `Book Title ${String(index)}`,
    subtitle: `Subtitle ${String(index)}`,
    authors: [`Author ${String(index)}`],
    series: null,
    isbn_13: `9780000${String(index).padStart(6, "0")}`,
    pages: 100 + index,
    cover_url: "",
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    reading_state: null,
    ...overrides,
  };
}

// react-data-grid's jsdom virtualization only mounts the rows that fit the
// stubbed 768px viewport (tests/setup.ts) at scrollTop 0 — roughly the first
// 20 of 36px-tall rows. Row 0 carries every nullable field cleared so the
// em-dash and title-link assertions both land on a row that's guaranteed to
// be mounted, rather than relying on a scroll to reach a later row.
const ROWS: BookListItem[] = Array.from({ length: 30 }, (_, index) =>
  index === 0
    ? rowFixture(index, { subtitle: null, isbn_13: null, pages: null, reading_state: null })
    : rowFixture(index),
);

function renderTableView(overrides: Partial<TableProps> = {}): TableProps {
  const props: TableProps = {
    items: ROWS,
    sort: "recent",
    onSortChange: vi.fn(),
    hasNextPage: false,
    isFetchingNextPage: false,
    isFetchNextPageError: false,
    onLoadMore: vi.fn(),
    listQueryKey: queryKeys.books.list({}),
    ...overrides,
  };
  const routes: RouteObject[] = [{ path: "/library", element: <LibraryTableView {...props} /> }];
  const router = createMemoryRouter(routes, { initialEntries: ["/library"] });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
  return props;
}

/**
 * Drives the binding's fetch-on-scroll gate. jsdom never lays out real
 * scroll geometry, so the three dimensions the `isAtBottom` check reads are
 * stubbed directly on the grid element; the values place it unambiguously
 * past the 10px slack in `isAtBottom`.
 */
function scrollToBottom(grid: HTMLElement): void {
  Object.defineProperty(grid, "scrollHeight", { value: 1000, configurable: true });
  Object.defineProperty(grid, "clientHeight", { value: 300, configurable: true });
  Object.defineProperty(grid, "scrollTop", { value: 1000, configurable: true });
  fireEvent.scroll(grid);
}

describe("LibraryTableView", () => {
  test("mounts an ARIA grid labeled Library books, with the title linking to /b/{id}", async () => {
    renderTableView();
    const grid = await screen.findByRole("grid", { name: "Library books" });
    const link = within(grid).getByRole("link", { name: ROWS[0].title });
    expect(link.getAttribute("href")).toBe(`/b/${ROWS[0].id}`);
  });

  test("a row with every nullable field null renders the em-dash placeholder", async () => {
    renderTableView();
    await screen.findByRole("grid");
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  });

  test("clicking the Authors header sorts by author", async () => {
    const onSortChange = vi.fn();
    renderTableView({ onSortChange });
    const grid = await screen.findByRole("grid");
    // Title and Subtitle (unwidthed columns) each auto-size to the full 1024px
    // viewport stubbed by `tests/setup.ts`'s getBoundingClientRect override, so
    // Authors starts at x≈2048 and sits outside the initial horizontal
    // virtualization window. scrollLeft=1500 lands inside (1024, 2098) — the
    // range that brings Authors' header cell into the mounted window without
    // scrolling past it. If that 1024px stub value ever changes, these two
    // numbers need to move with it.
    Object.defineProperty(grid, "scrollWidth", { value: 2600, configurable: true });
    Object.defineProperty(grid, "scrollLeft", { value: 1500, configurable: true });
    fireEvent.scroll(grid);
    const user = userEvent.setup();
    const header = await screen.findByRole("columnheader", { name: "Authors (all editions)" });
    await user.click(header);
    expect(onSortChange).toHaveBeenCalledWith("author");
  });

  test("clicking the Title header sorts by title", async () => {
    const onSortChange = vi.fn();
    renderTableView({ onSortChange });
    const user = userEvent.setup();
    const header = await screen.findByRole("columnheader", { name: "Title (all editions)" });
    await user.click(header);
    expect(onSortChange).toHaveBeenCalledWith("title");
  });

  test("scrolling to the bottom calls onLoadMore when a next page is available", async () => {
    const onLoadMore = vi.fn();
    renderTableView({ hasNextPage: true, isFetchingNextPage: false, onLoadMore });
    const grid = await screen.findByRole("grid");
    scrollToBottom(grid);
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  test("scrolling to the bottom does NOT call onLoadMore while a fetch is already in flight", async () => {
    const onLoadMore = vi.fn();
    renderTableView({ hasNextPage: true, isFetchingNextPage: true, onLoadMore });
    const grid = await screen.findByRole("grid");
    scrollToBottom(grid);
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  test("scrolling to the bottom does NOT call onLoadMore when there is no next page", async () => {
    const onLoadMore = vi.fn();
    renderTableView({ hasNextPage: false, isFetchingNextPage: false, onLoadMore });
    const grid = await screen.findByRole("grid");
    scrollToBottom(grid);
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  test("scrolling to the bottom does NOT call onLoadMore after a failed fetch; Retry is the only refire path", async () => {
    const onLoadMore = vi.fn();
    renderTableView({ hasNextPage: true, isFetchNextPageError: true, onLoadMore });
    const grid = await screen.findByRole("grid");
    scrollToBottom(grid);
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  test("shows the end-of-list line once every loaded row has been fetched", async () => {
    renderTableView({ hasNextPage: false });
    expect(await screen.findByText(/30 books loaded/)).toBeInTheDocument();
  });

  test("idle state with a next page renders the Load more fallback, which calls onLoadMore", async () => {
    const onLoadMore = vi.fn();
    renderTableView({ hasNextPage: true, isFetchingNextPage: false, onLoadMore });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Load more" }));
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  test("populated series and status cells render their projections, not placeholders", async () => {
    renderTableView({
      items: [
        rowFixture(1, {
          series: { id: "s1", name: "Discworld", position: 8 },
          reading_state: { status: "want_to_read", rating: 3, progress_pct: null },
        }),
      ],
    });
    const grid = await screen.findByRole("grid");
    // With this single short row the grid computes columns
    // 1024/1024/50/50/140/80/120/90 (total 2578), so Series onward sits past
    // the initial 1024px window. Scrolling to the max position (2578 - 1024)
    // mounts everything from Series through Rating in one shot.
    Object.defineProperty(grid, "scrollWidth", { value: 2578, configurable: true });
    Object.defineProperty(grid, "scrollLeft", { value: 1554, configurable: true });
    fireEvent.scroll(grid);
    expect(await within(grid).findByText("Discworld · #8")).toBeInTheDocument();
    // Mounting Series re-measures the auto columns to the 1024px viewport
    // stub (template becomes 4x1024 + fixed), pushing Status and Rating out
    // of the window again; a second max-position scroll reaches them.
    Object.defineProperty(grid, "scrollWidth", { value: 4526, configurable: true });
    Object.defineProperty(grid, "scrollLeft", { value: 3502, configurable: true });
    fireEvent.scroll(grid);
    expect(await within(grid).findByText("want to read")).toBeInTheDocument();
    expect(within(grid).getByText("★★★")).toBeInTheDocument();
  });

  test("a failed next-page fetch renders an alert with a Retry control that calls onLoadMore", async () => {
    const onLoadMore = vi.fn();
    renderTableView({ isFetchNextPageError: true, onLoadMore });
    const alert = await screen.findByRole("alert");
    const user = userEvent.setup();
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  test("work-scoped columns carry an 'all editions' suffix in their header, non-scoped columns don't", async () => {
    renderTableView({
      items: [
        rowFixture(1, {
          series: { id: "s1", name: "Discworld", position: 8 },
          reading_state: { status: "want_to_read", rating: 3, progress_pct: null },
        }),
      ],
    });
    const grid = await screen.findByRole("grid");
    expect(
      await screen.findByRole("columnheader", { name: "Title (all editions)" }),
    ).toBeInTheDocument();

    // Same scroll dance as the "populated series and status" render test:
    // ISBN and Pages sit past the initial 1024px window until scrolled in.
    Object.defineProperty(grid, "scrollWidth", { value: 2578, configurable: true });
    Object.defineProperty(grid, "scrollLeft", { value: 1554, configurable: true });
    fireEvent.scroll(grid);
    Object.defineProperty(grid, "scrollWidth", { value: 4526, configurable: true });
    Object.defineProperty(grid, "scrollLeft", { value: 3502, configurable: true });
    fireEvent.scroll(grid);

    expect(await within(grid).findByRole("columnheader", { name: "ISBN" })).toBeInTheDocument();
    expect(within(grid).getByRole("columnheader", { name: "Pages" })).toBeInTheDocument();
    expect(
      within(grid).queryByRole("columnheader", { name: "ISBN (all editions)" }),
    ).not.toBeInTheDocument();
    expect(
      within(grid).queryByRole("columnheader", { name: "Pages (all editions)" }),
    ).not.toBeInTheDocument();
  });

  describe("cell editing", () => {
    /** Same shape as the series/status/rating render test above: a single
     *  short row with every editable field populated, so the already-proven
     *  scroll-to-max-position dance reveals every column in one grid. */
    function editableRowFixture(): BookListItem {
      return rowFixture(1, {
        series: { id: "s1", name: "Discworld", position: 8 },
        reading_state: { status: "want_to_read", rating: 3, progress_pct: null },
      });
    }

    function scrollToRevealRightColumns(grid: HTMLElement): void {
      Object.defineProperty(grid, "scrollWidth", { value: 2578, configurable: true });
      Object.defineProperty(grid, "scrollLeft", { value: 1554, configurable: true });
      fireEvent.scroll(grid);
    }

    function scrollToRevealTrailingColumns(grid: HTMLElement): void {
      Object.defineProperty(grid, "scrollWidth", { value: 4526, configurable: true });
      Object.defineProperty(grid, "scrollLeft", { value: 3502, configurable: true });
      fireEvent.scroll(grid);
    }

    test("Enter opens the title text editor prefilled with the current value", async () => {
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const user = userEvent.setup();
      // Click a plain-text neighbor cell rather than the title cell itself:
      // the title cell renders a react-router Link, and clicking straight
      // into it would navigate instead of only selecting the grid cell.
      const subtitleCell = await screen.findByText(row.subtitle ?? "");
      await user.click(subtitleCell);
      await user.keyboard("{ArrowLeft}");
      await user.keyboard("{Enter}");
      expect(await screen.findByRole("textbox")).toHaveValue(row.title);
    });

    test("Enter opens the ISBN text editor", async () => {
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const grid = await screen.findByRole("grid");
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent.setup();
      const cell = await within(grid).findByText(row.isbn_13 ?? "");
      await user.click(cell);
      await user.keyboard("{Enter}");
      expect(await screen.findByRole("textbox")).toHaveValue(row.isbn_13);
    });

    test("Enter opens the pages text editor", async () => {
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const grid = await screen.findByRole("grid");
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent.setup();
      const cell = await within(grid).findByText(String(row.pages));
      await user.click(cell);
      await user.keyboard("{Enter}");
      expect(await screen.findByRole("textbox")).toHaveValue(String(row.pages));
    });

    test("Enter opens the status select editor", async () => {
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const grid = await screen.findByRole("grid");
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent.setup();
      const cell = await within(grid).findByText("want to read");
      await user.click(cell);
      await user.keyboard("{Enter}");
      expect(await screen.findByRole("combobox")).toHaveValue("want_to_read");
    });

    test("Enter opens the rating star editor", async () => {
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const grid = await screen.findByRole("grid");
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent.setup();
      const cell = await within(grid).findByText("★★★");
      await user.click(cell);
      await user.keyboard("{Enter}");
      expect(await screen.findByRole("group", { name: "Rating" })).toBeInTheDocument();
    });

    test("Enter opens the authors popover editor", async () => {
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const grid = await screen.findByRole("grid");
      scrollToRevealRightColumns(grid);
      const user = userEvent.setup();
      const cell = await within(grid).findByText(row.authors.join(", "));
      await user.click(cell);
      await user.keyboard("{Enter}");
      expect(await screen.findByRole("textbox", { name: "Author 1" })).toBeInTheDocument();
    });

    test("the series cell never opens an editor", async () => {
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const grid = await screen.findByRole("grid");
      scrollToRevealRightColumns(grid);
      const user = userEvent.setup();
      const cell = await within(grid).findByText("Discworld · #8");
      await user.click(cell);
      await user.keyboard("{Enter}");
      expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
      expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    });

    test("a pending cell renders aria-busy with the draft value while its commit is in flight", async () => {
      vi.mocked(updateReadingState).mockImplementation(() => new Promise(() => undefined));
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const grid = await screen.findByRole("grid");
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent.setup();
      const cell = await within(grid).findByText("★★★");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await screen.findByRole("group", { name: "Rating" });
      // Digit "5" both sets and commits the rating (RatingCellEditor).
      await user.keyboard("5");

      const pendingCell = await within(grid).findByText("★★★★★");
      expect(pendingCell).toHaveAttribute("aria-busy", "true");
      expect(updateReadingState).toHaveBeenCalled();
    });

    test("a pending cell blocks its own editor from reopening while the commit is in flight", async () => {
      vi.mocked(updateReadingState).mockImplementation(() => new Promise(() => undefined));
      const row = editableRowFixture();
      renderTableView({ items: [row] });
      const grid = await screen.findByRole("grid");
      scrollToRevealRightColumns(grid);
      scrollToRevealTrailingColumns(grid);
      const user = userEvent.setup();
      const cell = await within(grid).findByText("★★★");
      await user.click(cell);
      await user.keyboard("{Enter}");
      await screen.findByRole("group", { name: "Rating" });
      // Digit "5" both sets and commits the rating (RatingCellEditor),
      // closing the editor and leaving the cell pending.
      await user.keyboard("5");
      const pendingCell = await within(grid).findByText("★★★★★");

      await user.click(pendingCell);
      await user.keyboard("{Enter}");

      expect(screen.queryByRole("group", { name: "Rating" })).not.toBeInTheDocument();
    });
  });
});
