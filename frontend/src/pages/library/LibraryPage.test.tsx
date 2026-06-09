import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import type { BookListItem, BookListResponse, ListBooksParams } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { LibraryPage } from "./LibraryPage";

function bookFixture(overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    id: "11111111-1111-1111-1111-111111111111",
    work_id: "22222222-2222-2222-2222-222222222222",
    title: "The Brothers Karamazov",
    authors: ["Fyodor Dostoevsky"],
    series: null,
    isbn_13: "9780374528379",
    cover_url: "/api/v1/books/11111111/cover/thumb",
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    ...overrides,
  };
}

interface RenderOpts {
  items: BookListItem[];
  nextCursor: string | null;
  initialEntries?: string[];
  /** Params shape to prefill cache under. Defaults to the empty-params slot. */
  cacheParams?: ListBooksParams;
}

function renderLibrary({ items, nextCursor, initialEntries, cacheParams }: RenderOpts): {
  client: QueryClient;
} {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const params: ListBooksParams = cacheParams ?? {};
  const response: BookListResponse = { items, next_cursor: nextCursor };
  client.setQueryData(queryKeys.books.list(params), {
    pages: [response],
    pageParams: [undefined],
  });

  function Wrapper(): ReactElement {
    const routes: RouteObject[] = [{ path: "/library", element: <LibraryPage /> }];
    const router = createMemoryRouter(routes, {
      initialEntries: initialEntries ?? ["/library"],
    });
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }

  render(<Wrapper />);
  return { client };
}

describe("LibraryPage", () => {
  test("renders the heading and book count", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    expect(await screen.findByRole("heading", { name: "Library" })).toBeInTheDocument();
    expect(screen.getByText("1 book")).toBeInTheDocument();
  });

  test("renders one card per item in the grid by default", async () => {
    renderLibrary({
      items: [
        bookFixture({ id: "a", title: "Stoner" }),
        bookFixture({ id: "b", title: "Piranesi" }),
        bookFixture({ id: "c", title: "Annihilation" }),
      ],
      nextCursor: null,
    });
    const grid = await screen.findByTestId("library-grid");
    expect(within(grid).getAllByRole("listitem")).toHaveLength(3);
    expect(within(grid).getByText("Stoner")).toBeInTheDocument();
    expect(within(grid).getByText("Piranesi")).toBeInTheDocument();
    expect(within(grid).getByText("Annihilation")).toBeInTheDocument();
  });

  test("does NOT show Load more when next_cursor is null", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    await screen.findByRole("heading", { name: "Library" });
    expect(screen.queryByRole("button", { name: /load more/i })).not.toBeInTheDocument();
  });

  test("shows Load more when next_cursor is set", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: "eyJ4Ijox" });
    expect(await screen.findByRole("button", { name: /load more/i })).toBeInTheDocument();
  });

  test("renders the list table when ?view=list", async () => {
    renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      initialEntries: ["/library?view=list"],
    });
    const list = await screen.findByTestId("library-list");
    expect(within(list).getByText("Stoner")).toBeInTheDocument();
  });

  test("renders the empty state when items is empty", async () => {
    renderLibrary({ items: [], nextCursor: null });
    expect(await screen.findByText("No books yet")).toBeInTheDocument();
    expect(screen.queryByTestId("library-grid")).not.toBeInTheDocument();
  });

  test("links each book card to /b/{id}", async () => {
    renderLibrary({
      items: [bookFixture({ id: "abc-123", title: "Stoner" })],
      nextCursor: null,
    });
    const link = await screen.findByRole("link", { name: /stoner/i });
    expect(link.getAttribute("href")).toBe("/b/abc-123");
  });

  test("renders no active-filter chip row when no filter params are set", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    await screen.findByRole("heading", { name: "Library" });
    expect(screen.queryByTestId("active-filters")).not.toBeInTheDocument();
  });

  test("renders an active-filter chip when ?author= param is set", async () => {
    const authorId = "aaaa1111-0000-0000-0000-000000000000";
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: [`/library?author=${authorId}`],
      cacheParams: { author: authorId },
    });
    await screen.findByTestId("active-filters");
    expect(screen.getByRole("button", { name: /clear author filter/i })).toBeInTheDocument();
  });

  test("renders one tag chip per ?tag= repetition", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?tag=scifi&tag=hugo"],
      cacheParams: {},
    });
    await screen.findByTestId("active-filters");
    expect(screen.getByRole("button", { name: /clear tag scifi filter/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /clear tag hugo filter/i })).toBeInTheDocument();
  });

  test("clicking a filter chip removes the param and the cursor", async () => {
    const authorId = "aaaa1111-0000-0000-0000-000000000000";
    // Pre-fill both the active-author slot AND the empty-params slot
    // so the suspense-infinite-query has a hit after the chip click
    // changes the cache key.
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const response: BookListResponse = { items: [bookFixture()], next_cursor: null };
    client.setQueryData(queryKeys.books.list({ author: authorId }), {
      pages: [response],
      pageParams: [undefined],
    });
    client.setQueryData(queryKeys.books.list({}), {
      pages: [response],
      pageParams: [undefined],
    });
    function Wrapper(): ReactElement {
      const routes: RouteObject[] = [{ path: "/library", element: <LibraryPage /> }];
      const router = createMemoryRouter(routes, {
        initialEntries: [`/library?author=${authorId}&cursor=eyJ4Ijox`],
      });
      return (
        <QueryClientProvider client={client}>
          <RouterProvider router={router} />
        </QueryClientProvider>
      );
    }
    render(<Wrapper />);

    const chip = await screen.findByRole("button", { name: /clear author filter/i });
    const user = userEvent.setup();
    await user.click(chip);
    expect(screen.queryByTestId("active-filters")).not.toBeInTheDocument();
  });
});
