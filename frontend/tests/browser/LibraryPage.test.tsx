import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { QueryClient, QueryClientProvider, type InfiniteData } from "@tanstack/react-query";
import { cleanup, render, renderHook } from "vitest-browser-react";
import { page, userEvent, type Locator } from "vite-plus/test/browser";
import { act } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import {
  emptyFilterState,
  filterStateToParams,
  LibraryVisitSchema,
  type FilterState,
  type LibraryVisit,
} from "@/routes/library-params";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { useSyncExternalStore, type MouseEvent, type ReactElement } from "react";
import { toast } from "sonner";

import type {
  BookDetail,
  BookListItem,
  BookListResponse,
  ListBooksParams,
  Preferences,
} from "@/api";
import type { AuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import type {
  useCellEdit as useCellEditType,
  UseCellEditOptions,
} from "@/pages/library/table/useCellEdit";

import { CommandPalette } from "@/components/CommandPalette";

import { rememberActiveUser } from "@/lib/active-user";

import { displayStorageKey } from "@/pages/library/display-storage";
import { LibraryPage } from "@/pages/library/LibraryPage";
import { useLibraryPreferences } from "@/pages/library/use-library-preferences";

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const { observedListKeys } = vi.hoisted(() => ({ observedListKeys: [] as unknown[] }));
vi.mock("@/pages/library/table/useCellEdit", async (importOriginal) => {
  const original = await importOriginal<{ useCellEdit: typeof useCellEditType }>();
  return {
    ...original,
    useCellEdit: (options: UseCellEditOptions) => {
      observedListKeys.push(options.listKey);
      return original.useCellEdit(options);
    },
  };
});

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ preference: "system", effective: "dark", setPreference: vi.fn() }),
}));

