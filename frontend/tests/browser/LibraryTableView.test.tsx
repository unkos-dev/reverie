import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { userEvent } from "vite-plus/test/browser";
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

function renderTableView(overrides: Partial<TableProps> = {}): TableProps {
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
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
  return props;
}

describe("LibraryTableView", () => {
  test("mounts an ARIA grid labeled Library books, with the title linking to /b/{id}", async () => {
    renderTableView();
    const grid = await screen.findByRole("grid", { name: "Library books" });
    const link = within(grid).getByRole("link", { name: ROWS[0].title });
    expect(link.getAttribute("href")).toBe(`/b/${ROWS[0].id}`);
  });

  test("hosts no filter surface of its own", () => {
    renderTableView();
    expect(screen.queryByTestId("filter-bar")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Quick search")).not.toBeInTheDocument();
  });

  test("a row with every nullable field null renders the em-dash placeholder", async () => {
    renderTableView();
    await screen.findByRole("grid");
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  });

  test("clicking the Title header sorts by title", async () => {
    const onSortChange = vi.fn();
    renderTableView({ onSortChange });
    const user = userEvent;
    const header = await screen.findByRole("columnheader", { name: "Title" });
    await user.click(header);
    expect(onSortChange).toHaveBeenCalledWith([{ field: "title", desc: false }]);
  });

  describe("selection", () => {
    test("renders the loaded-rows select-all label and reports row selection", async () => {
      const onSelectionChange = vi.fn();
      renderTableView({ onSelectionChange });
      const grid = await screen.findByRole("grid");
      expect(grid).toHaveAttribute("aria-multiselectable", "true");
      expect(screen.getByRole("checkbox", { name: "Select all loaded books" })).toBeInTheDocument();
      const user = userEvent;
      await user.click(screen.getAllByRole("checkbox", { name: "Select" })[0]);
      expect(onSelectionChange).toHaveBeenCalledWith(new Set([ROWS[0].id]));
    });

    test("select-all covers every loaded row", async () => {
      const onSelectionChange = vi.fn();
      renderTableView({ onSelectionChange });
      await screen.findByRole("grid");
      const user = userEvent;
      await user.click(screen.getByRole("checkbox", { name: "Select all loaded books" }));
      expect(onSelectionChange).toHaveBeenCalledWith(new Set(ROWS.map((row) => row.id)));
    });

    test("selected rows carry aria-selected", async () => {
      renderTableView({ selectedIds: new Set([ROWS[0].id]) });
      await screen.findByRole("grid");
      const selectedRows = screen
        .getAllByRole("row")
        .filter((row) => row.getAttribute("aria-selected") === "true");
      expect(selectedRows).toHaveLength(1);
    });
  });

  test("the details button opens the drawer for its row", async () => {
    const onRowActivate = vi.fn();
    renderTableView({ onRowActivate });
    const user = userEvent;
    await user.click(
      await screen.findByRole("button", { name: `Open details for ${ROWS[0].title}` }),
    );
    expect(onRowActivate).toHaveBeenCalledTimes(1);
    expect(onRowActivate).toHaveBeenCalledWith(ROWS[0]);
  });
});
