import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, useLocation, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import type { BookListItem, BookListResponse, ListBooksParams } from "@/api";
import type { AuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";

import { LibraryPage } from "./LibraryPage";
import { VIEW_COOKIE_NAME } from "./view-cookie";

function bookFixture(overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    id: "11111111-1111-1111-1111-111111111111",
    work_id: "22222222-2222-2222-2222-222222222222",
    title: "The Brothers Karamazov",
    subtitle: null,
    authors: ["Fyodor Dostoevsky"],
    series: null,
    isbn_13: "9780374528379",
    pages: null,
    cover_url: "/api/v1/books/11111111/cover/thumb",
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    reading_state: null,
    created_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function meFixture(overrides: Partial<AuthMe> = {}): AuthMe {
  return {
    id: "33333333-3333-3333-3333-333333333333",
    display_name: "Operator",
    email: null,
    role: "admin",
    is_child: false,
    theme_preference: "system",
    csrf_token: null,
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
  /** Prefill the `/auth/me` cache so role-gated UI renders without a fetch. */
  me?: AuthMe;
}

function renderLibrary({
  items,
  nextCursor,
  initialEntries,
  cacheParams,
  extraCacheParams = [],
  me,
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
  if (me !== undefined) client.setQueryData(queryKeys.auth.me(), me);

  // Sibling probe exposing the live search string, since the memory router
  // instance is scoped to this wrapper and not reachable from assertions.
  function LocationProbe(): ReactElement {
    const location = useLocation();
    return <div data-testid="location-search">{location.search}</div>;
  }

  function Wrapper(): ReactElement {
    const routes: RouteObject[] = [
      {
        path: "/library",
        element: (
          <>
            <LibraryPage />
            <LocationProbe />
          </>
        ),
      },
    ];
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

  test("cinema-hint is always in the DOM so its CSS fade can play (not gated on state)", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    await screen.findByRole("heading", { name: "Library" });
    // Not in cinematic mode, but the hint must still render — visibility is
    // CSS-only (`[data-cinematic="on"] .cinema-hint`), so unmounting it would
    // kill the fade-out.
    expect(document.querySelector(".cinema-hint")).not.toBeNull();
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

  test("a table-built stack matching no preset shows the Custom sort state", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?sort=author,-created_at"],
      cacheParams: { sort: "author,-created_at" },
    });
    const sortButton = await screen.findByRole("button", { name: "Sort: Custom" });
    const user = userEvent.setup();
    await user.click(sortButton);
    // None of the three presets match a two-level stack, so none is checked.
    for (const label of ["Recent", "Title", "Author"]) {
      expect(await screen.findByRole("menuitemradio", { name: label })).toHaveAttribute(
        "aria-checked",
        "false",
      );
    }
  });

  test("grid tiles carry a focus treatment equal to the hover treatment", async () => {
    renderLibrary({ items: [bookFixture({ id: "abc", title: "Stoner" })], nextCursor: null });
    const link = await screen.findByRole("link", { name: /stoner/i });
    expect(link.className).toMatch(/hover:/);
    expect(link.className).toMatch(/focus-visible:/);
  });

  test("missing cover art falls back to the cloth spine", async () => {
    renderLibrary({
      items: [bookFixture({ id: "no-cover", title: "Spineless", cover_url: "" })],
      nextCursor: null,
    });
    await screen.findByTestId("library-grid");
    expect(document.querySelector("[data-cloth]")).not.toBeNull();
  });

  test("cover image load error swaps in the cloth spine", async () => {
    renderLibrary({
      items: [bookFixture({ id: "broken-cover", title: "Broken" })],
      nextCursor: null,
    });
    const grid = await screen.findByTestId("library-grid");
    const img = within(grid).getByRole("img", { name: /Cover of/ });
    expect(document.querySelector("[data-cloth]")).toBeNull();
    fireEvent.error(img);
    await waitFor(() => {
      expect(document.querySelector("[data-cloth]")).not.toBeNull();
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
    expect(within(rail).getAllByRole("checkbox", { name: "Discworld" })).toHaveLength(1);
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

  test("view toggle group renders three buttons; Table reflects aria-pressed", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    const group = await screen.findByRole("group", { name: "View mode" });
    expect(within(group).getByRole("button", { name: "Grid", pressed: true })).toBeInTheDocument();
    expect(within(group).getByRole("button", { name: "List", pressed: false })).toBeInTheDocument();
    expect(
      within(group).getByRole("button", { name: "Table", pressed: false }),
    ).toBeInTheDocument();
  });

  test("renders the table view when ?view=table (lazy-loaded)", async () => {
    renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      initialEntries: ["/library?view=table"],
    });
    expect(await screen.findByTestId("library-table")).toBeInTheDocument();
  });

  test("cookie default mounts the table view when ?view= is absent", async () => {
    document.cookie = `${VIEW_COOKIE_NAME}=table; Path=/`;
    try {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      expect(await screen.findByTestId("library-table")).toBeInTheDocument();
    } finally {
      // Explicit expiry so the cookie can't leak into a later test in this file.
      document.cookie = `${VIEW_COOKIE_NAME}=; Path=/; Max-Age=0`;
    }
  });

  test("invalid ?view= value falls back to grid", async () => {
    renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      initialEntries: ["/library?view=xyz"],
    });
    expect(await screen.findByTestId("library-grid")).toBeInTheDocument();
  });

  test("clicking a view toggle persists the choice to the cookie", async () => {
    try {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      const group = await screen.findByRole("group", { name: "View mode" });
      const user = userEvent.setup();
      await user.click(within(group).getByRole("button", { name: "List" }));
      expect(document.cookie).toContain(`${VIEW_COOKIE_NAME}=list`);
    } finally {
      document.cookie = `${VIEW_COOKIE_NAME}=; Path=/; Max-Age=0`;
    }
  });

  test("table header click writes ?sort= and clears any cursor param", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?view=table&cursor=stale123"],
      extraCacheParams: [{ sort: "title" }],
    });
    await screen.findByTestId("library-table");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("columnheader", { name: "Title (all editions)" }));
    await waitFor(() => {
      const search = screen.getByTestId("location-search").textContent;
      expect(search).toContain("sort=title");
      expect(search).not.toContain("cursor=");
    });
  });

  test("table quick search writes ?q= and clears any cursor param", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?view=table&cursor=stale123"],
      extraCacheParams: [{ q: "war" }],
    });
    await screen.findByTestId("library-table");
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Quick search"), "war");
    await waitFor(() => {
      const search = screen.getByTestId("location-search").textContent;
      expect(search).toContain("q=war");
      expect(search).not.toContain("cursor=");
    });
  });

  test("a typed filter with no matches shows the filtered-empty state", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?status_any=unread"],
      cacheParams: { status_any: ["unread"] },
      extraCacheParams: [{}],
    });
    expect(await screen.findByText("No books match these filters")).toBeInTheDocument();
  });

  test("clear-all from the filtered-empty state drops typed filter params", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?status_any=unread&pages_gte=500"],
      cacheParams: { status_any: ["unread"], pages_gte: 500 },
      extraCacheParams: [{}],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    await waitFor(() => {
      const search = screen.getByTestId("location-search").textContent;
      expect(search).not.toContain("status_any");
      expect(search).not.toContain("pages_gte");
    });
  });

  test("URL ?view= beats the cookie default", async () => {
    document.cookie = `${VIEW_COOKIE_NAME}=table; Path=/`;
    try {
      renderLibrary({
        items: [bookFixture({ title: "Stoner" })],
        nextCursor: null,
        initialEntries: ["/library?view=list"],
      });
      expect(await screen.findByTestId("library-list")).toBeInTheDocument();
      expect(screen.queryByTestId("library-table")).not.toBeInTheDocument();
    } finally {
      document.cookie = `${VIEW_COOKIE_NAME}=; Path=/; Max-Age=0`;
    }
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
      cacheParams: { author: [authorId] },
    });
    await screen.findByTestId("active-filters");
    expect(screen.getByRole("button", { name: /clear author filter/i })).toBeInTheDocument();
  });

  test("renders one tag chip per ?tag= repetition", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?tag=scifi&tag=hugo"],
      cacheParams: { tag: ["scifi", "hugo"] },
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
    client.setQueryData(queryKeys.books.list({ author: [authorId] }), {
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

  test("an active series chip shows the resolved series name, not a raw id", async () => {
    renderLibrary({
      items: [bookFixture({ id: "a", series: { id: "s-1", name: "Discworld", position: 1 } })],
      nextCursor: null,
      initialEntries: ["/library?series=s-1"],
      cacheParams: { series: "s-1" },
    });
    const chips = await screen.findByTestId("active-filters");
    expect(within(chips).getByRole("button", { name: /Discworld/ })).toBeInTheDocument();
    expect(within(chips).queryByText(/s-1/)).not.toBeInTheDocument();
  });

  test("an active author chip shows the full author name, not a truncated id", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?author=Fyodor%20Dostoevsky"],
      cacheParams: { author: ["Fyodor Dostoevsky"] },
    });
    const chips = await screen.findByTestId("active-filters");
    expect(within(chips).getByRole("button", { name: /Fyodor Dostoevsky/ })).toBeInTheDocument();
  });

  test("filtered-empty shows its own copy and a clear-all action, not the true-empty copy", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?series=s-1"],
      cacheParams: { series: "s-1" },
      extraCacheParams: [{}],
    });
    expect(await screen.findByText("No books match these filters")).toBeInTheDocument();
    expect(screen.queryByText("No books yet")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /clear all filters/i })).toBeInTheDocument();
  });

  test("clearing all filters from the filtered-empty state drops the filter params", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?series=s-1&cursor=eyJ4Ijox"],
      cacheParams: { series: "s-1" },
      extraCacheParams: [{}],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    // Filters gone → the true-empty state takes over.
    expect(await screen.findByText("No books yet")).toBeInTheDocument();
    expect(screen.queryByText("No books match these filters")).not.toBeInTheDocument();
  });

  test("true-empty state shows no clear-filters action", async () => {
    renderLibrary({ items: [], nextCursor: null });
    expect(await screen.findByText("No books yet")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /clear all filters/i })).not.toBeInTheDocument();
  });

  test("true-empty offers an ingestion link to admins", async () => {
    renderLibrary({ items: [], nextCursor: null, me: meFixture({ role: "admin" }) });
    const link = await screen.findByRole("link", { name: /ingestion/i });
    expect(link.getAttribute("href")).toBe("/admin/dashboard");
  });

  test("true-empty hides the ingestion link from non-admin readers", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      me: meFixture({ role: "child", is_child: true }),
    });
    expect(await screen.findByText("No books yet")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /ingestion/i })).not.toBeInTheDocument();
  });

  test("a failed Load more surfaces an inline error and a Retry control", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network down"));
    // staleTime Infinity so the cached first page never background-refetches
    // and consumes the mock — only the explicit Load more fetch should fail.
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    client.setQueryData(queryKeys.books.list({}), {
      pages: [{ items: [bookFixture()], next_cursor: "eyJ4Ijox" }],
      pageParams: [undefined],
    });
    function Wrapper(): ReactElement {
      const routes: RouteObject[] = [{ path: "/library", element: <LibraryPage /> }];
      const router = createMemoryRouter(routes, { initialEntries: ["/library"] });
      return (
        <QueryClientProvider client={client}>
          <RouterProvider router={router} />
        </QueryClientProvider>
      );
    }
    render(<Wrapper />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /load more/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/couldn't load more/i);
    expect(await screen.findByRole("button", { name: /retry/i })).toBeInTheDocument();
    fetchSpy.mockRestore();
  });

  test("Retry after a failed Load more re-fetches, recovers, and keeps the loaded pages", async () => {
    // Deliberately triggered failure logs to console — silence it for clean output.
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const page2: BookListResponse = {
      items: [bookFixture({ id: "p2", title: "Crime and Punishment" })],
      next_cursor: null,
    };
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockRejectedValueOnce(new Error("network down"))
      .mockResolvedValueOnce(
        new Response(JSON.stringify(page2), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    client.setQueryData(queryKeys.books.list({}), {
      pages: [
        { items: [bookFixture({ title: "The Brothers Karamazov" })], next_cursor: "eyJ4Ijox" },
      ],
      pageParams: [undefined],
    });
    // Prefill the shelves cache so the rail's on-mount shelves fetch does not
    // consume the first (rejected) mock before the Load more click does.
    client.setQueryData(queryKeys.shelves.list(), []);
    function Wrapper(): ReactElement {
      const routes: RouteObject[] = [{ path: "/library", element: <LibraryPage /> }];
      const router = createMemoryRouter(routes, { initialEntries: ["/library"] });
      return (
        <QueryClientProvider client={client}>
          <RouterProvider router={router} />
        </QueryClientProvider>
      );
    }
    render(<Wrapper />);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: /load more/i }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    // Page 1 survives the failure.
    expect(screen.getByText("The Brothers Karamazov")).toBeInTheDocument();

    // Network heals: Retry clears the error and appends page 2.
    await user.click(screen.getByRole("button", { name: /retry/i }));
    expect(await screen.findByText("Crime and Punishment")).toBeInTheDocument();
    expect(screen.getByText("The Brothers Karamazov")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /retry/i })).not.toBeInTheDocument();

    fetchSpy.mockRestore();
    errorSpy.mockRestore();
  });

  test("clearing all filters preserves the active view and sort params", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?series=s-1&sort=title&view=list"],
      cacheParams: { series: "s-1", sort: "title" },
      extraCacheParams: [{ sort: "title" }],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    // Filter gone (true-empty takes over), but view (list) and sort (title)
    // survive — assert via the persisted controls, not the list table (which
    // an empty result set does not render).
    expect(await screen.findByRole("button", { name: /Sort: Title/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "List", pressed: true })).toBeInTheDocument();
    expect(screen.queryByText("No books match these filters")).not.toBeInTheDocument();
  });

  test("series chip falls back to a short id when the id is absent from the loaded pages", async () => {
    const longId = "aaaabbbb-cccc-dddd-eeee-ffffffffffff";
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: [`/library?series=${longId}`],
      cacheParams: { series: longId },
    });
    const chips = await screen.findByTestId("active-filters");
    // No matching series in the loaded pages, so shortId (first 8 + ellipsis),
    // never "undefined".
    expect(within(chips).getByRole("button", { name: /aaaabbbb…/ })).toBeInTheDocument();
    expect(within(chips).queryByRole("button", { name: /undefined/ })).not.toBeInTheDocument();
  });
});