const originalEntryState: unknown = navigation.currentEntry?.getState();
const originalURL = window.location.href;
beforeEach(() => {
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
});
afterEach(async () => {
  localStorage.clear();

  await cleanup();
  history.replaceState(originalEntryState ?? null, "", originalURL);
  vi.mocked(toast.success).mockClear();
  vi.mocked(toast.error).mockClear();
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
    cover_url: "/brand/raster/favicon-48.png",
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

type RenderOpts = {
  items: BookListItem[];
  nextCursor: string | null;
  visit?: { filters?: Partial<FilterState>; view?: LibraryVisit["view"] };

  preferences?: Preferences | null;

  cacheParams?: ListBooksParams;

  extraCacheParams?: ListBooksParams[];

  me?: AuthMe;

  details?: BookDetail[];
  withPalette?: boolean;
  uncached?: boolean;
};

function findLibraryTable(): Locator {
  return page.getByTestId("library-table");
}

async function renderLibrary({
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
}: RenderOpts): Promise<{
  client: QueryClient;
}> {
  const client = new QueryClient({
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

  history.replaceState(navigation.currentEntry?.getState() ?? null, "", "/library");
  navigation.updateCurrentEntry({
    state: {
      reverieLibrary: { filters: { ...emptyFilterState(), ...visit?.filters }, view: visit?.view },
    },
  });
  function LocationProbe(): ReactElement {
    const state = useSyncExternalStore(
      (onChange) => {
        navigation.addEventListener("currententrychange", onChange);
        return () => {
          navigation.removeEventListener("currententrychange", onChange);
        };
      },
      () => JSON.stringify(navigation.currentEntry?.getState()),
    );
    const raw: unknown = JSON.parse(state);
    if (typeof raw !== "object" || raw === null || !("reverieLibrary" in raw))
      throw new Error("Missing native visit state");
    const restored = LibraryVisitSchema.parse(raw.reverieLibrary);
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(filterStateToParams(restored.filters))) {
      if (Array.isArray(value)) for (const item of value) params.append(key, item);
      else params.set(key, String(value));
    }
    if (restored.view !== undefined) params.set("view", restored.view);
    return <div data-testid="visit-params">{params.toString()}</div>;
  }

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
    const router = createMemoryRouter(routes, { initialEntries: ["/library"] });
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }

  await render(<Wrapper />);
  return { client };
}

function searchBox(): Locator {
  return page.getByRole("searchbox", { name: "Search your library" });
}

async function drawerSection(drawer: Locator, title: string): Promise<HTMLElement> {
  const section = drawer
    .getByRole("button", { name: new RegExp(`^${title}(?:\\s*\\d+)?$`) })
    .elements()
    .map((node) => node.closest("section"))
    .find((candidate): candidate is HTMLElement => candidate !== null);
  if (section === undefined) throw new Error(`no drawer section titled ${title}`);
  const disclosure = section.querySelector("button[slot='trigger'][aria-expanded='false']");
  if (disclosure !== null) await userEvent.click(disclosure);
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
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        me: meFixture(),
        withPalette: true,
      });
      await expect.element(findLibraryTable()).toBeInTheDocument();
      const user = userEvent;
      await user.keyboard("{Control>}k{/Control}");
      const palette = page.getByRole("dialog", { name: "Search library" });
      const input = palette.getByRole("searchbox");
      await user.type(input, query);
      await expect.element(input).toHaveValue(query);
      await expect.element(document.documentElement).not.toHaveAttribute("data-cinematic");
      await expect
        .element(page.getByRole("dialog", { name: /Keyboard shortcuts/i }))
        .not.toBeInTheDocument();
      fetchSpy.mockRestore();
    },
  );

  test("renders the heading without a fabricated total", async () => {
    await renderLibrary({ items: [bookFixture()], nextCursor: null });
    await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();

    await expect.element(page.getByText(/1 book/)).not.toBeInTheDocument();
  });

  test("the title renders inside the hero band over the atmosphere layers", async () => {
    await renderLibrary({ items: [bookFixture()], nextCursor: null });
    const heading = page.getByRole("heading", { name: "Library" });
    expect(heading.element().closest(".lib-hero")).not.toBeNull();

    expect(document.querySelector(".lib-atm")).not.toBeNull();
    expect(document.querySelector(".lib-grain")).not.toBeNull();
  });

  test("cinema-hint is always in the DOM so its CSS fade can play (not gated on state)", async () => {
    await renderLibrary({ items: [bookFixture()], nextCursor: null });
    await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
    expect(document.querySelector(".cinema-hint")).not.toBeNull();
  });

  test("the toolbar hosts no result totals or sort menu", async () => {
    await renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      preferences: preferencesFixture({ sort_stack: "title" }),
      cacheParams: { sort: "title" },
    });
    await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: /^Sort:/ })).not.toBeInTheDocument();
    await expect.element(page.getByText(/All records/)).not.toBeInTheDocument();
    await expect.element(page.getByText(/Catalogue/)).not.toBeInTheDocument();
  });

  test("grid tiles carry a focus treatment equal to the hover treatment", async () => {
    await renderLibrary({ items: [bookFixture({ id: "abc", title: "Stoner" })], nextCursor: null });
    const link = page.getByRole("link", { name: /stoner/i });
    expect(link.element().getAttribute("class")).toMatch(/hover:/);
    expect(link.element().getAttribute("class")).toMatch(/focus-visible:/);
  });

  test("missing cover art falls back to the cloth spine", async () => {
    await renderLibrary({
      items: [bookFixture({ id: "no-cover", title: "Spineless", cover_url: "" })],
      nextCursor: null,
    });
    await expect.element(page.getByTestId("library-grid")).toBeInTheDocument();
    expect(document.querySelector("[data-cloth]")).not.toBeNull();
  });

  test("cover image load error swaps in the cloth spine", async () => {
    await renderLibrary({
      items: [bookFixture({ id: "broken-cover", title: "Broken" })],
      nextCursor: null,
    });
    const grid = page.getByTestId("library-grid");
    const img = grid.getByRole("img", { name: /Cover of/ });
    expect(document.querySelector("[data-cloth]")).toBeNull();
    img.element().setAttribute("src", "/nonexistent-library-cover.png");
    await vi.waitFor(() => {
      expect(document.querySelector("[data-cloth]")).not.toBeNull();
    });
  });

  test("the filter drawer lists distinct series from the loaded pages", async () => {
    await renderLibrary({
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
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /^Filters/ }));
    const drawer = page.getByRole("dialog");
    await drawerSection(drawer, "Series");
    expect(drawer.getByRole("checkbox", { name: "Discworld" }).elements()).toHaveLength(1);
  });

  test("renders one card per item in the grid by default", async () => {
    await renderLibrary({
      items: [
        bookFixture({ id: "a", title: "Stoner" }),
        bookFixture({ id: "b", title: "Piranesi" }),
        bookFixture({ id: "c", title: "Annihilation" }),
      ],
      nextCursor: null,
    });
    const grid = page.getByTestId("library-grid");
    expect(grid.getByRole("listitem").elements()).toHaveLength(3);
    await expect.element(grid.getByText("Stoner")).toBeInTheDocument();
    await expect.element(grid.getByText("Piranesi")).toBeInTheDocument();
    await expect.element(grid.getByText("Annihilation")).toBeInTheDocument();
  });

  test("does NOT show Load more when next_cursor is null", async () => {
    await renderLibrary({ items: [bookFixture()], nextCursor: null });
    await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: /load more/i })).not.toBeInTheDocument();
  });

  test("shows Load more when next_cursor is set", async () => {
    await renderLibrary({ items: [bookFixture()], nextCursor: "eyJ4Ijox" });
    await expect.element(page.getByRole("button", { name: /load more/i })).toBeInTheDocument();
  });

  test("view toggle group renders grid and table only; no list mode survives", async () => {
    await renderLibrary({ items: [bookFixture()], nextCursor: null });
    const group = page.getByRole("radiogroup", { name: "View mode" });
    await expect
      .element(group.getByRole("radio", { name: "Grid", checked: true }))
      .toBeInTheDocument();
    await expect
      .element(group.getByRole("radio", { name: "Table", checked: false }))
      .toBeInTheDocument();
    await expect.element(group.getByRole("radio", { name: "List" })).not.toBeInTheDocument();
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
    await renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      preferences: preferencesFixture(unrelated),
      cacheParams: { sort: "title" },
    });
    const user = userEvent;
    const view = page.getByRole("radiogroup", { name: "View mode" });
    await user.click(view.getByRole("radio", { name: "Grid" }));
    await user.keyboard("{ArrowRight}");
    await expect.element(view.getByRole("radio", { name: "Table" })).toHaveFocus();
    await user.keyboard(" ");
    await expect.element(view.getByRole("radio", { name: "Table" })).toBeChecked();
    await expect.element(view.getByRole("radio", { name: "Grid" })).not.toBeChecked();
    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("visit-params")).toMatchTextContent("view=table");
    });
    await user.keyboard(" ");
    await expect.element(view.getByRole("radio", { name: "Table" })).toBeChecked();
    const density = page.getByRole("radiogroup", { name: "Table density" });
    await user.click(density.getByRole("radio", { name: "Comfortable" }));
    await user.keyboard("{ArrowRight}");
    await expect.element(density.getByRole("radio", { name: "Compact" })).toHaveFocus();
    await user.keyboard(" ");
    await expect.element(density.getByRole("radio", { name: "Compact" })).toBeChecked();
    await expect.element(density.getByRole("radio", { name: "Comfortable" })).not.toBeChecked();
    await user.keyboard(" ");
    await expect.element(density.getByRole("radio", { name: "Compact" })).toBeChecked();
    await vi.waitFor(() => {
      expect(Object.assign({}, unrelated, ...preferenceWrites(fetchSpy))).toEqual({
        ...unrelated,
        view: "table",
        density: "compact",
      });
    });
    fetchSpy.mockRestore();
  });

  test("renders the table view for a visit override (lazy-loaded)", async () => {
    await renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      visit: { view: "table" },
    });
    await expect.element(findLibraryTable()).toBeInTheDocument();
  });

  test("the stored view preference mounts the table when the visit override is absent", async () => {
    await renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      preferences: preferencesFixture({ view: "table" }),
    });
    await expect.element(findLibraryTable()).toBeInTheDocument();
  });

  test("clicking a view toggle persists the choice to the account", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(preferencesResponse(preferencesFixture({ view: "table" })));
    await renderLibrary({ items: [bookFixture()], nextCursor: null });
    const group = page.getByRole("radiogroup", { name: "View mode" });
    const user = userEvent;
    await user.click(group.getByRole("radio", { name: "Table" }));
    await vi.waitFor(() => {
      expect(preferenceWrites(fetchSpy)).toEqual([{ view: "table" }]);
    });
    fetchSpy.mockRestore();
  });

  test("Visit view beats the stored view preference", async () => {
    await renderLibrary({
      items: [bookFixture({ title: "Stoner" })],
      nextCursor: null,
      visit: { view: "grid" },
      preferences: preferencesFixture({ view: "table" }),
    });
    await expect.element(page.getByTestId("library-grid")).toBeInTheDocument();
    await expect.element(page.getByTestId("library-table")).not.toBeInTheDocument();
  });

  test("table header click persists the sort to the account and never touches the URL", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(preferencesResponse(preferencesFixture({ sort_stack: "title" })));
    await renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      visit: { view: "table" },
      extraCacheParams: [{ sort: "title" }],
    });
    await expect.element(findLibraryTable()).toBeInTheDocument();
    const user = userEvent;
    await user.click(page.getByRole("columnheader", { name: /^Title(?:\s|$)/ }));
    await vi.waitFor(() => {
      expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
    });
    expect(page.getByTestId("visit-params").element().textContent).not.toContain("sort=");
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
    await renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      visit: { view: "table" },
      extraCacheParams: [{ q: "war" }, { sort: "title" }, { q: "war", sort: "title" }],
    });
    await expect.element(findLibraryTable()).toBeInTheDocument();
    const user = userEvent;
    await user.type(searchBox(), "war");
    await user.click(page.getByRole("columnheader", { name: /^Title(?:\s|$)/ }));
    await vi.waitFor(() => {
      expect(page.getByTestId("visit-params").element().textContent).toContain("q=war");
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
    await renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      visit: { view: "table" },
      extraCacheParams: [{ q: "war" }, { sort: "title" }, { q: "war", sort: "title" }],
    });
    await expect.element(findLibraryTable()).toBeInTheDocument();
    const user = userEvent;
    await user.click(page.getByRole("columnheader", { name: /^Title(?:\s|$)/ }));
    await user.type(searchBox(), "war");
    await vi.waitFor(() => {
      expect(page.getByTestId("visit-params").element().textContent).toContain("q=war");
      expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
    });
    fetchSpy.mockRestore();
  });

  test("a typed filter with no matches shows the filtered-empty state", async () => {
    await renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { status: { any: ["unread"], none: [] } } },
      cacheParams: { status_any: ["unread"] },
      extraCacheParams: [{}],
    });
    await expect.element(page.getByText("No books match these filters")).toBeInTheDocument();
  });

  test("clear-all from the filtered-empty state drops typed filter params", async () => {
    await renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { status: { any: ["unread"], none: [] }, pages: { gte: 500 } } },
      cacheParams: { status_any: ["unread"], pages_gte: 500 },
      extraCacheParams: [{}],
    });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /clear all filters/i }));
    await vi.waitFor(() => {
      const search = page.getByTestId("visit-params").element().textContent;
      expect(search).not.toContain("status_any");
      expect(search).not.toContain("pages_gte");
    });
  });

  test("a well-formed filter lights the badge with the sent-param count", async () => {
    await renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { pages: { gte: 500 } } },
      cacheParams: { pages_gte: 500 },
      extraCacheParams: [{}],
    });
    await expect.element(page.getByText("No books match these filters")).toBeInTheDocument();

    await expect.element(page.getByRole("button", { name: "Filters 1" })).toBeInTheDocument();
  });

  test("renders the empty state when items is empty", async () => {
    await renderLibrary({ items: [], nextCursor: null });
    await expect.element(page.getByText("No books yet")).toBeInTheDocument();
    await expect.element(page.getByTestId("library-grid")).not.toBeInTheDocument();
  });

  test("links each book card to /b/{id}", async () => {
    await renderLibrary({
      items: [bookFixture({ id: "abc-123", title: "Stoner" })],
      nextCursor: null,
    });
    const link = page.getByRole("link", { name: /stoner/i });
    await expect.element(link).toHaveAttribute("href", "/b/abc-123");
  });

  test("true-empty state shows no clear-filters action", async () => {
    await renderLibrary({ items: [], nextCursor: null });
    await expect.element(page.getByText("No books yet")).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: /clear all filters/i }))
      .not.toBeInTheDocument();
  });

  test("true-empty offers an ingestion link to admins", async () => {
    await renderLibrary({ items: [], nextCursor: null, me: meFixture({ role: "admin" }) });
    const link = page.getByRole("link", { name: /ingestion/i });
    await expect.element(link).toHaveAttribute("href", "/admin/dashboard");
  });

  test("true-empty hides the ingestion link from non-admin readers", async () => {
    await renderLibrary({
      items: [],
      nextCursor: null,
      me: meFixture({ role: "child", is_child: true }),
    });
    await expect.element(page.getByText("No books yet")).toBeInTheDocument();
    await expect.element(page.getByRole("link", { name: /ingestion/i })).not.toBeInTheDocument();
  });

  test("filtered-empty shows its own copy and a clear-all action, not the true-empty copy", async () => {
    await renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" } },
      cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" },
      extraCacheParams: [{}],
    });
    await expect.element(page.getByText("No books match these filters")).toBeInTheDocument();
    await expect.element(page.getByText("No books yet")).not.toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: /clear all filters/i }))
      .toBeInTheDocument();
  });

  test("clearing all filters from the filtered-empty state drops the filter params", async () => {
    await renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" } },
      cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" },
      extraCacheParams: [{}],
    });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /clear all filters/i }));

    await expect.element(page.getByText("No books yet")).toBeInTheDocument();
    await expect.element(page.getByText("No books match these filters")).not.toBeInTheDocument();
  });

  test("the sort live region announces the stack in every state", async () => {
    await renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      preferences: preferencesFixture({ sort_stack: "-pages" }),
      cacheParams: { sort: "-pages" },
    });
    await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
    const region = page.getByText("Sorted by Pages descending");
    await expect.element(region).toHaveAttribute("aria-live", "polite");
  });

  test("a failed Load more surfaces an inline error and a Retry control", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network down"));
    await renderLibrary({ items: [bookFixture()], nextCursor: "eyJ4Ijox" });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /load more/i }));
    await expect.element(page.getByRole("alert")).toMatchTextContent(/couldn't load more/i);
    await expect.element(page.getByRole("button", { name: /retry/i })).toBeInTheDocument();
    fetchSpy.mockRestore();
  });

  test("Retry after a failed Load more re-fetches, recovers, and keeps the loaded pages", async () => {
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
    await renderLibrary({
      items: [bookFixture({ title: "The Brothers Karamazov" })],
      nextCursor: "eyJ4Ijox",
    });
    const user = userEvent;

    await user.click(page.getByRole("button", { name: /load more/i }));
    await expect.element(page.getByRole("alert")).toBeInTheDocument();

    await expect.element(page.getByText("The Brothers Karamazov")).toBeInTheDocument();

    await user.click(page.getByRole("button", { name: /retry/i }));
    await expect.element(page.getByText("Crime and Punishment")).toBeInTheDocument();
    await expect.element(page.getByText("The Brothers Karamazov")).toBeInTheDocument();
    await expect.element(page.getByRole("alert")).not.toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: /retry/i })).not.toBeInTheDocument();

    fetchSpy.mockRestore();
    errorSpy.mockRestore();
  });

  test("clear-all discards a toolbar search; the cleared filter cannot resurrect", async () => {
    await renderLibrary({
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
    const user = userEvent;
    await expect.element(page.getByText("No books match these filters")).toBeInTheDocument();
    await user.type(searchBox(), "du");
    await user.click(page.getByRole("button", { name: /clear all filters/i }));
    const search = page.getByTestId("visit-params").element().textContent;
    expect(search).not.toContain("q=");
    expect(search).not.toContain("series=");
    await expect.element(searchBox()).toHaveValue("");
  });

  test("clearing all filters preserves the visit view and the stored sort", async () => {
    await renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" }, view: "table" },
      preferences: preferencesFixture({ sort_stack: "title" }),
      cacheParams: { series: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", sort: "title" },
      extraCacheParams: [{ sort: "title" }],
    });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /clear all filters/i }));
    await vi.waitFor(() => {
      const search = page.getByTestId("visit-params").element().textContent;
      expect(search).toContain("view=table");
      expect(search).not.toContain("series=");
    });

    await expect.element(page.getByText("Sorted by Title ascending")).toBeInTheDocument();
    await expect
      .element(page.getByRole("radio", { name: "Table", checked: true }))
      .toBeInTheDocument();
  });

  describe("filter chips", () => {
    test("pointer removal retains adjacent focus and final removal focuses the empty collection", async () => {
      await renderLibrary({
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
      const user = userEvent;
      const chips = page.getByTestId("filter-chips");
      await user.click(chips.getByRole("button", { name: /Remove.*Series: Discworld/ }));
      await vi.waitFor(async () => {
        await expect.element(chips.getByRole("row")).toHaveFocus();
      });
      await user.click(chips.getByRole("button", { name: /Remove/ }));
      await vi.waitFor(async () => {
        await expect.element(chips.getByRole("group", { name: "Active filters" })).toHaveFocus();
      });
      await expect.element(chips.getByText("No active filters.")).toBeInTheDocument();
      await expect.element(chips.getByRole("button", { name: "Clear all" })).toBeDisabled();
    });

    test.each(["{Delete}", "{Backspace}"])(
      "%s removes the focused tag and preserves its neighbour",
      async (key) => {
        await renderLibrary({
          items: [bookFixture()],
          nextCursor: null,
          visit: { filters: { status: { any: ["unread"], none: [] }, pages: { gte: 500 } } },
          cacheParams: { status_any: ["unread"], pages_gte: 500 },
          extraCacheParams: [{ pages_gte: 500 }, {}],
        });
        const user = userEvent;
        const chips = page.getByTestId("filter-chips");
        const tags = chips.getByRole("row").elements();
        await user.click(tags[0]);
        await user.keyboard(key);
        await vi.waitFor(async () => {
          expect(chips.getByRole("row").elements()).toHaveLength(1);
          await expect.element(chips.getByRole("row")).toHaveFocus();
        });
        await user.keyboard(key);
        await vi.waitFor(async () => {
          await expect.element(chips.getByRole("group", { name: "Active filters" })).toHaveFocus();
        });
        await user.tab({ shift: true });
        await user.tab();
        await expect.element(chips.getByRole("group", { name: "Active filters" })).toHaveFocus();
      },
    );

    test("keeps the empty collection reachable when no filter params are set", async () => {
      await renderLibrary({ items: [bookFixture()], nextCursor: null });
      await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
      const chips = page.getByTestId("filter-chips");
      await expect.element(chips.getByText("No active filters.")).toBeInTheDocument();
      await expect
        .element(chips.getByRole("group", { name: "Active filters" }))
        .toHaveAttribute("tabindex", "0");
      await expect.element(chips.getByRole("button", { name: "Clear all" })).toBeDisabled();
    });

    test("an active series filter renders a chip with the resolved name, not a raw id", async () => {
      await renderLibrary({
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
      const chips = page.getByTestId("filter-chips");
      await expect.element(chips.getByText(/Series: Discworld/)).toBeInTheDocument();
      await expect.element(chips.getByText(/s-1/)).not.toBeInTheDocument();
    });

    test("a chip's remove control drops exactly that filter param", async () => {
      await renderLibrary({
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
      const chips = page.getByTestId("filter-chips");
      const user = userEvent;
      await user.click(chips.getByRole("button", { name: /Remove.*Series: Discworld/ }));
      await vi.waitFor(() => {
        const search = page.getByTestId("visit-params").element().textContent;
        expect(search).not.toContain("series=");
        expect(search).toContain("status_any=unread");
      });
    });

    test("the chip row's clear-all drops every filter param", async () => {
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { filters: { status: { any: ["unread"], none: [] }, pages: { gte: 500 } } },
        cacheParams: { status_any: ["unread"], pages_gte: 500 },
        extraCacheParams: [{}],
      });
      const chips = page.getByTestId("filter-chips");
      const user = userEvent;
      await user.click(chips.getByRole("button", { name: "Clear all" }));
      await vi.waitFor(() => {
        const search = page.getByTestId("visit-params").element().textContent;
        expect(search).not.toContain("status_any");
        expect(search).not.toContain("pages_gte");
      });
    });

    test("multi-token vocabulary filters render one family chip with a count", async () => {
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { filters: { tags: { all: ["scifi", "hugo"], any: [], none: [] } } },
        cacheParams: { tag: ["scifi", "hugo"] },
      });
      const chips = page.getByTestId("filter-chips");
      await expect.element(chips.getByText(/Tags \(2\)/)).toBeInTheDocument();
    });

    test("a chip falls back to a short id when the series is absent from the loaded pages", async () => {
      const longId = "aaaabbbb-cccc-4ddd-8eee-ffffffffffff";
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { filters: { series: longId } },
        cacheParams: { series: longId },
      });
      const chips = page.getByTestId("filter-chips");
      await expect.element(chips.getByText(/Series: aaaabbbb…/)).toBeInTheDocument();
      await expect.element(chips.getByText(/undefined/)).not.toBeInTheDocument();
    });
  });

  describe("overlays", () => {
    test("the Filters button opens the filter drawer; Escape closes it and returns focus", async () => {
      await renderLibrary({ items: [bookFixture()], nextCursor: null });
      const user = userEvent;
      const trigger = page.getByRole("button", { name: /^Filters/ });
      await expect.element(trigger).toHaveAttribute("aria-expanded", "false");
      await user.click(trigger);
      const drawer = page.getByRole("dialog");
      await expect.element(drawer.getByText("Refine")).toBeInTheDocument();
      await user.keyboard("{Escape}");
      await vi.waitFor(async () => {
        await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
      });
      await vi.waitFor(async () => {
        await expect.element(trigger).toHaveFocus();
      });
    });

    test("activating a table row opens the book-detail drawer with real record actions", async () => {
      const book = bookFixture({ title: "Stoner", id: "abc-123" });
      await renderLibrary({
        items: [book],
        nextCursor: null,
        visit: { view: "table" },
        details: [detailFixture(book)],
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: `Open details for ${book.title}` }));
      const drawer = page.getByRole("dialog");
      await expect.element(drawer.getByText("Farrar, Straus and Giroux")).toBeInTheDocument();
      const fullRecord = drawer.getByRole("link", { name: /View full record/ });
      await expect.element(fullRecord).toHaveAttribute("href", `/b/${book.id}`);
      await expect
        .element(drawer.getByRole("button", { name: /Add to shelf/ }))
        .toBeInTheDocument();

      await expect
        .element(drawer.getByRole("button", { name: /Copy title/ }))
        .not.toBeInTheDocument();
      await expect.element(drawer.getByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
      await user.keyboard("{Escape}");
      await vi.waitFor(async () => {
        await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
      });
    });

    test("detail drawer failure shows an alert with a working Retry", async () => {
      const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
      const book = bookFixture({ title: "Stoner", id: "abc-123" });
      const detail = detailFixture(book);

      let detailCalls = 0;
      const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
        let url: string;
        if (typeof input === "string") {
          url = input;
        } else if (input instanceof URL) {
          url = input.href;
        } else {
          url = input.url;
        }
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
      await renderLibrary({
        items: [book],
        nextCursor: null,
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: `Open details for ${book.title}` }));
      const drawer = page.getByRole("dialog");
      await expect.element(drawer.getByRole("alert")).toMatchTextContent(/couldn't load/i);
      await user.click(drawer.getByRole("button", { name: "Retry" }));
      await expect.element(drawer.getByText("Farrar, Straus and Giroux")).toBeInTheDocument();
      fetchSpy.mockRestore();
      errorSpy.mockRestore();
    });

    test("detail drawer shows a busy skeleton while the record loads", async () => {
      const book = bookFixture({ title: "Stoner", id: "abc-123" });

      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => new Promise(() => undefined));
      await renderLibrary({
        items: [book],
        nextCursor: null,
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: `Open details for ${book.title}` }));
      const drawer = page.getByRole("dialog");
      expect(drawer.element().querySelector('[aria-busy="true"]')).not.toBeNull();
      fetchSpy.mockRestore();
    });

    test("the overlay slot is exclusive: opening one drawer replaces the other", async () => {
      const book = bookFixture({ title: "Stoner", id: "abc-123" });
      await renderLibrary({
        items: [book],
        nextCursor: null,
        visit: { view: "table" },
        details: [detailFixture(book)],
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: `Open details for ${book.title}` }));
      await expect.element(page.getByRole("dialog")).toBeInTheDocument();
      await user.keyboard("{Escape}");
      await vi.waitFor(async () => {
        await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
      });
      await user.click(page.getByRole("button", { name: /^Filters/ }));
      const drawers = page.getByRole("dialog").elements();
      expect(drawers).toHaveLength(1);
      await expect.element(page.elementLocator(drawers[0]).getByText("Refine")).toBeInTheDocument();
    });
  });

  describe("selection and batch bar", () => {
    test("selecting rows raises the batch bar with count, clear, and no Remove action", async () => {
      await renderLibrary({
        items: [bookFixture({ id: "a", title: "Stoner" }), bookFixture({ id: "b" })],
        nextCursor: null,
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("checkbox", { name: "Select" }).elements()[0]);
      const bar = page.getByRole("toolbar", { name: "Batch actions" });
      await expect.element(bar.getByText("1 selected")).toBeInTheDocument();
      await expect.element(bar.getByRole("button", { name: /Add to shelf/ })).toBeInTheDocument();
      await expect.element(bar.getByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
      await user.click(bar.getByRole("button", { name: /Clear selection/ }));
      await expect
        .element(page.getByRole("toolbar", { name: "Batch actions" }))
        .not.toBeInTheDocument();
    });

    test("the batch bar discloses loaded-only coverage when more pages exist", async () => {
      await renderLibrary({
        items: [bookFixture({ id: "a" })],
        nextCursor: "eyJ4Ijox",
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("checkbox", { name: "Select all loaded books" }));
      const bar = page.getByRole("toolbar", { name: "Batch actions" });
      await expect.element(bar.getByText("Selection covers loaded books only")).toBeInTheDocument();
    });

    test("a query change clears the selection", async () => {
      await renderLibrary({
        items: [bookFixture({ id: "a" })],
        nextCursor: null,
        visit: { view: "table" },
        extraCacheParams: [{ q: "war" }],
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("checkbox", { name: "Select" }).elements()[0]);
      await expect
        .element(page.getByRole("toolbar", { name: "Batch actions" }))
        .toBeInTheDocument();
      await user.type(searchBox(), "war");
      await vi.waitFor(async () => {
        await expect
          .element(page.getByRole("toolbar", { name: "Batch actions" }))
          .not.toBeInTheDocument();
      });
    });

    test("switching projections clears the selection", async () => {
      await renderLibrary({
        items: [bookFixture({ id: "a" })],
        nextCursor: null,
        visit: { view: "table" },
        extraCacheParams: [{}],
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("checkbox", { name: "Select" }).elements()[0]);
      await expect
        .element(page.getByRole("toolbar", { name: "Batch actions" }))
        .toBeInTheDocument();
      const group = page.getByRole("radiogroup", { name: "View mode" });
      await user.click(group.getByRole("radio", { name: "Grid" }));
      await expect.element(page.getByTestId("library-grid")).toBeInTheDocument();
      await user.click(group.getByRole("radio", { name: "Table" }));
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      await expect
        .element(page.getByRole("toolbar", { name: "Batch actions" }))
        .not.toBeInTheDocument();
    });
  });

  describe("display preferences", () => {
    test("table-mode display controls are absent in grid mode", async () => {
      await renderLibrary({ items: [bookFixture()], nextCursor: null });
      await expect.element(page.getByTestId("library-grid")).toBeInTheDocument();
      await expect
        .element(page.getByRole("radiogroup", { name: "Table density" }))
        .not.toBeInTheDocument();
      await expect.element(page.getByRole("button", { name: /Columns/ })).not.toBeInTheDocument();
    });

    test("the density toggle switches and persists the preference to the account", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ density: "compact" })));
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const group = page.getByRole("radiogroup", { name: "Table density" });
      await expect
        .element(group.getByRole("radio", { name: "Comfortable", checked: true }))
        .toBeInTheDocument();
      const user = userEvent;
      await user.click(group.getByRole("radio", { name: "Compact" }));
      await expect
        .element(group.getByRole("radio", { name: "Compact", checked: true }))
        .toBeInTheDocument();
      await vi.waitFor(() => {
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
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      await expect
        .element(page.getByRole("columnheader", { name: /^Subtitle/ }))
        .toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: /Columns/ }));
      const title = page.getByRole("checkbox", { name: "Title" });
      await expect.element(title).toBeDisabled();
      await user.click(
        page.getByRole("dialog", { name: "Columns" }).getByText("Subtitle", { exact: true }),
      );
      await vi.waitFor(async () => {
        await expect
          .element(page.getByRole("columnheader", { name: /^Subtitle/ }))
          .not.toBeInTheDocument();
      });
      await vi.waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ hidden_columns: ["subtitle"] }]);
      });
      fetchSpy.mockRestore();
    });

    test("a stored hidden column seeds the table without it", async () => {
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: preferencesFixture({ hidden_columns: ["subtitle"] }),
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      await expect
        .element(page.getByRole("columnheader", { name: /^Subtitle/ }))
        .not.toBeInTheDocument();
      await expect.element(page.getByRole("columnheader", { name: /^Title/ })).toBeInTheDocument();
    });

    test("preference changes restart the debounce and send one patch with the latest fields", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => Promise.resolve(preferencesResponse(preferencesFixture())));
      const client = new QueryClient({
        defaultOptions: { queries: { retry: false, staleTime: Infinity } },
      });
      client.setQueryData(queryKeys.auth.preferences(), preferencesFixture());
      const {
        result,
        act: update,
        unmount,
      } = await renderHook(() => useLibraryPreferences(), {
        wrapper: ({ children }) => (
          <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
      });
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      try {
        await update(() => {
          result.current.setHiddenColumns(new Set(["subtitle"]));
        });
        await update(() => vi.advanceTimersByTimeAsync(200));
        await update(() => {
          result.current.setHiddenColumns(new Set(["subtitle", "pages"]));
          result.current.setDensity("compact");
        });
        await update(() => vi.advanceTimersByTimeAsync(399));
        expect(preferenceWrites(fetchSpy)).toEqual([]);

        await update(() => vi.advanceTimersByTimeAsync(1));
        const expected = [{ hidden_columns: ["subtitle", "pages"], density: "compact" }];
        expect(preferenceWrites(fetchSpy)).toEqual(expected);

        await update(() => vi.advanceTimersByTimeAsync(400));
        expect(preferenceWrites(fetchSpy)).toEqual(expected);
      } finally {
        await unmount();
        client.clear();
        vi.useRealTimers();
        fetchSpy.mockRestore();
      }
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
        await renderLibrary({
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
        await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
        await expect
          .element(page.getByRole("columnheader", { name: /^Subtitle/ }))
          .not.toBeInTheDocument();
        const user = userEvent;
        const trigger = page.getByRole("button", { name: /Columns/ });
        await user.click(trigger);
        const reset = page.getByRole("button", { name: "Reset to default" });
        if (activation === "mouse") {
          await user.click(reset);
        } else {
          reset.element().focus();
          await user.keyboard("{Enter}");
        }
        await expect.element(page.getByRole("dialog", { name: "Columns" })).not.toBeInTheDocument();
        await expect.element(trigger).toHaveFocus();

        await expect
          .element(page.getByRole("columnheader", { name: /^Subtitle/ }))
          .toBeInTheDocument();

        await vi.waitFor(() => {
          expect(preferenceWrites(fetchSpy)).toEqual([{ hidden_columns: null }]);
        });
        const group = page.getByRole("radiogroup", { name: "Table density" });
        await expect
          .element(group.getByRole("radio", { name: "Compact", checked: true }))
          .toBeInTheDocument();
        await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
        await expect
          .element(page.getByRole("columnheader", { name: /^Title/ }))
          .toHaveAttribute("aria-sort", "ascending");
        await user.click(trigger);
        await expect.element(page.getByRole("button", { name: "Reset to default" })).toBeDisabled();
        await user.keyboard("{Escape}");
        await expect.element(page.getByRole("dialog", { name: "Columns" })).not.toBeInTheDocument();
        await expect.element(trigger).toHaveFocus();
        fetchSpy.mockRestore();
      },
    );

    test("the reset is offered only while an override exists to drop", async () => {
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: /Columns/ }));
      await expect.element(page.getByRole("button", { name: "Reset to default" })).toBeDisabled();
    });

    test("an override whose value equals the default still offers the reset", async () => {
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: preferencesFixture({ hidden_columns: [] }),
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: /Columns/ }));
      const reset = page.getByRole("button", { name: "Reset to default" });
      await expect.element(reset).toBeEnabled();
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => Promise.resolve(preferencesResponse(preferencesFixture())));
      await user.click(reset);
      await vi.waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ hidden_columns: null }]);
      });
      await expect.element(page.getByRole("dialog", { name: "Columns" })).not.toBeInTheDocument();
      await user.click(page.getByRole("button", { name: /Columns/ }));
      await expect.element(page.getByRole("button", { name: "Reset to default" })).toBeDisabled();
      fetchSpy.mockRestore();
    });

    test("hiding then unhiding a column leaves the reset offered", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ hidden_columns: [] })));
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: /Columns/ }));
      await user.click(
        page.getByRole("dialog", { name: "Columns" }).getByText("Subtitle", { exact: true }),
      );
      await user.click(
        page.getByRole("dialog", { name: "Columns" }).getByText("Subtitle", { exact: true }),
      );

      await expect.element(page.getByRole("button", { name: "Reset to default" })).toBeEnabled();
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

      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => new Promise(() => undefined));
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: null,
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      await expect
        .element(page.getByRole("columnheader", { name: /^Subtitle/ }))
        .not.toBeInTheDocument();
      const group = page.getByRole("radiogroup", { name: "Table density" });
      await expect
        .element(group.getByRole("radio", { name: "Compact", checked: true }))
        .toBeInTheDocument();
      fetchSpy.mockRestore();
    });

    test("a malformed mirror paints the defaults instead of throwing", async () => {
      rememberActiveUser("mirror-user");
      localStorage.setItem(displayStorageKey("mirror-user"), "{not json");
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      await expect
        .element(page.getByRole("columnheader", { name: /^Subtitle/ }))
        .toBeInTheDocument();
      const group = page.getByRole("radiogroup", { name: "Table density" });
      await expect
        .element(group.getByRole("radio", { name: "Comfortable", checked: true }))
        .toBeInTheDocument();
    });

    test("a response landing after a change does not overwrite what the reader chose", async () => {
      let resolveRead: ((value: Response) => void) | undefined;
      const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
        if (input === PREFERENCES_PATH && init?.method !== "PATCH") {
          return new Promise<Response>((resolve) => {
            resolveRead = resolve;
          });
        }
        return Promise.resolve(preferencesResponse(preferencesFixture()));
      });
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: null,
      });
      await expect.element(page.getByTestId("library-table")).toBeInTheDocument();
      const user = userEvent;
      const group = page.getByRole("radiogroup", { name: "Table density" });
      await user.click(group.getByRole("radio", { name: "Compact" }));

      await vi.waitFor(() => {
        expect(resolveRead).toBeDefined();
      });
      resolveRead?.(preferencesResponse(preferencesFixture({ density: "comfortable" })));

      await vi.waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ density: "compact" }]);
      });
      await expect
        .element(group.getByRole("radio", { name: "Compact", checked: true }))
        .toBeInTheDocument();
      fetchSpy.mockRestore();
    });
  });

  describe("per-user sort", () => {
    test("the stored sort applies on a bare URL", async () => {
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        preferences: preferencesFixture({ sort_stack: "-pages" }),
        cacheParams: { sort: "-pages" },
      });
      await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
      await expect.element(page.getByText("Sorted by Pages descending")).toBeInTheDocument();
    });

    test("a stale URL ?sort= is inert: the stored sort still applies", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ sort_stack: "-pages" })));
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: {},
        preferences: preferencesFixture({ sort_stack: "-pages" }),
        cacheParams: { sort: "-pages" },
      });
      await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
      await expect.element(page.getByText("Sorted by Pages descending")).toBeInTheDocument();

      await new Promise((resolve) => setTimeout(resolve, 600));
      expect(preferenceWrites(fetchSpy)).toEqual([]);
      fetchSpy.mockRestore();
    });

    test("an inheriting reader sees the installation order announced truthfully", async () => {
      await renderLibrary({ items: [bookFixture()], nextCursor: null, cacheParams: {} });
      await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();

      await expect.element(page.getByText("Sorted by Added descending")).toBeInTheDocument();
    });

    test("a sort chosen from the table header becomes the stored sort", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ sort_stack: "title" })));
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        extraCacheParams: [{ sort: "title" }],
      });
      await expect.element(findLibraryTable()).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("columnheader", { name: /^Title(?:\s|$)/ }));
      await vi.waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
      });
      fetchSpy.mockRestore();
    });

    test("a header press on a descending sort wraps to ascending, never to unsorted", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValue(preferencesResponse(preferencesFixture({ sort_stack: "title" })));
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: { view: "table" },
        preferences: preferencesFixture({ sort_stack: "-title" }),
        cacheParams: { sort: "-title" },
        extraCacheParams: [{ sort: "title" }],
      });
      await expect.element(findLibraryTable()).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("columnheader", { name: /^Title(?:\s|$)/ }));
      await vi.waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: "title" }]);
      });
      await expect.element(page.getByText("Sorted by Title ascending")).toBeInTheDocument();
      fetchSpy.mockRestore();
    });

    test("the rail's reset drops the override and returns to the installation order", async () => {
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockImplementation(() => Promise.resolve(preferencesResponse(preferencesFixture())));
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        preferences: preferencesFixture({ sort_stack: "-pages" }),
        cacheParams: { sort: "-pages" },
        extraCacheParams: [{}],
      });
      await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: /^Filters/ }));
      await expect.element(page.getByRole("dialog")).toBeInTheDocument();
      await user.click(page.getByRole("button", { name: "Reset sort to the installation order" }));
      await vi.waitFor(() => {
        expect(preferenceWrites(fetchSpy)).toEqual([{ sort_stack: null }]);
      });

      await expect.element(page.getByText("Sorted by Added descending")).toBeInTheDocument();
      fetchSpy.mockRestore();
    });

    test("a late preferences response re-sorts the initial list once", async () => {
      let resolveRead: ((value: Response) => void) | undefined;
      const listCalls: string[] = [];
      const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
        let url: string;
        if (typeof input === "string") {
          url = input;
        } else if (input instanceof URL) {
          url = input.href;
        } else {
          url = input.url;
        }
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

      await renderLibrary({
        items: [bookFixture({ title: "The Seeded Row" })],
        nextCursor: null,
        preferences: null,
        cacheParams: {},
      });
      await expect.element(page.getByText("The Seeded Row")).toBeInTheDocument();
      await vi.waitFor(() => {
        expect(resolveRead).toBeDefined();
      });
      resolveRead?.(preferencesResponse(preferencesFixture({ sort_stack: "-pages" })));

      await expect.element(page.getByText("A Sorted Row")).toBeInTheDocument();
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

test("selection made on retained rows clears when a disjoint result replaces them", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  const origin = bookFixture({ title: "Alpha", id: "alpha" });
  const destination = bookFixture({
    title: "Beta",
    id: "beta",
    work_id: "different",
  });
  const { client } = await renderLibrary({
    items: [origin],
    nextCursor: "old-page",
    visit: { filters: { q: "alpha" }, view: "table" },
    cacheParams: { q: "alpha" },
    me: meFixture(),
  });
  await expect.element(findLibraryTable()).toBeInTheDocument();
  const user = userEvent;
  await user.click(page.getByRole("checkbox", { name: "Select all loaded books" }));
  await userEvent.fill(searchBox(), "beta");
  await expect.element(page.getByText("Updating results…")).toBeInTheDocument();
  expect(observedListKeys.at(-1)).toEqual(queryKeys.books.list({ q: "alpha" }));
  await expect.element(page.getByText("Alpha")).toBeInTheDocument();
  await expect
    .element(page.getByRole("checkbox", { name: "Select all loaded books" }))
    .not.toBeChecked();
  await expect.element(page.getByRole("button", { name: /Load more/i })).not.toBeInTheDocument();
  expect(client.getQueryData(queryKeys.books.list({ q: "beta" }))).toBeUndefined();
  await user.click(page.getByRole("checkbox", { name: "Select" }));
  await expect
    .element(page.getByRole("toolbar", { name: "Batch actions" }))
    .toMatchTextContent("1 selected");
  pending.resolve(booksResponse([destination]));
  await expect.element(page.getByText("Beta")).toBeInTheDocument();
  await expect
    .element(page.getByRole("toolbar", { name: "Batch actions" }))
    .not.toBeInTheDocument();
  await expect.element(page.getByRole("checkbox", { name: "Select" })).not.toBeChecked();
  expect(observedListKeys.at(-1)).toEqual(queryKeys.books.list({ q: "beta" }));
  expect(
    client.getQueryData<InfiniteData<BookListResponse>>(queryKeys.books.list({ q: "alpha" }))
      ?.pages[0].items,
  ).toEqual([origin]);
  expect(
    client.getQueryData<InfiniteData<BookListResponse>>(queryKeys.books.list({ q: "beta" }))
      ?.pages[0].items,
  ).toEqual([destination]);
  fetchSpy.mockRestore();
});

test("a late old page cannot append to a new filter result", async () => {
  const oldPage = deferredResponse();
  const origin = bookFixture({ title: "Origin" });
  const destination = bookFixture({ title: "Destination", id: "destination" });
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    let raw: string;
    if (typeof input === "string") raw = input;
    else if (input instanceof URL) raw = input.href;
    else raw = input.url;
    const params = new URL(raw, window.location.origin).searchParams;
    return params.has("cursor") ? oldPage.promise : Promise.resolve(booksResponse([destination]));
  });
  const { client } = await renderLibrary({ items: [origin], nextCursor: "old-page" });
  await userEvent.setup().click(page.getByRole("button", { name: "Load more" }));
  await userEvent.fill(searchBox(), "destination");
  await expect.element(page.getByText("Destination")).toBeInTheDocument();
  oldPage.resolve(booksResponse([bookFixture({ title: "Late Old Page", id: "late" })]));
  await act(async () => {
    await oldPage.promise;
  });
  await expect.element(page.getByText("Late Old Page")).not.toBeInTheDocument();
  expect(
    client.getQueryData<InfiniteData<BookListResponse>>(queryKeys.books.list({ q: "destination" }))
      ?.pages,
  ).toHaveLength(1);
  fetchSpy.mockRestore();
});

