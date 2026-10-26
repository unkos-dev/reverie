import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { useState, type ComponentProps, type ReactElement } from "react";

import type { BookListItem, SortLevelParam } from "@/api";
import { getBookMetadata } from "@/api/books";
import { getReadingState } from "@/api/reading";
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
