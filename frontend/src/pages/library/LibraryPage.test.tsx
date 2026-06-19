import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
  /** Additional cache slots to prefill (e.g. a post-interaction sort key). */
  extraCacheParams?: ListBooksParams[];
}

function renderLibrary({
  items,
  nextCursor,
  initialEntries,
  cacheParams,
  extraCacheParams = [],
}: RenderOpts): {
  client: QueryClient;
} {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const params: ListBooksParams = cacheParams ?? {};
  const response: BookListResponse = { items, next_cursor: nextCursor };
  for (const slot of [params, ...extraCacheParams]) {
    client.setQueryData(queryKeys.books.list(slot), {
      pages: [response],
      pageParams: [undefined],
    });
  }

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
  test("renders the heading without a fabricated total", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    expect(await screen.findByRole("heading", { name: "Library" })).toBeInTheDocument();
    // items.length counts loaded pages only — a "N books" line would
    // misstate the true library total, so the page renders no count.
    expect(screen.queryByText(/1 book/)).not.toBeInTheDocument();
  });

  test("mounts the ambient atmosphere behind the editorial masthead", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    // Masthead renders in page context (integration, not the unit test).
    expect(await screen.findByRole("heading", { name: "Library" })).toBeInTheDocument();
    expect(screen.getByText("Your library, catalogued.")).toBeInTheDocument();
    // Decorative atmosphere layers mount once at the top of the page.
    expect(document.querySelector(".lib-atm")).not.toBeNull();
    expect(document.querySelector(".lib-grain")).not.toBeNull();
  });

  test("sort control is a button menu writing ?sort=", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      extraCacheParams: [{ sort: "title" }],
    });
    const user = userEvent.setup();
    const sortButton = await screen.findByRole("button", { name: /Sort/ });
    await user.click(sortButton);
    const title = await screen.findByRole("menuitemradio", { name: "Title" });
    title.focus();
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("button", { name: /Sort: Title/i })).toBeInTheDocument();
  });

  test("grid tiles carry a focus treatment equal to the hover treatment", async () => {
    renderLibrary({ items: [bookFixture({ id: "abc", title: "Stoner" })], nextCursor: null });
    const link = await screen.findByRole("link", { name: /stoner/i });
    expect(link.className).toMatch(/hover:/);
    expect(link.className).toMatch(/focus-visible:/);
  });

  test("missing cover art falls back to the typographic spine", async () => {
    renderLibrary({
      items: [bookFixture({ id: "no-cover", title: "Spineless", cover_url: "" })],
      nextCursor: null,
    });
    await screen.findByTestId("library-grid");
    expect(document.querySelector("[data-layout]")).not.toBeNull();
  });

  test("cover image load error swaps in the typographic spine", async () => {
    renderLibrary({
      items: [bookFixture({ id: "broken-cover", title: "Broken" })],
      nextCursor: null,
    });
    const grid = await screen.findByTestId("library-grid");
    const img = within(grid).getByRole("img", { name: /Cover of/ });
    expect(document.querySelector("[data-layout]")).toBeNull();
    fireEvent.error(img);
    await waitFor(() => {
      expect(document.querySelector("[data-layout]")).not.toBeNull();
    });
  });

  test("filter rail lists distinct series from the loaded pages", async () => {
    renderLibrary({
      items: [
        bookFixture({ id: "a", series: { id: "s-1", name: "Discworld", position: 1 } }),
        bookFixture({ id: "b", series: { id: "s-1", name: "Discworld", position: 2 } }),
        bookFixture({ id: "c", series: null }),
      ],
      nextCursor: null,
    });
    const rail = await screen.findByRole("complementary", { name: "Filters" });
    expect(within(rail).getAllByRole("radio", { name: "Discworld" })).toHaveLength(1);
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