test("a successful empty response replaces placeholders", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  const { client } = await renderLibrary({
    items: [bookFixture({ title: "Origin" })],
    nextCursor: null,
  });
  await userEvent.fill(page.getByRole("searchbox"), "missing");
  await expect.element(page.getByText("Origin")).toBeInTheDocument();
  pending.resolve(booksResponse([]));
  await expect.element(page.getByText("No books match these filters")).toBeInTheDocument();
  await expect.element(page.getByText("Origin")).not.toBeInTheDocument();
  expect(
    client.getQueryData<InfiniteData<BookListResponse>>(queryKeys.books.list({ q: "missing" }))
      ?.pages[0].items,
  ).toEqual([]);
  fetchSpy.mockRestore();
});

test.each(["grid", "table"] as const)(
  "invalid active text prevents the whole %s request, paging and invalidation until corrected",
  async (view) => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(booksResponse([bookFixture({ title: "Corrected" })]));
    const { client } = await renderLibrary({
      items: [bookFixture()],
      nextCursor: "next-page",
      visit: { view, filters: { pages: { gte: 100 } } },
      cacheParams: { pages_gte: 100 },
    });
    await expect.element(page.getByRole("searchbox")).toBeInTheDocument();
    if (view === "table") await expect.element(findLibraryTable()).toBeInTheDocument();
    await userEvent.fill(searchBox(), "x".repeat(201));
    await expect.element(searchBox()).toHaveValue("x".repeat(201));
    await expect.element(searchBox()).toHaveAttribute("aria-invalid", "true");
    await expect
      .element(searchBox())
      .toHaveAccessibleDescription("Use 200 characters or fewer, excluding surrounding spaces.");
    await expect.element(page.getByRole("alert")).toMatchTextContent("Results are paused.");
    await expect.element(page.getByRole("button", { name: /Load more/i })).not.toBeInTheDocument();
    await expect.element(page.getByText("No books match these filters")).not.toBeInTheDocument();
    await act(async () => {
      await client.invalidateQueries({ queryKey: queryKeys.books.all });
      await client.refetchQueries({ queryKey: queryKeys.books.all });
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    await expect.element(page.getByTestId("visit-params")).toMatchTextContent("pages_gte=100");
    await userEvent.fill(searchBox(), "The Sea");
    await expect.element(page.getByText("Corrected")).toBeInTheDocument();
    const requested = fetchSpy.mock.calls.map(([input]) => {
      let raw: string;
      if (typeof input === "string") raw = input;
      else if (input instanceof URL) raw = input.href;
      else raw = input.url;
      return new URL(raw, window.location.origin).searchParams;
    });
    expect(requested.filter((params) => params.get("q") === "The Sea")).toHaveLength(1);
    expect(requested.every((params) => params.get("pages_gte") === "100")).toBe(true);
    expect(requested.filter((params) => !params.has("q"))).toHaveLength(1);
    await expect.element(page.getByRole("alert")).not.toBeInTheDocument();
    fetchSpy.mockRestore();
  },
);

test("an uncached restored overlong filter stays editable and Clear recovers the full request", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  await renderLibrary({
    items: [],
    nextCursor: null,
    uncached: true,
    visit: { filters: { title: { op: "ne", text: "😀".repeat(201) }, q: "Sea" } },
  });
  await expect.element(page.getByRole("searchbox")).toHaveValue("Sea");
  await expect.element(page.getByRole("alert")).toMatchTextContent("Results are paused.");
  const user = userEvent;
  await user.click(page.getByRole("button", { name: /^Filters/ }));
  const title = await drawerSection(page.getByRole("dialog"), "Title");
  const input = page.elementLocator(title).getByRole("textbox", { name: "Filter value" });
  await expect.element(input).toHaveValue("😀".repeat(201));
  await expect.element(input).toHaveAttribute("aria-invalid", "true");
  await expect
    .element(input)
    .toHaveAccessibleDescription("Use 200 characters or fewer, excluding surrounding spaces.");
  expect(fetchSpy).not.toHaveBeenCalled();
  await user.keyboard("{Escape}");
  await user.click(page.getByRole("button", { name: /^Clear all$/ }));
  await expect.element(searchBox()).toBeInTheDocument();
  await expect.element(page.getByRole("button", { name: /^Filters/ })).toBeInTheDocument();
  await expect.element(page.getByText("Updating results…")).toBeInTheDocument();
  expect(document.querySelector('[aria-busy="true"]')).not.toBeNull();
  await expect.element(page.getByText("No books yet")).not.toBeInTheDocument();
  await expect.element(page.getByText("No books match these filters")).not.toBeInTheDocument();
  expect(fetchSpy.mock.calls.map(([input]) => input)).toEqual([
    new URL("/api/v1/books", window.location.origin),
  ]);
  pending.resolve(booksResponse([bookFixture({ title: "Recovered" })]));
  await expect.element(page.getByText("Recovered")).toBeInTheDocument();
  await expect.element(searchBox()).toHaveValue("");
  await expect.element(page.getByText("Updating results…")).not.toBeInTheDocument();
  expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  expect(fetchSpy).toHaveBeenCalledTimes(1);
  expect(fetchSpy.mock.calls[0]?.[0]).toEqual(new URL("/api/v1/books", window.location.origin));
  fetchSpy.mockRestore();
});

