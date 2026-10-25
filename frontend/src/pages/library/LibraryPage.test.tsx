import { QueryClient, QueryClientProvider, type InfiniteData } from "@tanstack/react-query";
import { cleanup, act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import {
  emptyFilterState,
  filterStateToParams,
  LibraryVisitSchema,
  type FilterState,
  type LibraryVisit,
} from "@/routes/library-params";
import { RouterProvider, createBrowserRouter, type RouteObject } from "react-router";
import { useSyncExternalStore, type ReactElement } from "react";

import type {
  BookDetail,
  BookListItem,
  BookListResponse,
  ListBooksParams,
  Preferences,
} from "@/api";
import type { AuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import type { useCellEdit as useCellEditType, UseCellEditOptions } from "./table/useCellEdit";

import { CommandPalette } from "@/components/CommandPalette";

import { rememberActiveUser } from "@/lib/active-user";

import { displayStorageKey } from "./display-storage";
import { LibraryPage } from "./LibraryPage";

const { observedListKeys } = vi.hoisted(() => ({ observedListKeys: [] as unknown[] }));
vi.mock("./table/useCellEdit", async (importOriginal) => {
  const original = await importOriginal<{ useCellEdit: typeof useCellEditType }>();
  return {
    ...original,
    useCellEdit: (options: UseCellEditOptions) => {
      observedListKeys.push(options.listKey);
      return original.useCellEdit(options);
    },
  };
});

// The masthead reads the effective theme to pick its hero asset; these tests
// exercise the page, not theme resolution, so the provider is stubbed.
vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ preference: "system", effective: "dark", setPreference: vi.fn() }),
}));

afterEach(() => {
  // The page mirrors its effective preferences to localStorage on every
  // change, so one test's choice would otherwise seed the next one's paint.
  localStorage.clear();
  window.history.replaceState(null, "", "/library");
  cleanup();
  vi.unstubAllGlobals();
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
    has_local_password: true,
    theme_preference: "system",
    csrf_token: null,
    ...overrides,
  };
}

/** Installation defaults as `backend/src/models/user_preferences.rs` serves them. */
const PREFERENCE_DEFAULTS: Preferences["defaults"] = {
  hidden_columns: [],
  density: "comfortable",
  view: "grid",
  sort_stack: "-created_at",
};

function preferencesFixture(overrides: Partial<Preferences> = {}): Preferences {
  return {
    hidden_columns: null,
    density: null,
    view: null,
    sort_stack: null,
    defaults: PREFERENCE_DEFAULTS,
    ...overrides,
  };
}

function preferencesResponse(body: Preferences): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

type FetchCall = [input: RequestInfo | URL, init?: RequestInit];

/** The preference PATCH bodies a spied `fetch` observed, oldest first. */
function preferenceWrites(spy: { mock: { calls: FetchCall[] } }): unknown[] {
  return spy.mock.calls
    .filter(([input, init]) => input === PREFERENCES_PATH && init?.method === "PATCH")
    .map(([, init]) => parseJsonBody(init?.body));
}

function parseJsonBody(body: BodyInit | null | undefined): unknown {
  if (typeof body !== "string") throw new Error("expected a stringified JSON body");
  return JSON.parse(body) as unknown;
}

const PREFERENCES_PATH = "/auth/me/preferences";

interface RenderOpts {
  items: BookListItem[];
  nextCursor: string | null;
  visit?: { filters?: Partial<FilterState>; view?: LibraryVisit["view"] };
  /**
   * Preferences to serve from cache. `null` leaves the slot empty, which is
   * how a test exercises the window before the response lands; the test then
   * owns the fetch mock for it.
   */
  preferences?: Preferences | null;
  /** Params shape to prefill cache under. Defaults to the empty-params slot. */
  cacheParams?: ListBooksParams;
  /** Additional cache slots to prefill (e.g. a post-interaction sort key). */
  extraCacheParams?: ListBooksParams[];
  /** Prefill the `/auth/me` cache so role-gated UI renders without a fetch. */
  me?: AuthMe;
  /** Prefill book-detail cache slots so the drawer renders without a fetch. */
  details?: BookDetail[];
  withPalette?: boolean;
  uncached?: boolean;
}

function findLibraryTable(): Promise<HTMLElement> {
  return screen.findByTestId("library-table", {}, { timeout: 3_000 });
}

