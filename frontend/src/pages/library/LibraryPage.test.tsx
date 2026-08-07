import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { NuqsTestingAdapter } from "nuqs/adapters/testing";
import { RouterProvider, createMemoryRouter, useLocation, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import type { BookDetail, BookListItem, BookListResponse, ListBooksParams } from "@/api";
import type { AuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";

import { DISPLAY_STORAGE_KEY } from "./display-storage";
import { LibraryPage } from "./LibraryPage";
import { VIEW_COOKIE_NAME } from "./view-cookie";

// The masthead reads the effective theme to pick its hero asset; these tests
// exercise the page, not theme resolution, so the provider is stubbed.
vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ preference: "system", effective: "dark", setPreference: vi.fn() }),
}));

// Any test that toggles the view writes the cookie as a side effect; expire
// it unconditionally so one test's choice can't leak into the next.
afterEach(() => {
  document.cookie = `${VIEW_COOKIE_NAME}=; Path=/; Max-Age=0`;
});

function bookFixture(overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    id: "11111111-1111-1111-1111-111111111111",
    work_id: "22222222-2222-2222-2222-222222222222",
    title: "The Brothers Karamazov",
    subtitle: null,
    authors: ["Fyodor Dostoevsky"],
    contributors: [],
    series: null,
    isbn_13: "9780374528379",
    pages: null,
    tags: [],
    genres: [],
    moods: [],
    content_rating: null,
    cover_url: "/api/v1/books/11111111/cover/thumb",
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    reading_state: null,
    created_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function detailFixture(base: BookListItem): BookDetail {
  return {
    id: base.id,
    work_id: base.work_id,
    title: base.title,
    authors: base.authors,
    contributors: [],
    series: base.series,
    description: null,
    language: null,
    isbn_13: base.isbn_13,
    isbn_10: null,
    publisher: "Farrar, Straus and Giroux",
    pub_date: null,
    cover_url: base.cover_url,
    tags: [],
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    metadata_version_summary: { pending: 0, accepted: 0 },
    metadata_versions: [],
    created_at: base.created_at,
    updated_at: base.created_at,
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
  /** Prefill book-detail cache slots so the drawer renders without a fetch. */
  details?: BookDetail[];
}

function findLibraryTable(): Promise<HTMLElement> {
  return screen.findByTestId("library-table", {}, { timeout: 3_000 });
}

function renderLibrary({
  items,
  nextCursor,
  initialEntries,
  cacheParams,
  extraCacheParams = [],
  me,
  details = [],
}: RenderOpts): {
  client: QueryClient;
} {
  const client = new QueryClient({
    // staleTime Infinity: prefilled cache slots must not background-refetch
    // (there is no network in these tests to serve the refetch).
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
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
  for (const detail of details) {
    client.setQueryData(queryKeys.books.detail(detail.id), detail);
  }

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
    const entry = initialEntries?.[0] ?? "/library";
    const queryIndex = entry.indexOf("?");
    const router = createMemoryRouter(routes, {
      initialEntries: initialEntries ?? ["/library"],
    });
    return (
      <QueryClientProvider client={client}>
        {/* resetUrlUpdateQueueOnMount is off deliberately: on it, the queue
            resets on every render rather than on mount, which drops queued
            updates mid-batch and makes debounced writes flaky. Per-flush
            hygiene still comes from autoResetQueueOnUpdate. */}
        <NuqsTestingAdapter
          searchParams={queryIndex === -1 ? "" : entry.slice(queryIndex)}
          resetUrlUpdateQueueOnMount={false}
        >
          <RouterProvider router={router} />
        </NuqsTestingAdapter>
      </QueryClientProvider>
    );
  }

  render(<Wrapper />);
  return { client };
}

/** The toolbar search box; its accessible name matches the placeholder. */
function searchBox(): HTMLElement {
  return screen.getByRole("searchbox", { name: "Search your library" });
}

/** A rail section's `<details>` inside the open filter drawer, found by its
 *  summary title (several sections share inner control labels). */
function drawerSection(drawer: HTMLElement, title: string): HTMLElement {
  const details = within(drawer)
    .getAllByText(title)
    .map((node) => node.closest("details"))
    .find((candidate): candidate is HTMLDetailsElement => candidate !== null);
  if (details === undefined) throw new Error(`no drawer section titled ${title}`);
  return details;
}

describe("LibraryPage", () => {
  test("renders the heading without a fabricated total", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    expect(await screen.findByRole("heading", { name: "Library" })).toBeInTheDocument();
    // items.length counts loaded pages only — a "N books" line would
    // misstate the true library total, so the page renders no count.
    expect(screen.queryByText(/1 book/)).not.toBeInTheDocument();
  });

  test("the title renders inside the hero band over the atmosphere layers", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    const heading = await screen.findByRole("heading", { name: "Library" });
    expect(heading.closest(".lib-hero")).not.toBeNull();
    // Decorative atmosphere layers mount once at the top of the page.
    expect(document.querySelector(".lib-atm")).not.toBeNull();
    expect(document.querySelector(".lib-grain")).not.toBeNull();
  });

  test("cinema-hint is always in the DOM so its CSS fade can play (not gated on state)", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    await screen.findByRole("heading", { name: "Library" });
    expect(document.querySelector(".cinema-hint")).not.toBeNull();
  });

  test("the toolbar hosts no result totals or sort menu", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?sort=title"],
      cacheParams: { sort: "title" },
    });
    await screen.findByRole("heading", { name: "Library" });
    expect(screen.queryByRole("button", { name: /^Sort:/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/All records/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Catalogue/)).not.toBeInTheDocument();
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

  test("the filter drawer lists distinct series from the loaded pages", async () => {
    renderLibrary({
      items: [
        bookFixture({ id: "a", series: { id: "s-1", name: "Discworld", position: 1 } }),
        bookFixture({ id: "b", series: { id: "s-1", name: "Discworld", position: 2 } }),
        bookFixture({ id: "c", series: null }),
      ],
      nextCursor: null,
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^Filters/ }));
    const drawer = await screen.findByRole("dialog");
    expect(within(drawer).getAllByRole("checkbox", { name: "Discworld" })).toHaveLength(1);
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

  test("view toggle group renders grid and table only; no list mode survives", async () => {
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    const group = await screen.findByRole("group", { name: "View mode" });
    expect(within(group).getByRole("button", { name: "Grid", pressed: true })).toBeInTheDocument();
    expect(
      within(group).getByRole("button", { name: "Table", pressed: false }),
    ).toBeInTheDocument();
    expect(within(group).queryByRole("button", { name: "List" })).not.toBeInTheDocument();
  });

  test("renders the table view when ?view=table (lazy-loaded)", async () => {
    renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      initialEntries: ["/library?view=table"],
    });
    expect(await findLibraryTable()).toBeInTheDocument();
  });

  test("cookie default mounts the table view when ?view= is absent", async () => {
    document.cookie = `${VIEW_COOKIE_NAME}=table; Path=/`;
    try {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      expect(await findLibraryTable()).toBeInTheDocument();
    } finally {
      // Explicit expiry so the cookie can't leak into a later test in this file.
      document.cookie = `${VIEW_COOKIE_NAME}=; Path=/; Max-Age=0`;
    }
  });

  test("stale ?view=list and unknown values fall back to grid", async () => {
    renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      initialEntries: ["/library?view=list"],
    });
    expect(await screen.findByTestId("library-grid")).toBeInTheDocument();
  });

  test("clicking a view toggle persists the choice to the cookie", async () => {
    try {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      const group = await screen.findByRole("group", { name: "View mode" });
      const user = userEvent.setup();
      await user.click(within(group).getByRole("button", { name: "Table" }));
      expect(document.cookie).toContain(`${VIEW_COOKIE_NAME}=table`);
    } finally {
      document.cookie = `${VIEW_COOKIE_NAME}=; Path=/; Max-Age=0`;
    }
  });

  test("URL ?view= beats the cookie default", async () => {
    document.cookie = `${VIEW_COOKIE_NAME}=table; Path=/`;
    try {
      renderLibrary({
        items: [bookFixture({ title: "Stoner" })],
        nextCursor: null,
        initialEntries: ["/library?view=grid"],
      });
      expect(await screen.findByTestId("library-grid")).toBeInTheDocument();
      expect(screen.queryByTestId("library-table")).not.toBeInTheDocument();
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
    await findLibraryTable();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("columnheader", { name: "Title" }));
    await waitFor(() => {
      const search = screen.getByTestId("location-search").textContent;
      expect(search).toContain("sort=title");
      expect(search).not.toContain("cursor=");
    });
  });

  test("a pending toolbar search and a header sort preserve each other (search first)", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?view=table"],
      extraCacheParams: [{ q: "war" }, { sort: "title" }, { q: "war", sort: "title" }],
    });
    await findLibraryTable();
    const user = userEvent.setup();
    await user.type(searchBox(), "war");
    await user.click(await screen.findByRole("columnheader", { name: "Title" }));
    await waitFor(() => {
      const search = screen.getByTestId("location-search").textContent;
      expect(search).toContain("q=war");
      expect(search).toContain("sort=title");
    });
  });

  test("a header sort and a following toolbar search preserve each other (sort first)", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?view=table"],
      extraCacheParams: [{ q: "war" }, { sort: "title" }, { q: "war", sort: "title" }],
    });
    await findLibraryTable();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("columnheader", { name: "Title" }));
    await user.type(searchBox(), "war");
    await waitFor(() => {
      const search = screen.getByTestId("location-search").textContent;
      expect(search).toContain("q=war");
      expect(search).toContain("sort=title");
    });
  });

  test("the toolbar search writes ?q= and clears any cursor param", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?cursor=stale123"],
      extraCacheParams: [{ q: "war" }],
    });
    await screen.findByRole("heading", { name: "Library" });
    const user = userEvent.setup();
    await user.type(searchBox(), "war");
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

  test("dead params neither light the filter badge nor flip the empty state", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?title_empty=true&subtitle_eq=x&subtitle_ne=y&isbn_13_ne=z"],
    });
    // None of these reach the API, so the page must read as unfiltered: the
    // onboarding empty state, and a Filters trigger with no count badge
    // (a badge would change its accessible name to "Filters N").
    expect(await screen.findByText("No books yet")).toBeInTheDocument();
    expect(screen.queryByText("No books match these filters")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Filters" })).toBeInTheDocument();
  });

  test("a malformed live-key value neither lights the badge nor flips the empty state", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?pages_gte=abc&created_at_gte=notadate&status_any=bogus"],
    });
    // The codec drops all three values, so nothing reaches the wire: the
    // page must read as unfiltered even though the live keys are present.
    expect(await screen.findByText("No books yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Filters" })).toBeInTheDocument();
  });

  test("a well-formed filter lights the badge with the sent-param count", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?pages_gte=500"],
      cacheParams: { pages_gte: 500 },
      extraCacheParams: [{}],
    });
    expect(await screen.findByText("No books match these filters")).toBeInTheDocument();
    // The count badge renders inside the trigger, so it joins the accessible
    // name with no separator.
    expect(screen.getByRole("button", { name: "Filters1" })).toBeInTheDocument();
  });

  test("clear-all also sweeps dead params off the URL", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?series=s-1&title_empty=true"],
      cacheParams: { series: "s-1" },
      extraCacheParams: [{}],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    await waitFor(() => {
      const search = screen.getByTestId("location-search").textContent;
      expect(search).not.toContain("series");
      expect(search).not.toContain("title_empty");
    });
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

  test("the sort live region announces the stack in every state", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      initialEntries: ["/library?sort=-pages"],
      cacheParams: { sort: "-pages" },
    });
    await screen.findByRole("heading", { name: "Library" });
    const region = screen.getByText("Sorted by Pages descending");
    expect(region).toHaveAttribute("aria-live", "polite");
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

  test("clear-all discards a pending toolbar-search draft; the cleared filter cannot resurrect", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?series=s-1"],
      cacheParams: { series: "s-1" },
      extraCacheParams: [{}, { q: "du" }, { series: "s-1", q: "du" }],
    });
    const user = userEvent.setup();
    await screen.findByText("No books match these filters");
    await user.type(searchBox(), "du");
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    // Past the debounce window: the pending draft must not have committed.
    await new Promise((resolve) => setTimeout(resolve, 400));
    const search = screen.getByTestId("location-search").textContent;
    expect(search).not.toContain("q=");
    expect(search).not.toContain("series=");
    expect(searchBox()).toHaveValue("");
  });

  test("clearing all filters preserves the active view and sort params", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      initialEntries: ["/library?series=s-1&sort=title&view=table"],
      cacheParams: { series: "s-1", sort: "title" },
      extraCacheParams: [{ sort: "title" }],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    await waitFor(() => {
      const search = screen.getByTestId("location-search").textContent;
      expect(search).toContain("view=table");
      expect(search).toContain("sort=title");
      expect(search).not.toContain("series=");
    });
    expect(screen.getByRole("button", { name: "Table", pressed: true })).toBeInTheDocument();
  });

  describe("filter chips", () => {
    test("renders no chip row when no filter params are set", async () => {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      await screen.findByRole("heading", { name: "Library" });
      expect(screen.queryByTestId("filter-chips")).not.toBeInTheDocument();
    });

    test("an active series filter renders a chip with the resolved name, not a raw id", async () => {
      renderLibrary({
        items: [bookFixture({ id: "a", series: { id: "s-1", name: "Discworld", position: 1 } })],
        nextCursor: null,
        initialEntries: ["/library?series=s-1"],
        cacheParams: { series: "s-1" },
      });
      const chips = await screen.findByTestId("filter-chips");
      expect(within(chips).getByText(/Series: Discworld/)).toBeInTheDocument();
      expect(within(chips).queryByText(/s-1/)).not.toBeInTheDocument();
    });

    test("a chip's remove control drops exactly that filter param", async () => {
      renderLibrary({
        items: [bookFixture({ id: "a", series: { id: "s-1", name: "Discworld", position: 1 } })],
        nextCursor: null,
        initialEntries: ["/library?series=s-1&status_any=unread"],
        cacheParams: { series: "s-1", status_any: ["unread"] },
        extraCacheParams: [{ status_any: ["unread"] }],
      });
      const chips = await screen.findByTestId("filter-chips");
      const user = userEvent.setup();
      await user.click(
        within(chips).getByRole("button", { name: /Remove filter: Series: Discworld/ }),
      );
      await waitFor(() => {
        const search = screen.getByTestId("location-search").textContent;
        expect(search).not.toContain("series=");
        expect(search).toContain("status_any=unread");
      });
    });

    test("the chip row's clear-all drops every filter param", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        initialEntries: ["/library?status_any=unread&pages_gte=500"],
        cacheParams: { status_any: ["unread"], pages_gte: 500 },
        extraCacheParams: [{}],
      });
      const chips = await screen.findByTestId("filter-chips");
      const user = userEvent.setup();
      await user.click(within(chips).getByRole("button", { name: "Clear all" }));
      await waitFor(() => {
        const search = screen.getByTestId("location-search").textContent;
        expect(search).not.toContain("status_any");
        expect(search).not.toContain("pages_gte");
      });
    });

    test("multi-token vocabulary filters render one family chip with a count", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        initialEntries: ["/library?tag=scifi&tag=hugo"],
        cacheParams: { tag: ["scifi", "hugo"] },
      });
      const chips = await screen.findByTestId("filter-chips");
      expect(within(chips).getByText(/Tags \(2\)/)).toBeInTheDocument();
    });

    test("a chip falls back to a short id when the series is absent from the loaded pages", async () => {
      const longId = "aaaabbbb-cccc-dddd-eeee-ffffffffffff";
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        initialEntries: [`/library?series=${longId}`],
        cacheParams: { series: longId },
      });
      const chips = await screen.findByTestId("filter-chips");
      expect(within(chips).getByText(/Series: aaaabbbb…/)).toBeInTheDocument();
      expect(within(chips).queryByText(/undefined/)).not.toBeInTheDocument();
    });
  });

  describe("overlays", () => {
    test("the Filters button opens the filter drawer; Escape closes it and returns focus", async () => {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      const user = userEvent.setup();
      const trigger = await screen.findByRole("button", { name: /^Filters/ });
      expect(trigger).toHaveAttribute("aria-expanded", "false");
      await user.click(trigger);
      const drawer = await screen.findByRole("dialog");
      expect(within(drawer).getByText("Refine")).toBeInTheDocument();
      await user.keyboard("{Escape}");
      await waitFor(() => {
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      });
      // Radix restores focus asynchronously on close.
      await waitFor(() => {
        expect(trigger).toHaveFocus();
      });
    });

    test("activating a table row opens the book-detail drawer with real record actions", async () => {
      const book = bookFixture({ title: "Stoner", id: "abc-123" });
      renderLibrary({
        items: [book],
        nextCursor: null,
        initialEntries: ["/library?view=table"],
        details: [detailFixture(book)],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(
        await screen.findByRole("button", { name: `Open details for ${book.title}` }),
      );
      const drawer = await screen.findByRole("dialog");
      expect(await within(drawer).findByText("Farrar, Straus and Giroux")).toBeInTheDocument();
      const fullRecord = within(drawer).getByRole("link", { name: /View full record/ });
      expect(fullRecord.getAttribute("href")).toBe(`/b/${book.id}`);
      expect(within(drawer).getByRole("button", { name: /Add to shelf/ })).toBeInTheDocument();
      // No mock-only actions survive from the prototype.
      expect(within(drawer).queryByRole("button", { name: /Copy title/ })).not.toBeInTheDocument();
      expect(within(drawer).queryByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
      await user.keyboard("{Escape}");
      await waitFor(() => {
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      });
    });

    test("detail drawer failure shows an alert with a working Retry", async () => {
      // Deliberate failure logs a console breadcrumb; keep the output clean.
      const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
      const book = bookFixture({ title: "Stoner", id: "abc-123" });
      const detail = detailFixture(book);
      // URL-keyed mock: only the detail endpoint fails (once), so a stray
      // fetch from an unrelated query cannot eat the scripted rejection.
      let detailCalls = 0;
      const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
        const url =
          typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
        if (url.includes(`/books/${book.id}`)) {
          detailCalls += 1;
          if (detailCalls === 1) return Promise.reject(new Error("network down"));
          return Promise.resolve(
            new Response(JSON.stringify(detail), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }),
          );
        }
        return Promise.resolve(
          new Response(JSON.stringify({ items: [], next_cursor: null }), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }),
        );
      });
      renderLibrary({
        items: [book],
        nextCursor: null,
        initialEntries: ["/library?view=table"],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(
        await screen.findByRole("button", { name: `Open details for ${book.title}` }),
      );
      const drawer = await screen.findByRole("dialog");
      expect(await within(drawer).findByRole("alert")).toHaveTextContent(/couldn't load/i);
      await user.click(within(drawer).getByRole("button", { name: "Retry" }));
      expect(await within(drawer).findByText("Farrar, Straus and Giroux")).toBeInTheDocument();
      fetchSpy.mockRestore();
      errorSpy.mockRestore();
    });

    test("detail drawer shows a busy skeleton while the record loads", async () => {
      const book = bookFixture({ title: "Stoner", id: "abc-123" });
      // A never-settling fetch pins the loading branch.
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => new Promise(() => undefined));
      renderLibrary({
        items: [book],
        nextCursor: null,
        initialEntries: ["/library?view=table"],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(
        await screen.findByRole("button", { name: `Open details for ${book.title}` }),
      );
      const drawer = await screen.findByRole("dialog");
      expect(drawer.querySelector('[aria-busy="true"]')).not.toBeNull();
      fetchSpy.mockRestore();
    });

    test("the overlay slot is exclusive: opening one drawer replaces the other", async () => {
      const book = bookFixture({ title: "Stoner", id: "abc-123" });
      renderLibrary({
        items: [book],
        nextCursor: null,
        initialEntries: ["/library?view=table"],
        details: [detailFixture(book)],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(
        await screen.findByRole("button", { name: `Open details for ${book.title}` }),
      );
      await screen.findByRole("dialog");
      await user.keyboard("{Escape}");
      await waitFor(() => {
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      });
      await user.click(screen.getByRole("button", { name: /^Filters/ }));
      const drawers = await screen.findAllByRole("dialog");
      expect(drawers).toHaveLength(1);
      expect(within(drawers[0]).getByText("Refine")).toBeInTheDocument();
    });
  });

  describe("drawer close semantics and cross-surface drafts", () => {
    test("a toolbar search draft survives a rail commit made from the drawer", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        extraCacheParams: [{ status_any: ["reading"] }],
      });
      await screen.findByRole("heading", { name: "Library" });
      const user = userEvent.setup();
      // One character: below MIN_Q_LEN, so it stays a pure draft forever.
      await user.type(searchBox(), "d");
      await user.click(screen.getByRole("button", { name: /^Filters/ }));
      const drawer = await screen.findByRole("dialog");
      await user.click(within(drawer).getByRole("checkbox", { name: "Reading" }));
      await user.keyboard("{Escape}");
      await waitFor(() => {
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      });
      const search = screen.getByTestId("location-search").textContent;
      expect(search).toContain("status_any=reading");
      expect(searchBox()).toHaveValue("d");
    });

    test("Escape abandons a pending rail draft", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        extraCacheParams: [{ title_contains: "dune" }],
      });
      await screen.findByRole("heading", { name: "Library" });
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: /^Filters/ }));
      const drawer = await screen.findByRole("dialog");
      const titleSection = drawerSection(drawer, "Title");
      await user.type(within(titleSection).getByRole("textbox", { name: "Filter value" }), "dune");
      await user.keyboard("{Escape}");
      await waitFor(() => {
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      });
      // Past the debounce window: the abandoned draft must never commit.
      await new Promise((resolve) => setTimeout(resolve, 400));
      expect(screen.getByTestId("location-search").textContent).not.toContain("title_contains");
    });

    test("a non-Escape close applies a pending rail draft", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        extraCacheParams: [{ title_contains: "sea" }],
      });
      await screen.findByRole("heading", { name: "Library" });
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: /^Filters/ }));
      const drawer = await screen.findByRole("dialog");
      const titleSection = drawerSection(drawer, "Title");
      await user.type(within(titleSection).getByRole("textbox", { name: "Filter value" }), "sea");
      await user.click(within(drawer).getByRole("button", { name: "Close" }));
      await waitFor(() => {
        expect(screen.getByTestId("location-search").textContent).toContain("title_contains=sea");
      });
    });
  });

  describe("selection and batch bar", () => {
    test("selecting rows raises the batch bar with count, clear, and no Remove action", async () => {
      renderLibrary({
        items: [bookFixture({ id: "a", title: "Stoner" }), bookFixture({ id: "b" })],
        nextCursor: null,
        initialEntries: ["/library?view=table"],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getAllByRole("checkbox", { name: "Select" })[0]);
      const bar = await screen.findByRole("toolbar", { name: "Batch actions" });
      expect(within(bar).getByText("1 selected")).toBeInTheDocument();
      expect(within(bar).getByRole("button", { name: /Add to shelf/ })).toBeInTheDocument();
      expect(within(bar).queryByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
      await user.click(within(bar).getByRole("button", { name: /Clear selection/ }));
      expect(screen.queryByRole("toolbar", { name: "Batch actions" })).not.toBeInTheDocument();
    });

    test("the batch bar discloses loaded-only coverage when more pages exist", async () => {
      renderLibrary({
        items: [bookFixture({ id: "a" })],
        nextCursor: "eyJ4Ijox",
        initialEntries: ["/library?view=table"],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getByRole("checkbox", { name: "Select all loaded books" }));
      const bar = await screen.findByRole("toolbar", { name: "Batch actions" });
      expect(within(bar).getByText("Selection covers loaded books only")).toBeInTheDocument();
    });

    test("a query change clears the selection", async () => {
      renderLibrary({
        items: [bookFixture({ id: "a" })],
        nextCursor: null,
        initialEntries: ["/library?view=table"],
        extraCacheParams: [{ q: "war" }],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getAllByRole("checkbox", { name: "Select" })[0]);
      await screen.findByRole("toolbar", { name: "Batch actions" });
      await user.type(searchBox(), "war");
      await waitFor(() => {
        expect(screen.queryByRole("toolbar", { name: "Batch actions" })).not.toBeInTheDocument();
      });
    });

    test("switching projections clears the selection", async () => {
      renderLibrary({
        items: [bookFixture({ id: "a" })],
        nextCursor: null,
        initialEntries: ["/library?view=table"],
        extraCacheParams: [{}],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getAllByRole("checkbox", { name: "Select" })[0]);
      await screen.findByRole("toolbar", { name: "Batch actions" });
      const group = screen.getByRole("group", { name: "View mode" });
      await user.click(within(group).getByRole("button", { name: "Grid" }));
      await screen.findByTestId("library-grid");
      await user.click(within(group).getByRole("button", { name: "Table" }));
      await screen.findByTestId("library-table");
      expect(screen.queryByRole("toolbar", { name: "Batch actions" })).not.toBeInTheDocument();
    });
  });

  describe("display preferences", () => {
    test("table-mode display controls are absent in grid mode", async () => {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      await screen.findByTestId("library-grid");
      expect(screen.queryByRole("group", { name: "Table density" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Columns/ })).not.toBeInTheDocument();
    });

    test("the density toggle switches and persists the preference", async () => {
      try {
        renderLibrary({
          items: [bookFixture()],
          nextCursor: null,
          initialEntries: ["/library?view=table"],
        });
        await screen.findByTestId("library-table");
        const group = screen.getByRole("group", { name: "Table density" });
        expect(
          within(group).getByRole("button", { name: "Comfortable", pressed: true }),
        ).toBeInTheDocument();
        const user = userEvent.setup();
        await user.click(within(group).getByRole("button", { name: "Compact" }));
        expect(
          within(group).getByRole("button", { name: "Compact", pressed: true }),
        ).toBeInTheDocument();
        expect(localStorage.getItem(DISPLAY_STORAGE_KEY)).toContain('"compact"');
      } finally {
        localStorage.removeItem(DISPLAY_STORAGE_KEY);
      }
    });

    test("the Columns popover hides a column, locks Title, and persists the choice", async () => {
      try {
        renderLibrary({
          items: [bookFixture()],
          nextCursor: null,
          initialEntries: ["/library?view=table"],
        });
        await screen.findByTestId("library-table");
        expect(screen.getByRole("columnheader", { name: /^Subtitle/ })).toBeInTheDocument();
        const user = userEvent.setup();
        await user.click(screen.getByRole("button", { name: /Columns/ }));
        const title = await screen.findByRole("checkbox", { name: "Title" });
        expect(title).toBeDisabled();
        await user.click(screen.getByRole("checkbox", { name: "Subtitle" }));
        await waitFor(() => {
          expect(screen.queryByRole("columnheader", { name: /^Subtitle/ })).not.toBeInTheDocument();
        });
        expect(localStorage.getItem(DISPLAY_STORAGE_KEY)).toContain("subtitle");
      } finally {
        localStorage.removeItem(DISPLAY_STORAGE_KEY);
      }
    });

    test("a persisted hidden column seeds the table without it", async () => {
      localStorage.setItem(
        DISPLAY_STORAGE_KEY,
        JSON.stringify({ density: "comfortable", hiddenColumns: ["subtitle"] }),
      );
      try {
        renderLibrary({
          items: [bookFixture()],
          nextCursor: null,
          initialEntries: ["/library?view=table"],
        });
        await screen.findByTestId("library-table");
        expect(screen.queryByRole("columnheader", { name: /^Subtitle/ })).not.toBeInTheDocument();
        expect(screen.getByRole("columnheader", { name: /^Title/ })).toBeInTheDocument();
      } finally {
        localStorage.removeItem(DISPLAY_STORAGE_KEY);
      }
    });
  });
});