test.each([false, true])(
  "server 422 remains recoverable with controls mounted, initial=%s",
  async (uncached) => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          status: 422,
          type: "https://reverie.example/probs/invalid-query",
          title: "Invalid query",
          detail: "A server-side restriction",
        }),
        { status: 422, headers: { "Content-Type": "application/problem+json" } },
      ),
    );
    await renderLibrary({
      items: [bookFixture()],
      nextCursor: null,
      uncached,
      visit: { filters: { q: "restricted" } },
      cacheParams: { q: "restricted" },
    });
    if (!uncached) await userEvent.fill(page.getByRole("searchbox"), "rejected");
    await expect
      .element(page.getByRole("alert"))
      .toMatchTextContent("These filters could not be used.");
    await expect.element(searchBox()).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: /^Filters/ })).toBeInTheDocument();
    await expect.element(page.getByText("No books match these filters")).not.toBeInTheDocument();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    fetchSpy.mockResolvedValue(booksResponse([bookFixture({ title: "Recovered" })]));
    await userEvent.fill(searchBox(), "corrected");
    await expect.element(page.getByText("Recovered")).toBeInTheDocument();
    await expect.element(page.getByRole("alert")).not.toBeInTheDocument();
    fetchSpy.mockRestore();
  },
);

test("an initial uncached load uses the skeleton, then mounts the controls with its result", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  await renderLibrary({ items: [], nextCursor: null, uncached: true });
  await expect.element(page.getByRole("searchbox")).not.toBeInTheDocument();
  expect(document.querySelector('[aria-busy="true"]')).not.toBeNull();
  pending.resolve(booksResponse([bookFixture({ title: "Initial Result" })]));
  await expect.element(page.getByText("Initial Result")).toBeInTheDocument();
  await expect.element(searchBox()).toBeInTheDocument();
  fetchSpy.mockRestore();
});