function renderLibrary({
  items,
  nextCursor,
  visit,
  preferences,
  cacheParams,
  extraCacheParams = [],
  me,
  details = [],
  withPalette = false,
  uncached = false,
}: RenderOpts): {
  client: QueryClient;
} {
  const client = new QueryClient({
    // staleTime Infinity: prefilled cache slots must not background-refetch
    // (there is no network in these tests to serve the refetch).
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.shelves.list(), []);
  const params: ListBooksParams = cacheParams ?? {};
  const response: BookListResponse = { items, next_cursor: nextCursor };
  for (const slot of uncached ? [] : [params, ...extraCacheParams]) {
    client.setQueryData(queryKeys.books.list(slot), {
      pages: [response],
      pageParams: [undefined],
    });
  }
  if (preferences !== null) {
    client.setQueryData(queryKeys.auth.preferences(), preferences ?? preferencesFixture());
  }
  if (me !== undefined) client.setQueryData(queryKeys.auth.me(), me);
  for (const detail of details) {
    client.setQueryData(queryKeys.books.detail(detail.id), detail);
  }

  let entryState: Record<string, unknown> = {
    reverieLibrary: { filters: { ...emptyFilterState(), ...visit?.filters }, view: visit?.view },
  };
  const events = new EventTarget();
  vi.stubGlobal("navigation", {
    currentEntry: { key: "library-test", getState: () => entryState },
    updateCurrentEntry: ({ state }: { state: Record<string, unknown> }) => {
      entryState = state;
      events.dispatchEvent(
        Object.assign(new Event("currententrychange"), { navigationType: null }),
      );
    },
    addEventListener: events.addEventListener.bind(events),
    removeEventListener: events.removeEventListener.bind(events),
  });
  function LocationProbe(): ReactElement {
    const state = useSyncExternalStore(
      (onChange) => {
        events.addEventListener("currententrychange", onChange);
        return () => {
          events.removeEventListener("currententrychange", onChange);
        };
      },
      () => entryState,
    );
    const restored = LibraryVisitSchema.parse(state.reverieLibrary);
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(filterStateToParams(restored.filters))) {
      if (Array.isArray(value)) for (const item of value) params.append(key, item);
      else params.set(key, String(value));
    }
    if (restored.view !== undefined) params.set("view", restored.view);
    return <div data-testid="visit-params">{params.toString()}</div>;
  }
  window.history.replaceState(null, "", "/library");

  function Wrapper(): ReactElement {
    const routes: RouteObject[] = [
      {
        path: "/library",
        element: (
          <>
            <LibraryPage />
            {withPalette && <CommandPalette />}
            <LocationProbe />
          </>
        ),
      },
    ];
    const router = createBrowserRouter(routes);
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
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

/** A rail section inside the open filter drawer, found by its title
 *  (several sections share inner control labels) and expanded if collapsed,
 *  because collapsed content is hidden from queries. */
function drawerSection(drawer: HTMLElement, title: string): HTMLElement {
  const section = within(drawer)
    .getAllByRole("button", { name: new RegExp(`^${title}(?:\\s*\\d+)?$`) })
    .map((node) => node.closest("section"))
    .find((candidate): candidate is HTMLElement => candidate !== null);
  if (section === undefined) throw new Error(`no drawer section titled ${title}`);
  const disclosure = section.querySelector("button[slot='trigger'][aria-expanded='false']");
  if (disclosure !== null) fireEvent.click(disclosure);
  return section;
}

describe("LibraryPage", () => {
  test.each(["AC/DC", "fantasy", "why?"])(
    "palette accepts %s without invoking library shortcuts",
    async (query) => {
      const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() =>
        Promise.resolve(
          new Response(JSON.stringify({ items: [] }), {
            headers: { "Content-Type": "application/json" },
          }),
        ),
      );
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        me: meFixture(),
        withPalette: true,
      });
      await findLibraryTable();
      const user = userEvent.setup();
      await user.keyboard("{Control>}k{/Control}");
      const palette = await screen.findByRole("dialog", { name: "Search library" });
      const input = within(palette).getByRole("searchbox");
      await user.type(input, query);
      expect(input).toHaveValue(query);
      expect(document.documentElement).not.toHaveAttribute("data-cinematic");
      expect(screen.queryByRole("dialog", { name: /Keyboard shortcuts/i })).not.toBeInTheDocument();
      fetchSpy.mockRestore();
    },
  );

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
      preferences: preferencesFixture({ sort_stack: "title" }),
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
        bookFixture({
          id: "a",
          series: { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", name: "Discworld", position: 1 },
        }),
        bookFixture({
          id: "b",
          series: { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", name: "Discworld", position: 2 },
        }),
        bookFixture({ id: "c", series: null }),
      ],
      nextCursor: null,
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^Filters/ }));
    const drawer = await screen.findByRole("dialog");
    drawerSection(drawer, "Series");
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
    const group = await screen.findByRole("radiogroup", { name: "View mode" });
    expect(within(group).getByRole("radio", { name: "Grid", checked: true })).toBeInTheDocument();
    expect(within(group).getByRole("radio", { name: "Table", checked: false })).toBeInTheDocument();
    expect(within(group).queryByRole("radio", { name: "List" })).not.toBeInTheDocument();
  });

  test("keyboard choices keep view and density nonempty and exclusive", async () => {
    const unrelated = { hidden_columns: ["subtitle"], sort_stack: "title" };
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(
        preferencesResponse(
          preferencesFixture({ ...unrelated, view: "table", density: "compact" }),
        ),
      );
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      preferences: preferencesFixture(unrelated),
      cacheParams: { sort: "title" },
    });
    const user = userEvent.setup();
    const view = await screen.findByRole("radiogroup", { name: "View mode" });
    await user.click(within(view).getByRole("radio", { name: "Grid" }));
    await user.keyboard("{ArrowRight}");
    expect(within(view).getByRole("radio", { name: "Table" })).toHaveFocus();
    await user.keyboard(" ");
    expect(within(view).getByRole("radio", { name: "Table" })).toBeChecked();
    expect(within(view).getByRole("radio", { name: "Grid" })).not.toBeChecked();
    await waitFor(() => {
      expect(screen.getByTestId("visit-params")).toHaveTextContent("view=table");
    });
    await user.keyboard(" ");
    expect(within(view).getByRole("radio", { name: "Table" })).toBeChecked();
    const density = screen.getByRole("radiogroup", { name: "Table density" });
    await user.click(within(density).getByRole("radio", { name: "Comfortable" }));
    await user.keyboard("{ArrowRight}");
    expect(within(density).getByRole("radio", { name: "Compact" })).toHaveFocus();
    await user.keyboard(" ");
    expect(within(density).getByRole("radio", { name: "Compact" })).toBeChecked();
    expect(within(density).getByRole("radio", { name: "Comfortable" })).not.toBeChecked();
    await user.keyboard(" ");
    expect(within(density).getByRole("radio", { name: "Compact" })).toBeChecked();
    await waitFor(() => {
      expect(Object.assign({}, unrelated, ...preferenceWrites(fetchSpy))).toEqual({
        ...unrelated,
        view: "table",
        density: "compact",
      });
    });
    fetchSpy.mockRestore();
  });

  test("renders the table view for a visit override (lazy-loaded)", async () => {
    renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      visit: { view: "table" },
    });
    expect(await findLibraryTable()).toBeInTheDocument();
  });

  test("the stored view preference mounts the table when the visit override is absent", async () => {
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      preferences: preferencesFixture({ view: "table" }),
    });
    expect(await findLibraryTable()).toBeInTheDocument();
  });

  test("clicking a view toggle persists the choice to the account", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(preferencesResponse(preferencesFixture({ view: "table" })));
    renderLibrary({ items: [bookFixture()], nextCursor: null });
    const group = await screen.findByRole("radiogroup", { name: "View mode" });
    const user = userEvent.setup();
    await user.click(within(group).getByRole("radio", { name: "Table" }));
    await waitFor(() => {
      expect(preferenceWrites(fetchSpy)).toEqual([{ view: "table" }]);
    });
    fetchSpy.mockRestore();
  });

  test("Visit view beats the stored view preference", async () => {
    renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      visit: { view: "grid" },
      preferences: preferencesFixture({ view: "table" }),
    });
    expect(await screen.findByTestId("library-grid")).toBeInTheDocument();
    expect(screen.queryByTestId("library-table")).not.toBeInTheDocument();
  });

  test("table header click persists the sort to the account and never touches the URL", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(preferencesResponse(preferencesFixture({ sort_stack: "title" })));
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      visit: { view: "table" },
      extraCacheParams: [{ sort: "title" }],
    });
    await findLibraryTable();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("columnheader", { name: "Title" }));
    await waitFor(() => {
      expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
    });
    expect(screen.getByTestId("visit-params").textContent).not.toContain("sort=");
    fetchSpy.mockRestore();
  });

  test("toolbar search and a header sort preserve each other (search first)", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input) =>
      Promise.resolve(
        input === PREFERENCES_PATH
          ? preferencesResponse(preferencesFixture({ sort_stack: "title" }))
          : new Response(JSON.stringify({ items: [bookFixture()], next_cursor: null }), {
              headers: { "Content-Type": "application/json" },
            }),
      ),
    );
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      visit: { view: "table" },
      extraCacheParams: [{ q: "war" }, { sort: "title" }, { q: "war", sort: "title" }],
    });
    await findLibraryTable();
    const user = userEvent.setup();
    await user.type(searchBox(), "war");
    await user.click(await screen.findByRole("columnheader", { name: "Title" }));
    await waitFor(() => {
      expect(screen.getByTestId("visit-params").textContent).toContain("q=war");
      expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
    });
    fetchSpy.mockRestore();
  });

  test("a header sort and a following toolbar search preserve each other (sort first)", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input) =>
      Promise.resolve(
        input === PREFERENCES_PATH
          ? preferencesResponse(preferencesFixture({ sort_stack: "title" }))
          : new Response(JSON.stringify({ items: [bookFixture()], next_cursor: null }), {
              headers: { "Content-Type": "application/json" },
            }),
      ),
    );
    renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      visit: { view: "table" },
      extraCacheParams: [{ q: "war" }, { sort: "title" }, { q: "war", sort: "title" }],
    });
    await findLibraryTable();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("columnheader", { name: "Title" }));
    await user.type(searchBox(), "war");
    await waitFor(() => {
      expect(screen.getByTestId("visit-params").textContent).toContain("q=war");
      expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
    });
    fetchSpy.mockRestore();
  });

  test("a typed filter with no matches shows the filtered-empty state", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { status: { any: ["unread"], none: [] } } },
      cacheParams: { status_any: ["unread"] },
      extraCacheParams: [{}],
    });
    expect(await screen.findByText("No books match these filters")).toBeInTheDocument();
  });

  test("clear-all from the filtered-empty state drops typed filter params", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { status: { any: ["unread"], none: [] }, pages: { gte: 500 } } },
      cacheParams: { status_any: ["unread"], pages_gte: 500 },
      extraCacheParams: [{}],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    await waitFor(() => {
      const search = screen.getByTestId("visit-params").textContent;
      expect(search).not.toContain("status_any");
      expect(search).not.toContain("pages_gte");
    });
  });

  test("a well-formed filter lights the badge with the sent-param count", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { pages: { gte: 500 } } },
      cacheParams: { pages_gte: 500 },
      extraCacheParams: [{}],
    });
    expect(await screen.findByText("No books match these filters")).toBeInTheDocument();
    // The count badge renders inside the trigger, so it joins the accessible
    // name with no separator.
    expect(screen.getByRole("button", { name: "Filters1" })).toBeInTheDocument();
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
      visit: { filters: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" } },
      cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" },
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
      visit: { filters: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" } },
      cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" },
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
      preferences: preferencesFixture({ sort_stack: "-pages" }),
      cacheParams: { sort: "-pages" },
    });
    await screen.findByRole("heading", { name: "Library" });
    const region = screen.getByText("Sorted by Pages descending");
    expect(region).toHaveAttribute("aria-live", "polite");
  });

  test("a failed Load more surfaces an inline error and a Retry control", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network down"));
    renderLibrary({ items: [bookFixture()], nextCursor: "eyJ4Ijox" });
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
    renderLibrary({
      items: [bookFixture({ title: "The Brothers Karamazov" })],
      nextCursor: "eyJ4Ijox",
    });
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

  test("clear-all discards a toolbar search; the cleared filter cannot resurrect", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" } },
      cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" },
      extraCacheParams: [
        {},
        { q: "du" },
        { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", q: "du" },
      ],
    });
    const user = userEvent.setup();
    await screen.findByText("No books match these filters");
    await user.type(searchBox(), "du");
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    const search = screen.getByTestId("visit-params").textContent;
    expect(search).not.toContain("q=");
    expect(search).not.toContain("series=");
    expect(searchBox()).toHaveValue("");
  });

  test("clearing all filters preserves the visit view and the stored sort", async () => {
    renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" }, view: "table" },
      preferences: preferencesFixture({ sort_stack: "title" }),
      cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", sort: "title" },
      extraCacheParams: [{ sort: "title" }],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /clear all filters/i }));
    await waitFor(() => {
      const search = screen.getByTestId("visit-params").textContent;
      expect(search).toContain("view=table");
      expect(search).not.toContain("series=");
    });
    // The stored sort is not a filter: it survives clear-all untouched.
    expect(screen.getByText("Sorted by Title ascending")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Table", checked: true })).toBeInTheDocument();
  });

  describe("filter chips", () => {
    test("pointer removal retains adjacent focus and final removal focuses the empty collection", async () => {
      renderLibrary({
        items: [
          bookFixture({
            series: { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", name: "Discworld", position: 1 },
          }),
        ],
        nextCursor: null,
        visit: {
          filters: {
            series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            status: { any: ["unread"], none: [] },
          },
        },
        cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", status_any: ["unread"] },
        extraCacheParams: [{ status_any: ["unread"] }, {}],
      });
      const user = userEvent.setup();
      const chips = await screen.findByTestId("filter-chips");
      await user.click(within(chips).getByRole("button", { name: /Remove.*Series: Discworld/ }));
      await waitFor(() => {
        expect(within(chips).getByRole("row")).toHaveFocus();
      });
      await user.click(within(chips).getByRole("button", { name: /Remove/ }));
      await waitFor(() => {
        expect(within(chips).getByRole("group", { name: "Active filters" })).toHaveFocus();
      });
      expect(within(chips).getByText("No active filters.")).toBeInTheDocument();
      expect(within(chips).getByRole("button", { name: "Clear all" })).toBeDisabled();
    });

    test.each(["{Delete}", "{Backspace}"])(
      "%s removes the focused tag and preserves its neighbour",
      async (key) => {
        renderLibrary({
          items: [bookFixture()],
          nextCursor: null,
          visit: { filters: { status: { any: ["unread"], none: [] }, pages: { gte: 500 } } },
          cacheParams: { status_any: ["unread"], pages_gte: 500 },
          extraCacheParams: [{ pages_gte: 500 }, {}],
        });
        const user = userEvent.setup();
        const chips = await screen.findByTestId("filter-chips");
        const tags = within(chips).getAllByRole("row");
        await user.click(tags[0]);
        await user.keyboard(key);
        await waitFor(() => {
          expect(within(chips).getAllByRole("row")).toHaveLength(1);
          expect(within(chips).getByRole("row")).toHaveFocus();
        });
        await user.keyboard(key);
        await waitFor(() => {
          expect(within(chips).getByRole("group", { name: "Active filters" })).toHaveFocus();
        });
        await user.tab({ shift: true });
        await user.tab();
        expect(within(chips).getByRole("group", { name: "Active filters" })).toHaveFocus();
      },
    );

    test("keeps the empty collection reachable when no filter params are set", async () => {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      await screen.findByRole("heading", { name: "Library" });
      const chips = screen.getByTestId("filter-chips");
      expect(within(chips).getByText("No active filters.")).toBeInTheDocument();
      expect(within(chips).getByRole("group", { name: "Active filters" })).toHaveAttribute(
        "tabindex",
        "0",
      );
      expect(within(chips).getByRole("button", { name: "Clear all" })).toBeDisabled();
    });

    test("an active series filter renders a chip with the resolved name, not a raw id", async () => {
      renderLibrary({
        items: [
          bookFixture({
            id: "a",
            series: { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", name: "Discworld", position: 1 },
          }),
        ],
        nextCursor: null,
        visit: { filters: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" } },
        cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" },
      });
      const chips = await screen.findByTestId("filter-chips");
      expect(within(chips).getByText(/Series: Discworld/)).toBeInTheDocument();
      expect(within(chips).queryByText(/s-1/)).not.toBeInTheDocument();
    });

    test("a chip's remove control drops exactly that filter param", async () => {
      renderLibrary({
        items: [
          bookFixture({
            id: "a",
            series: { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", name: "Discworld", position: 1 },
          }),
        ],
        nextCursor: null,
        visit: {
          filters: {
            series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            status: { any: ["unread"], none: [] },
          },
        },
        cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", status_any: ["unread"] },
        extraCacheParams: [{ status_any: ["unread"] }],
      });
      const chips = await screen.findByTestId("filter-chips");
      const user = userEvent.setup();
      await user.click(within(chips).getByRole("button", { name: /Remove.*Series: Discworld/ }));
      await waitFor(() => {
        const search = screen.getByTestId("visit-params").textContent;
        expect(search).not.toContain("series=");
        expect(search).toContain("status_any=unread");
      });
    });

    test("the chip row's clear-all drops every filter param", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { filters: { status: { any: ["unread"], none: [] }, pages: { gte: 500 } } },
        cacheParams: { status_any: ["unread"], pages_gte: 500 },
        extraCacheParams: [{}],
      });
      const chips = await screen.findByTestId("filter-chips");
      const user = userEvent.setup();
      await user.click(within(chips).getByRole("button", { name: "Clear all" }));
      await waitFor(() => {
        const search = screen.getByTestId("visit-params").textContent;
        expect(search).not.toContain("status_any");
        expect(search).not.toContain("pages_gte");
      });
    });

    test("multi-token vocabulary filters render one family chip with a count", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { filters: { tags: { all: ["scifi", "hugo"], any: [], none: [] } } },
        cacheParams: { tag: ["scifi", "hugo"] },
      });
      const chips = await screen.findByTestId("filter-chips");
      expect(within(chips).getByText(/Tags \(2\)/)).toBeInTheDocument();
    });

    test("a chip falls back to a short id when the series is absent from the loaded pages", async () => {
      const longId = "aaaabbbb-cccc-4ddd-8eee-ffffffffffff";
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { filters: { series: longId } },
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
      await waitFor(() => {
        expect(trigger).toHaveFocus();
      });
    });

    test("activating a table row opens the book-detail drawer with real record actions", async () => {
      const book = bookFixture({ title: "Stoner", id: "abc-123" });
      renderLibrary({
        items: [book],
        nextCursor: null,
        visit: { view: "table" },
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
        visit: { view: "table" },
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
        visit: { view: "table" },
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
        visit: { view: "table" },
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

  describe("selection and batch bar", () => {
    test("selecting rows raises the batch bar with count, clear, and no Remove action", async () => {
      renderLibrary({
        items: [bookFixture({ id: "a", title: "Stoner" }), bookFixture({ id: "b" })],
        nextCursor: null,
        visit: { view: "table" },
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
        visit: { view: "table" },
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
        visit: { view: "table" },
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
        visit: { view: "table" },
        extraCacheParams: [{}],
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getAllByRole("checkbox", { name: "Select" })[0]);
      await screen.findByRole("toolbar", { name: "Batch actions" });
      const group = screen.getByRole("radiogroup", { name: "View mode" });
      await user.click(within(group).getByRole("radio", { name: "Grid" }));
      await screen.findByTestId("library-grid");
      await user.click(within(group).getByRole("radio", { name: "Table" }));
      await screen.findByTestId("library-table");
      expect(screen.queryByRole("toolbar", { name: "Batch actions" })).not.toBeInTheDocument();
    });
  });

  describe("display preferences", () => {
    test("table-mode display controls are absent in grid mode", async () => {
      renderLibrary({ items: [bookFixture()], nextCursor: null });
      await screen.findByTestId("library-grid");
      expect(screen.queryByRole("radiogroup", { name: "Table density" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Columns/ })).not.toBeInTheDocument();
    });

    test("the density toggle switches and persists the preference to the account", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ density: "compact" })));
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await screen.findByTestId("library-table");
      const group = screen.getByRole("radiogroup", { name: "Table density" });
      expect(
        within(group).getByRole("radio", { name: "Comfortable", checked: true }),
      ).toBeInTheDocument();
      const user = userEvent.setup();
      await user.click(within(group).getByRole("radio", { name: "Compact" }));
      expect(
        within(group).getByRole("radio", { name: "Compact", checked: true }),
      ).toBeInTheDocument();
      await waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ density: "compact" }]);
      });
      fetchSpy.mockRestore();
    });

    test("the Columns popover hides a column, locks Title, and persists the choice", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(
          preferencesResponse(preferencesFixture({ hidden_columns: ["subtitle"] })),
        );
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
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
      await waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ hidden_columns: ["subtitle"] }]);
      });
      fetchSpy.mockRestore();
    });

    test("a stored hidden column seeds the table without it", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: preferencesFixture({ hidden_columns: ["subtitle"] }),
      });
      await screen.findByTestId("library-table");
      expect(screen.queryByRole("columnheader", { name: /^Subtitle/ })).not.toBeInTheDocument();
      expect(screen.getByRole("columnheader", { name: /^Title/ })).toBeInTheDocument();
    });

    test("several column toggles inside the debounce window send one merged patch", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => Promise.resolve(preferencesResponse(preferencesFixture())));
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: /Columns/ }));
      await user.click(await screen.findByRole("checkbox", { name: "Subtitle" }));
      await user.click(screen.getByRole("checkbox", { name: "Pages" }));
      await waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ hidden_columns: ["subtitle", "pages"] }]);
      });
      fetchSpy.mockRestore();
    });
  });

  describe("preference reset", () => {
    test.each(["mouse", "keyboard"])(
      "resetting the columns with %s closes, restores focus and preserves sibling preferences",
      async (activation) => {
        const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input) =>
          Promise.resolve(
            input === PREFERENCES_PATH
              ? preferencesResponse(
                  preferencesFixture({ density: "compact", view: "table", sort_stack: "title" }),
                )
              : new Response(JSON.stringify({ items: [], next_cursor: null }), {
                  headers: { "Content-Type": "application/json" },
                }),
          ),
        );
        renderLibrary({
          items: [bookFixture()],
          nextCursor: null,
          visit: { view: "table" },
          preferences: preferencesFixture({
            hidden_columns: ["subtitle"],
            density: "compact",
            view: "table",
            sort_stack: "title",
          }),
          extraCacheParams: [{ sort: "title" }],
        });
        await screen.findByTestId("library-table");
        expect(screen.queryByRole("columnheader", { name: /^Subtitle/ })).not.toBeInTheDocument();
        const user = userEvent.setup();
        const trigger = screen.getByRole("button", { name: /Columns/ });
        await user.click(trigger);
        const reset = await screen.findByRole("button", { name: "Reset to default" });
        if (activation === "mouse") {
          await user.click(reset);
        } else {
          reset.focus();
          await user.keyboard("{Enter}");
        }
        await waitFor(() =>
          expect(screen.queryByRole("dialog", { name: "Columns" })).not.toBeInTheDocument(),
        );
        await waitFor(() => expect(trigger).toHaveFocus());

        // The hidden group returns to the installation default...
        expect(await screen.findByRole("columnheader", { name: /^Subtitle/ })).toBeInTheDocument();
        // ...and only that group is named on the wire, so density survives.
        await waitFor(() => {
          expect(preferenceWrites(fetchSpy)).toEqual([{ hidden_columns: null }]);
        });
        const group = screen.getByRole("radiogroup", { name: "Table density" });
        expect(
          within(group).getByRole("radio", { name: "Compact", checked: true }),
        ).toBeInTheDocument();
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
        expect(screen.getByRole("columnheader", { name: /^Title/ })).toHaveAttribute(
          "aria-sort",
          "ascending",
        );
        await user.click(trigger);
        expect(await screen.findByRole("button", { name: "Reset to default" })).toBeDisabled();
        await user.keyboard("{Escape}");
        await waitFor(() =>
          expect(screen.queryByRole("dialog", { name: "Columns" })).not.toBeInTheDocument(),
        );
        await waitFor(() => expect(trigger).toHaveFocus());
        fetchSpy.mockRestore();
      },
    );

    test("the reset is offered only while an override exists to drop", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: /Columns/ }));
      expect(await screen.findByRole("button", { name: "Reset to default" })).toBeDisabled();
    });

    test("an override whose value equals the default still offers the reset", async () => {
      // Hiding then unhiding a column stores `hidden_columns: []`, an
      // override that matches today's default. Dropping it changes nothing
      // visible now, but re-subscribes the reader to future changes of the
      // installation default, so the control must stay live.
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: preferencesFixture({ hidden_columns: [] }),
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: /Columns/ }));
      const reset = await screen.findByRole("button", { name: "Reset to default" });
      expect(reset).toBeEnabled();
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => Promise.resolve(preferencesResponse(preferencesFixture())));
      await user.click(reset);
      await waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ hidden_columns: null }]);
      });
      expect(screen.queryByRole("dialog", { name: "Columns" })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /Columns/ }));
      expect(await screen.findByRole("button", { name: "Reset to default" })).toBeDisabled();
      fetchSpy.mockRestore();
    });

    test("hiding then unhiding a column leaves the reset offered", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ hidden_columns: [] })));
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: /Columns/ }));
      await user.click(await screen.findByRole("checkbox", { name: "Subtitle" }));
      await user.click(screen.getByRole("checkbox", { name: "Subtitle" }));
      // The visible set matches the default again, but the override exists.
      expect(screen.getByRole("button", { name: "Reset to default" })).toBeEnabled();
      fetchSpy.mockRestore();
    });
  });

  describe("first paint and in-flight responses", () => {
    test("the mirror supplies columns and density before the response lands", async () => {
      rememberActiveUser("mirror-user");
      localStorage.setItem(
        displayStorageKey("mirror-user"),
        JSON.stringify({
          density: "compact",
          hiddenColumns: ["subtitle"],
          view: null,
          sortStack: null,
        }),
      );
      // A never-settling fetch pins the page in the pre-response window.
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => new Promise(() => undefined));
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: null,
      });
      await screen.findByTestId("library-table");
      expect(screen.queryByRole("columnheader", { name: /^Subtitle/ })).not.toBeInTheDocument();
      const group = screen.getByRole("radiogroup", { name: "Table density" });
      expect(
        within(group).getByRole("radio", { name: "Compact", checked: true }),
      ).toBeInTheDocument();
      fetchSpy.mockRestore();
    });

    test("a malformed mirror paints the defaults instead of throwing", async () => {
      rememberActiveUser("mirror-user");
      localStorage.setItem(displayStorageKey("mirror-user"), "{not json");
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await screen.findByTestId("library-table");
      expect(screen.getByRole("columnheader", { name: /^Subtitle/ })).toBeInTheDocument();
      const group = screen.getByRole("radiogroup", { name: "Table density" });
      expect(
        within(group).getByRole("radio", { name: "Comfortable", checked: true }),
      ).toBeInTheDocument();
    });

    test("a response landing after a change does not overwrite what the reader chose", async () => {
      // Declared without an initialiser so it stays callable: initialising it
      // to null would narrow the type to null at the call site below, since
      // the only assignment happens inside a callback.
      let resolveRead: ((value: Response) => void) | undefined;
      const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
        if (input === PREFERENCES_PATH && init?.method !== "PATCH") {
          return new Promise<Response>((resolve) => {
            resolveRead = resolve;
          });
        }
        return Promise.resolve(preferencesResponse(preferencesFixture()));
      });
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: null,
      });
      await screen.findByTestId("library-table");
      const user = userEvent.setup();
      const group = screen.getByRole("radiogroup", { name: "Table density" });
      await user.click(within(group).getByRole("radio", { name: "Compact" }));

      // The read was already in flight when the reader chose; it answers with
      // the pre-change state, which must not win.
      await waitFor(() => {
        expect(resolveRead).toBeDefined();
      });
      resolveRead?.(preferencesResponse(preferencesFixture({ density: "comfortable" })));

      await waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ density: "compact" }]);
      });
      expect(
        within(group).getByRole("radio", { name: "Compact", checked: true }),
      ).toBeInTheDocument();
      fetchSpy.mockRestore();
    });
  });

  describe("per-user sort", () => {
    test("the stored sort applies on a bare URL", async () => {
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        preferences: preferencesFixture({ sort_stack: "-pages" }),
        cacheParams: { sort: "-pages" },
      });
      await screen.findByRole("heading", { name: "Library" });
      expect(screen.getByText("Sorted by Pages descending")).toBeInTheDocument();
    });

    test("a stale URL ?sort= is inert: the stored sort still applies", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ sort_stack: "-pages" })));
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: {},
        preferences: preferencesFixture({ sort_stack: "-pages" }),
        cacheParams: { sort: "-pages" },
      });
      await screen.findByRole("heading", { name: "Library" });
      expect(screen.getByText("Sorted by Pages descending")).toBeInTheDocument();
      // The dead param also never redefines the stored sort.
      await new Promise((resolve) => setTimeout(resolve, 600));
      expect(preferenceWrites(fetchSpy)).toEqual([]);
      fetchSpy.mockRestore();
    });

    test("an inheriting reader sees the installation order announced truthfully", async () => {
      renderLibrary({ items: [bookFixture()], nextCursor: null, cacheParams: {} });
      await screen.findByRole("heading", { name: "Library" });
      // The library is never unsorted: with no override, the effective sort
      // is the installation default the response carries.
      expect(screen.getByText("Sorted by Added descending")).toBeInTheDocument();
    });

    test("a sort chosen from the table header becomes the stored sort", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ sort_stack: "title" })));
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        extraCacheParams: [{ sort: "title" }],
      });
      await findLibraryTable();
      const user = userEvent.setup();
      await user.click(await screen.findByRole("columnheader", { name: "Title" }));
      await waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
      });
      fetchSpy.mockRestore();
    });

    test("a header press on a descending sort wraps to ascending, never to unsorted", async () => {
      // The regression the two-layer model shipped: with a descending stored
      // sort and a bare URL, the clearing press did nothing. Under two-state
      // headers the same press means ascending, and it must reach the wire.
      // Title rather than Pages: the grid virtualises columns horizontally,
      // and Pages sits outside the test viewport's rendered window.
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ sort_stack: "title" })));
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: preferencesFixture({ sort_stack: "-title" }),
        cacheParams: { sort: "-title" },
        extraCacheParams: [{ sort: "title" }],
      });
      await findLibraryTable();
      const user = userEvent.setup();
      await user.click(await screen.findByRole("columnheader", { name: "Title" }));
      await waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
      });
      expect(screen.getByText("Sorted by Title ascending")).toBeInTheDocument();
      fetchSpy.mockRestore();
    });

    test("the rail's reset drops the override and returns to the installation order", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => Promise.resolve(preferencesResponse(preferencesFixture())));
      renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        preferences: preferencesFixture({ sort_stack: "-pages" }),
        cacheParams: { sort: "-pages" },
        extraCacheParams: [{}],
      });
      await screen.findByRole("heading", { name: "Library" });
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: /^Filters/ }));
      await screen.findByRole("dialog");
      await user.click(
        screen.getByRole("button", { name: "Reset sort to the installation order" }),
      );
      await waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: null }]);
      });
      // Visibly the installation order, not an unsorted state.
      expect(screen.getByText("Sorted by Added descending")).toBeInTheDocument();
      fetchSpy.mockRestore();
    });

    test("a late preferences response re-sorts the initial list once", async () => {
      // The loader can only seed from the mirror; a device without one seeds
      // the installation order. When the response reveals an override, the
      // component asks for the sorted key: the sequence must reach the wire
      // as one list request for the sorted slot, not a discarded duplicate
      // of the seeded one.
      let resolveRead: ((value: Response) => void) | undefined;
      const listCalls: string[] = [];
      const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
        const url =
          typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
        if (url.includes(PREFERENCES_PATH) && init?.method !== "PATCH") {
          return new Promise<Response>((resolve) => {
            resolveRead = resolve;
          });
        }
        if (url.includes("/api/v1/books")) {
          listCalls.push(url);
          return Promise.resolve(
            new Response(
              JSON.stringify({
                items: [bookFixture({ id: "sorted", title: "A Sorted Row" })],
                next_cursor: null,
              }),
              { status: 200, headers: { "Content-Type": "application/json" } },
            ),
          );
        }
        return Promise.reject(new Error(`unexpected fetch: ${url}`));
      });
      // Only the loader's unsorted seed exists, exactly as a first visit on
      // a new device would leave the cache.
      renderLibrary({
        items: [bookFixture({ title: "The Seeded Row" })],
        nextCursor: null,
        preferences: null,
        cacheParams: {},
      });
      expect(await screen.findByText("The Seeded Row")).toBeInTheDocument();
      await waitFor(() => {
        expect(resolveRead).toBeDefined();
      });
      resolveRead?.(preferencesResponse(preferencesFixture({ sort_stack: "-pages" })));
      // The corrected key fetches once, and its rows replace the seed.
      expect(await screen.findByText("A Sorted Row")).toBeInTheDocument();
      expect(listCalls).toHaveLength(1);
      expect(listCalls[0]).toContain("sort=-pages");
      fetchSpy.mockRestore();
    });
  });
});

