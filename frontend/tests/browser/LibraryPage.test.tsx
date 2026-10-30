import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render } from "vitest-browser-react";
import { page, userEvent, type Locator } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import {
  emptyFilterState,
  filterStateToParams,
  LibraryVisitSchema,
  type FilterState,
  type LibraryVisit,
} from "@/routes/library-params";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { useSyncExternalStore, type ReactElement } from "react";

import type { BookListItem, BookListResponse, ListBooksParams, Preferences } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { LibraryPage } from "@/pages/library/LibraryPage";

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
}: RenderOpts): Promise<void> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.shelves.list(), []);
  const params: ListBooksParams = cacheParams ?? {};
  const response: BookListResponse = { items, next_cursor: nextCursor };
  for (const slot of [params, ...extraCacheParams]) {
    client.setQueryData(queryKeys.books.list(slot), {
      pages: [response],
      pageParams: [undefined],
    });
  }
  if (preferences !== null) {
    client.setQueryData(queryKeys.auth.preferences(), preferences ?? preferencesFixture());
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
}

describe("LibraryPage", () => {
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

  describe("per-user sort", () => {
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
  });
});