test("a changed-query failure reaches the error boundary without manufacturing an empty result", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
  await renderLibrary({ items: [bookFixture()], nextCursor: null });
  await userEvent.fill(page.getByRole("searchbox"), "different");
  pending.reject(new Error("Query unavailable"));
  await expect.element(page.getByText("Query unavailable")).toBeInTheDocument();
  await expect.element(page.getByText("No books match these filters")).not.toBeInTheDocument();
  fetchSpy.mockRestore();
  errorSpy.mockRestore();
});

test("a superseded filter request is aborted and cannot overwrite the current rows", async () => {
  const obsolete = deferredResponse();
  let obsoleteSignal: AbortSignal | undefined;
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    let raw: string;
    if (typeof input === "string") raw = input;
    else if (input instanceof URL) raw = input.href;
    else raw = input.url;
    const url = new URL(raw, window.location.origin);
    if (url.searchParams.get("q") === "obsolete") {
      obsoleteSignal = init?.signal ?? undefined;
      return obsolete.promise;
    }
    return Promise.resolve(
      booksResponse([bookFixture({ title: "Current Result", id: "current" })]),
    );
  });
  await renderLibrary({ items: [bookFixture({ title: "Origin" })], nextCursor: null });
  await userEvent.fill(page.getByRole("searchbox"), "obsolete");
  await vi.waitFor(() => {
    expect(obsoleteSignal).toBeDefined();
  });
  await userEvent.fill(searchBox(), "current");
  await expect.element(page.getByText("Current Result")).toBeInTheDocument();
  expect(obsoleteSignal?.aborted).toBe(true);
  obsolete.resolve(booksResponse([bookFixture({ title: "Obsolete Result", id: "obsolete" })]));
  await act(async () => {
    await obsolete.promise;
  });
  await expect.element(page.getByText("Obsolete Result")).not.toBeInTheDocument();
  await expect.element(page.getByText("Current Result")).toBeInTheDocument();
  fetchSpy.mockRestore();
});