function deferredResponse(): {
  promise: Promise<Response>;
  resolve: (response: Response) => void;
  reject: (error: Error) => void;
} {
  let resolve: (response: Response) => void = () => {
    throw new Error("promise not initialised");
  };
  let reject: (error: Error) => void = () => {
    throw new Error("promise not initialised");
  };
  const promise = new Promise<Response>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function booksResponse(items: BookListItem[], next_cursor: string | null = null): Response {
  return new Response(JSON.stringify({ items, next_cursor }), {
    headers: { "Content-Type": "application/json" },
  });
}

test("placeholder rows keep their source key, controls and edit capability while current intent clears selection", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  const origin = bookFixture({ title: "Origin", id: "origin" });
  const destination = bookFixture({
    title: "Destination",
    id: "destination",
    work_id: "different",
  });
  const { client } = renderLibrary({
    items: [origin],
    nextCursor: "old-page",
    visit: { view: "table" },
    me: meFixture(),
  });
  await findLibraryTable();
  const user = userEvent.setup();
  await user.click(screen.getByRole("checkbox", { name: "Select all loaded books" }));
  fireEvent.change(searchBox(), { target: { value: "destination" } });
  expect(await screen.findByText("Updating results…")).toBeInTheDocument();
  expect(observedListKeys.at(-1)).toEqual(queryKeys.books.list({}));
  expect(screen.getByText("Origin")).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Select all loaded books" })).not.toBeChecked();
  expect(screen.queryByRole("button", { name: /Load more/i })).not.toBeInTheDocument();
  expect(client.getQueryData(queryKeys.books.list({ q: "destination" }))).toBeUndefined();
  pending.resolve(booksResponse([destination]));
  await screen.findByText("Destination");
  expect(observedListKeys.at(-1)).toEqual(queryKeys.books.list({ q: "destination" }));
  expect(
    client.getQueryData<InfiniteData<BookListResponse>>(queryKeys.books.list({}))?.pages[0].items,
  ).toEqual([origin]);
  expect(
    client.getQueryData<InfiniteData<BookListResponse>>(queryKeys.books.list({ q: "destination" }))
      ?.pages[0].items,
  ).toEqual([destination]);
  fetchSpy.mockRestore();
});

