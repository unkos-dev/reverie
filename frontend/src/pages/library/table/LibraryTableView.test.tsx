import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";
import { describe, expect, test, vi } from "vite-plus/test";
import type { ComponentProps } from "react";

import type { BookListItem } from "@/api";

import { LibraryTableView } from "./LibraryTableView";

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
    ...overrides,
  };
  const routes: RouteObject[] = [{ path: "/library", element: <LibraryTableView {...props} /> }];
  const router = createMemoryRouter(routes, { initialEntries: ["/library"] });
  render(<RouterProvider router={router} />);
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
    const header = await screen.findByRole("columnheader", { name: "Authors" });
    await user.click(header);
    expect(onSortChange).toHaveBeenCalledWith("author");
  });

  test("clicking the Title header sorts by title", async () => {
    const onSortChange = vi.fn();
    renderTableView({ onSortChange });
    const user = userEvent.setup();
    const header = await screen.findByRole("columnheader", { name: "Title" });
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

  test("shows the end-of-list line once every loaded row has been fetched", async () => {
    renderTableView({ hasNextPage: false });
    expect(await screen.findByText(/30 books loaded/)).toBeInTheDocument();
  });

  test("a failed next-page fetch renders an alert with a Retry control that calls onLoadMore", async () => {
    const onLoadMore = vi.fn();
    renderTableView({ isFetchNextPageError: true, onLoadMore });
    const alert = await screen.findByRole("alert");
    const user = userEvent.setup();
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });
});