test("loading another page retains selected books", async () => {
  const fetchSpy = vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValue(booksResponse([bookFixture({ id: "page-two", title: "Second page" })]));
  await renderLibrary({
    items: [bookFixture({ title: "First page" })],
    nextCursor: "page-two",
    visit: { view: "table" },
  });
  await expect.element(findLibraryTable()).toBeInTheDocument();
  const user = userEvent;
  await user.click(page.getByRole("checkbox", { name: "Select" }));
  await user.click(page.getByRole("button", { name: /Load more/i }));
  await expect.element(page.getByText("Second page")).toBeInTheDocument();
  await expect
    .element(page.getByRole("toolbar", { name: "Batch actions" }))
    .toMatchTextContent("1 selected");
  await expect.element(page.getByRole("checkbox", { name: "Select" }).elements()[0]).toBeChecked();
  await expect
    .element(page.getByRole("checkbox", { name: "Select" }).elements()[1])
    .not.toBeChecked();
  fetchSpy.mockRestore();
});

function toastAction(kind: "success" | "error", label: string): () => void {
  const action = vi
    .mocked(toast[kind])
    .mock.calls.map((call) => call[1]?.action)
    .find(
      (candidate) =>
        candidate !== null &&
        typeof candidate === "object" &&
        "label" in candidate &&
        candidate.label === label,
    );
  if (action === null || typeof action !== "object" || !("onClick" in action))
    throw new Error(`Missing ${label} action`);
  return () => {
    action.onClick({} as MouseEvent<HTMLButtonElement>);
  };
}