test("a late old page cannot append to a new filter result", async () => {
  const oldPage = deferredResponse();
  const origin = bookFixture({ title: "Origin" });
  const destination = bookFixture({ title: "Destination", id: "destination" });
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const params = new URL(
      typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
      window.location.origin,
    ).searchParams;
    return params.has("cursor") ? oldPage.promise : Promise.resolve(booksResponse([destination]));
  });
  const { client } = renderLibrary({ items: [origin], nextCursor: "old-page" });
  await userEvent.setup().click(await screen.findByRole("button", { name: "Load more" }));
  fireEvent.change(searchBox(), { target: { value: "destination" } });
  await screen.findByText("Destination");
  oldPage.resolve(booksResponse([bookFixture({ title: "Late Old Page", id: "late" })]));
  await act(async () => {
    await oldPage.promise;
  });
  expect(screen.queryByText("Late Old Page")).not.toBeInTheDocument();
  expect(
    client.getQueryData<InfiniteData<BookListResponse>>(queryKeys.books.list({ q: "destination" }))
      ?.pages,
  ).toHaveLength(1);
  fetchSpy.mockRestore();
});

test("a successful empty response replaces placeholders", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  const { client } = renderLibrary({ items: [bookFixture({ title: "Origin" })], nextCursor: null });
  fireEvent.change(await screen.findByRole("searchbox"), { target: { value: "missing" } });
  expect(screen.getByText("Origin")).toBeInTheDocument();
  pending.resolve(booksResponse([]));
  expect(await screen.findByText("No books match these filters")).toBeInTheDocument();
  expect(screen.queryByText("Origin")).not.toBeInTheDocument();
  expect(
    client.getQueryData<InfiniteData<BookListResponse>>(queryKeys.books.list({ q: "missing" }))
      ?.pages[0].items,
  ).toEqual([]);
  fetchSpy.mockRestore();
});