async function editTitle(title: string, next: string): Promise<void> {
  const cell = page.getByText(title).element().closest('[role="gridcell"]');
  if (cell === null) throw new Error("Missing title cell");
  const mark = cell.querySelector<HTMLElement>("img, span[aria-hidden='true']");
  if (mark === null) throw new Error("Missing title-cell cover mark");
  await userEvent.dblClick(mark);
  const input = page.getByRole("textbox");
  await userEvent.fill(input, next);
  await userEvent.keyboard("{Enter}");
}

function metadataResponse(title: string): Response {
  return new Response(
    JSON.stringify({
      title,
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
    }),
    { headers: { "Content-Type": "application/json", ETag: '"alpha-version"' } },
  );
}

test.each(["before", "after"])(
  "a visible source edit begun %s intent changes and its undo stay live while the destination loads",
  async (timing) => {
    const destination = deferredResponse();
    const edit = deferredResponse();
    const origin = bookFixture({ title: "Alpha", id: "alpha" });
    const beta = bookFixture({ title: "Beta", id: "beta", work_id: "beta-work" });
    const alphaKey = queryKeys.books.list({ q: "alpha" });
    const betaKey = queryKeys.books.list({ q: "beta" });
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
      let path: string;
      if (typeof input === "string") path = input;
      else if (input instanceof URL) path = input.href;
      else path = input.url;
      if (path.includes("/metadata/revert"))
        return Promise.resolve(new Response(null, { status: 204 }));
      if (path.endsWith("/metadata"))
        return init?.method === "PATCH" ? edit.promise : Promise.resolve(metadataResponse("Alpha"));
      if (path.includes("q=beta")) return destination.promise;
      return Promise.reject(new Error(`Unexpected request: ${path}`));
    });
    const { client } = await renderLibrary({
      items: [origin],
      nextCursor: null,
      cacheParams: { q: "alpha" },
      visit: { filters: { q: "alpha" }, view: "table" },
      me: meFixture(),
    });
    await expect.element(findLibraryTable()).toBeInTheDocument();
    if (timing === "before") await editTitle("Alpha", "Alpha edited");
    await userEvent.fill(searchBox(), "beta");
    await expect.element(page.getByText("Updating results…")).toBeInTheDocument();
    if (timing === "after") await editTitle("Alpha", "Alpha edited");
    edit.resolve(
      new Response(
        JSON.stringify({
          fields: {
            title: {
              value: "Alpha edited",
              version_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
              previous_version_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            },
          },
        }),
        { headers: { "Content-Type": "application/json" } },
      ),
    );
    await expect.element(page.getByRole("link", { name: "Alpha edited" })).toBeInTheDocument();
    expect(
      client.getQueryData<InfiniteData<BookListResponse>>(alphaKey)?.pages[0].items[0].title,
    ).toBe("Alpha edited");
    expect(client.getQueryData(betaKey)).toBeUndefined();
    act(() => {
      toastAction("success", "Undo")();
    });
    await expect.element(page.getByRole("link", { name: "Alpha" })).toBeInTheDocument();
    expect(client.getQueryData<InfiniteData<BookListResponse>>(alphaKey)?.pages[0].items).toEqual([
      origin,
    ]);
    expect(client.getQueryData(betaKey)).toBeUndefined();
    destination.resolve(booksResponse([beta]));
    await expect.element(page.getByText("Beta")).toBeInTheDocument();
    expect(client.getQueryData<InfiniteData<BookListResponse>>(betaKey)?.pages[0].items).toEqual([
      beta,
    ]);
    fetchSpy.mockRestore();
  },
);

test("conflict Reload fetches and displays the retained source without changing the destination", async () => {
  const destination = deferredResponse();
  const origin = bookFixture({ title: "Alpha", id: "alpha" });
  const refreshed = { ...origin, title: "Alpha refreshed" };
  const beta = bookFixture({ title: "Beta", id: "beta", work_id: "beta-work" });
  const alphaKey = queryKeys.books.list({ q: "alpha" });
  const betaKey = queryKeys.books.list({ q: "beta" });
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    let path: string;
    if (typeof input === "string") path = input;
    else if (input instanceof URL) path = input.href;
    else path = input.url;
    if (path.endsWith("/metadata"))
      return Promise.resolve(
        init?.method === "PATCH"
          ? new Response(
              JSON.stringify({
                status: 412,
                type: "https://reverie.example/probs/if-match-mismatch",
                title: "Precondition Failed",
              }),
              { status: 412, headers: { "Content-Type": "application/problem+json" } },
            )
          : metadataResponse("Alpha refreshed"),
      );
    if (path.includes("q=beta")) return destination.promise;
    if (path.includes("q=alpha")) return Promise.resolve(booksResponse([refreshed]));
    return Promise.reject(new Error(`Unexpected request: ${path}`));
  });
  const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
  const { client } = await renderLibrary({
    items: [origin],
    nextCursor: null,
    cacheParams: { q: "alpha" },
    visit: { filters: { q: "alpha" }, view: "table" },
    me: meFixture(),
  });
  await expect.element(findLibraryTable()).toBeInTheDocument();
  await userEvent.fill(searchBox(), "beta");
  await expect.element(page.getByText("Updating results…")).toBeInTheDocument();
  await editTitle("Alpha", "Rejected edit");
  await vi.waitFor(() => {
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });
  act(() => {
    toastAction("error", "Reload")();
  });
  await expect.element(page.getByText("Alpha refreshed")).toBeInTheDocument();
  expect(client.getQueryData<InfiniteData<BookListResponse>>(alphaKey)?.pages[0].items).toEqual([
    refreshed,
  ]);
  expect(client.getQueryData(betaKey)).toBeUndefined();
  destination.resolve(booksResponse([beta]));
  await expect.element(page.getByText("Beta")).toBeInTheDocument();
  expect(client.getQueryData<InfiniteData<BookListResponse>>(betaKey)?.pages[0].items).toEqual([
    beta,
  ]);
  fetchSpy.mockRestore();
  errorSpy.mockRestore();
});

test("rapid intent changes keep source patches live and returning to cached source preserves provenance", async () => {
  const pending = deferredResponse();
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(() => pending.promise);
  const origin = bookFixture({ title: "Alpha", id: "alpha" });
  const alphaKey = queryKeys.books.list({ q: "alpha" });
  const { client } = await renderLibrary({
    items: [origin],
    nextCursor: null,
    cacheParams: { q: "alpha" },
    visit: { filters: { q: "alpha" }, view: "table" },
  });
  await expect.element(findLibraryTable()).toBeInTheDocument();
  await userEvent.fill(searchBox(), "beta");
  await expect.element(page.getByText("Updating results…")).toBeInTheDocument();
  await userEvent.fill(searchBox(), "gamma");
  act(() => {
    client.setQueryData<InfiniteData<BookListResponse>>(alphaKey, {
      pages: [{ items: [{ ...origin, title: "Alpha patched" }], next_cursor: null }],
      pageParams: [undefined],
    });
  });
  await expect.element(page.getByRole("link", { name: "Alpha patched" })).toBeInTheDocument();
  expect(observedListKeys.at(-1)).toEqual(alphaKey);
  expect(client.getQueryData(queryKeys.books.list({ q: "beta" }))).toBeUndefined();
  expect(client.getQueryData(queryKeys.books.list({ q: "gamma" }))).toBeUndefined();
  await userEvent.fill(searchBox(), "alpha");
  await vi.waitFor(async () => {
    await expect.element(page.getByText("Updating results…")).not.toBeInTheDocument();
  });
  await expect.element(page.getByRole("link", { name: "Alpha patched" })).toBeInTheDocument();
  expect(observedListKeys.at(-1)).toEqual(alphaKey);
  pending.resolve(booksResponse([bookFixture({ title: "Obsolete", id: "obsolete" })]));
  await act(async () => {
    await pending.promise;
  });
  await expect.element(page.getByText("Obsolete")).not.toBeInTheDocument();
  fetchSpy.mockRestore();
});