test("an initial uncached load uses the skeleton, then mounts the controls with its result", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  renderLibrary({ items: [], nextCursor: null, uncached: true });
  expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  expect(document.querySelector('[aria-busy="true"]')).not.toBeNull();
  pending.resolve(booksResponse([bookFixture({ title: "Initial Result" })]));
  expect(await screen.findByText("Initial Result")).toBeInTheDocument();
  expect(searchBox()).toBeInTheDocument();
  fetchSpy.mockRestore();
});

test("a changed-query failure reaches the error boundary without manufacturing an empty result", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
  renderLibrary({ items: [bookFixture()], nextCursor: null });
  fireEvent.change(await screen.findByRole("searchbox"), { target: { value: "different" } });
  pending.reject(new Error("Query unavailable"));
  expect(await screen.findByText("Query unavailable")).toBeInTheDocument();
  expect(screen.queryByText("No books match these filters")).not.toBeInTheDocument();
  fetchSpy.mockRestore();
  errorSpy.mockRestore();
});

test("a superseded filter request is aborted and cannot overwrite the current rows", async () => {
  const obsolete = deferredResponse();
  let obsoleteSignal: AbortSignal | undefined;
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const url = new URL(
      typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
      window.location.origin,
    );
    if (url.searchParams.get("q") === "obsolete") {
      obsoleteSignal = init?.signal ?? undefined;
      return obsolete.promise;
    }
    return Promise.resolve(
      booksResponse([bookFixture({ title: "Current Result", id: "current" })]),
    );
  });
  renderLibrary({ items: [bookFixture({ title: "Origin" })], nextCursor: null });
  fireEvent.change(await screen.findByRole("searchbox"), { target: { value: "obsolete" } });
  await waitFor(() => {
    expect(obsoleteSignal).toBeDefined();
  });
  fireEvent.change(searchBox(), { target: { value: "current" } });
  await screen.findByText("Current Result");
  expect(obsoleteSignal?.aborted).toBe(true);
  obsolete.resolve(booksResponse([bookFixture({ title: "Obsolete Result", id: "obsolete" })]));
  await act(async () => {
    await obsolete.promise;
  });
  expect(screen.queryByText("Obsolete Result")).not.toBeInTheDocument();
  expect(screen.getByText("Current Result")).toBeInTheDocument();
  fetchSpy.mockRestore();
});
