import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { QueryClient, QueryClientProvider, type InfiniteData } from "@tanstack/react-query";
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
import { RouterProvider, createMemoryRouter, useLocation, type RouteObject } from "react-router";
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
    metadata_etag: '"aaaaaaaaaaaaaaaaaaaaaa"',
    reading_etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
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
  seriesLabels?: { id: string; value: string }[];

  url?: string;
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
  seriesLabels = [],
  url = "/library",
}: RenderOpts): Promise<{
  client: QueryClient;
}> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.shelves.list(), []);
  if (visit?.filters?.series !== undefined)
    client.setQueryData(queryKeys.series.resolve(visit.filters.series), seriesLabels);
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

  history.replaceState(navigation.currentEntry?.getState() ?? null, "", url);
  navigation.updateCurrentEntry({
    state: {
      reverieLibrary: { filters: { ...emptyFilterState(), ...visit?.filters }, view: visit?.view },
    },
  });
  function LocationProbe(): ReactElement {
    const location = useLocation();
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
    return (
      <>
        <div data-testid="visit-params">{params.toString()}</div>
        <div data-testid="router-search">{location.search}</div>
      </>
    );
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
    const router = createMemoryRouter(routes, { initialEntries: [url] });
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }

  await render(<Wrapper />);
  return { client };
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
    expect(page.getByTestId("router-search").element().textContent).not.toContain("sort=");
    expect(window.location.search).not.toContain("sort=");
    fetchSpy.mockRestore();
  });

  test("clearing all filters preserves the visit view and the stored sort", async () => {
    // REV-REQ-0002: Clear removes every condition and preserves view and sort.
    const series = ["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"];
    const shelf = ["cccccccc-cccc-4ccc-8ccc-cccccccccccc"];
    await renderLibrary({
      items: [],
      nextCursor: null,
      visit: { filters: { series, shelf, status: { any: ["unread"], none: [] } }, view: "table" },
      preferences: preferencesFixture({ sort_stack: "title" }),
      cacheParams: { series_any: series, shelf_any: shelf, status_any: ["unread"], sort: "title" },
      extraCacheParams: [{ sort: "title" }],
    });
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /clear all filters/i }));
    await vi.waitFor(() => {
      const search = page.getByTestId("visit-params").element().textContent;
      expect(search).toContain("view=table");
      expect(search).toBe("view=table");
    });

    await expect.element(page.getByText("Sorted by Title ascending")).toBeInTheDocument();
    await expect
      .element(page.getByRole("radio", { name: "Table", checked: true }))
      .toBeInTheDocument();
  });

  describe("filter chips", () => {
    test("a chip's remove control drops exactly that filter param", async () => {
      // REV-REQ-0002: a condition edit and section Clear preserve sibling sections.
      const first = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
      const second = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
      const shelfId = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
      const shelf = [shelfId];
      const siblings = { status_any: ["unread"], shelf_any: shelf };
      await renderLibrary({
        items: [bookFixture()],
        nextCursor: null,
        visit: {
          filters: {
            series: [first, second],
            shelf,
            status: { any: ["unread"], none: [] },
          },
        },
        seriesLabels: [
          { id: first, value: "Discworld" },
          { id: second, value: "Culture" },
        ],
        cacheParams: { series_any: [first, second], ...siblings },
        extraCacheParams: [{ series_any: [second], ...siblings }, siblings],
      });
      const chips = page.getByTestId("filter-chips");
      const user = userEvent;
      await user.click(chips.getByRole("button", { name: /Remove.*Series: Discworld/ }));
      await vi.waitFor(() => {
        const search = page.getByTestId("visit-params").element().textContent;
        expect(search).not.toContain(first);
        expect(search).toContain(`series_any=${second}`);
        expect(search).toContain("status_any=unread");
        expect(search).toContain(`shelf_any=${shelfId}`);
      });
      await user.click(page.getByRole("button", { name: /^Filters/ }));
      await user.click(page.getByRole("button", { name: "Clear Series filters" }));
      await vi.waitFor(() => {
        const search = page.getByTestId("visit-params").element().textContent;
        expect(search).not.toContain("series_any");
        expect(search).toContain("status_any=unread");
        expect(search).toContain(`shelf_any=${shelfId}`);
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
  });

  describe("per-user sort", () => {
    test.each([
      {
        resolved: "the stored sort",
        preferences: preferencesFixture({ sort_stack: "-pages" }),
        cacheParams: { sort: "-pages" },
        announced: "Sorted by Pages descending",
      },
      {
        resolved: "the installation order",
        preferences: preferencesFixture(),
        cacheParams: {},
        announced: "Sorted by Added descending",
      },
    ])(
      "a stale URL ?sort= is inert: $resolved still applies",
      async ({ preferences, cacheParams, announced }) => {
        const fetchSpy = vi.spyOn(globalThis, "fetch");
        await renderLibrary({
          items: [bookFixture()],
          nextCursor: null,
          preferences,
          cacheParams,
          url: "/library?sort=title",
        });
        await expect.element(page.getByText(announced)).toBeInTheDocument();
        await expect.element(page.getByText("The Brothers Karamazov").first()).toBeInTheDocument();
        expect(fetchSpy).not.toHaveBeenCalled();
        fetchSpy.mockRestore();
      },
    );

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

// Data loss boundary: exact operations, source routing, acknowledgement recovery and input Undo ownership.
test("metadata Undo survives table refreshes", async () => {
  let current = bookFixture({ pages: 100 });
  const originKey = queryKeys.books.list({});
  const op = "33333333-3333-4333-8333-333333333333";
  const pagesOp = "44444444-4444-4444-8444-444444444444";
  let requestId = "";
  let undoCalls = 0;
  let resolvingSave = false;
  const pendingUndo = deferredResponse();
  const values = () => ({
    title: current.title,
    subtitle: current.subtitle,
    description: null,
    language: null,
    publisher: null,
    pub_date: null,
    isbn_10: null,
    isbn_13: current.isbn_13,
    isbn_mismatch: false,
    pages: current.pages,
    content_rating: null,
    genres: [],
    moods: [],
    tags: [],
    contributors: { author: current.authors, editor: [], translator: [] },
    work_identifiers: {},
    manifestation_identifiers: {},
  });
  const operation = {
    operation_id: op,
    field_name: "title",
    expires_at: new Date(Date.now() + 3600000).toISOString(),
    status: "eligible",
  };
  const response = (body: unknown) =>
    new Response(JSON.stringify(body), {
      headers: { "Content-Type": "application/json", ETag: current.metadata_etag },
    });
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const path = input instanceof Request ? input.url : input.toString();
    if (path.endsWith("/metadata") && init?.method === "PATCH") {
      const body = parseJsonBody(init.body);
      if (
        typeof body !== "object" ||
        body === null ||
        !("request_id" in body) ||
        typeof body.request_id !== "string"
      )
        throw new Error("missing request identity");
      if ("pages" in body) {
        current = { ...current, pages: 250, metadata_etag: '"eeeeeeeeeeeeeeeeeeeeee"' };
        return Promise.resolve(
          response({
            values: values(),
            request_id: body.request_id,
            operations: [{ ...operation, field_name: "pages", operation_id: pagesOp }],
          }),
        );
      }
      requestId = body.request_id;
      expect(new Headers(init.headers).get("If-Match")).toBe(current.metadata_etag);
      current = { ...current, title: "Saved title", metadata_etag: '"cccccccccccccccccccccc"' };
      resolvingSave = true;
      return Promise.reject(new TypeError("Lost acknowledgement"));
    }
    if (path.endsWith("/operations/resolve")) {
      if (resolvingSave) {
        expect(parseJsonBody(init?.body)).toEqual({ request_id: requestId });
        resolvingSave = false;
      } else expect(parseJsonBody(init?.body)).toEqual({ operation_id: op });
      return Promise.resolve(
        response({ values: values(), outcome: "saved", operations: [operation] }),
      );
    }
    if (path.endsWith("/metadata/undo")) {
      if (JSON.stringify(parseJsonBody(init?.body)) === JSON.stringify({ operation_id: pagesOp })) {
        current = { ...current, pages: 100, metadata_etag: '"ffffffffffffffffffffff"' };
        return Promise.resolve(response({ values: values(), outcome: "restored", operations: [] }));
      }
      undoCalls += 1;
      expect(parseJsonBody(init?.body)).toEqual({ operation_id: op });
      if (undoCalls === 1)
        return Promise.resolve(
          new Response(JSON.stringify({ type: "/if-match-mismatch", status: 412 }), {
            status: 412,
            headers: { "Content-Type": "application/problem+json" },
          }),
        );
      expect(new Headers(init?.headers).get("If-Match")).toBe(current.metadata_etag);
      return pendingUndo.promise;
    }
    if (path.includes("/api/v1/books"))
      return Promise.resolve(booksResponse(path.includes("missing") ? [] : [current]));
    if (path.includes(PREFERENCES_PATH))
      return Promise.resolve(preferencesResponse(preferencesFixture({ view: "table" })));
    throw new Error(`Unexpected request ${path}`);
  });
  const { client } = await renderLibrary({
    items: [current],
    nextCursor: null,
    visit: { view: "table" },
  });
  await expect.element(findLibraryTable()).toBeInTheDocument();
  const titleCell = page.getByRole("gridcell").filter({ hasText: "The Brothers Karamazov" });
  await userEvent.click(titleCell, { position: { x: 3, y: 3 } });
  await userEvent.keyboard("{F2}");
  const editor = findLibraryTable().getByRole("textbox");
  await userEvent.type(editor, "Draft");
  await userEvent.keyboard("{Control>}z{/Control}");
  await expect.element(editor).toHaveValue("The Brothers Karamazov");
  expect(undoCalls).toBe(0);
  expect(requestId).toBe("");
  await editor.fill("Saved title");
  await userEvent.keyboard("{Enter}");
  const undo = page.getByRole("button", { name: "Undo last edit" });
  await expect.element(undo).toBeEnabled();
  await expect
    .element(page.getByText("Title for “The Brothers Karamazov”", { exact: true }))
    .toBeInTheDocument();
  const pagesCell = page.getByRole("gridcell", { name: "100", exact: true });
  await userEvent.click(pagesCell);
  await userEvent.keyboard("{F2}");
  await findLibraryTable().getByRole("textbox").fill("250");
  await userEvent.keyboard("{Enter}");
  await expect
    .element(page.getByText("Pages for “Saved title”", { exact: true }))
    .toBeInTheDocument();
  await userEvent.click(undo);
  await expect
    .element(page.getByText("Title for “The Brothers Karamazov”", { exact: true }))
    .toBeInTheDocument();
  const ratingCell = page
    .getByRole("row")
    .filter({ hasText: "Saved title" })
    .getByRole("gridcell")
    .nth(9);
  await userEvent.click(ratingCell);
  await userEvent.keyboard("{F2}");
  await expect.element(page.getByRole("group", { name: "Rating", exact: true })).toHaveFocus();
  await userEvent.keyboard("{Control>}z{/Control}");
  expect(undoCalls).toBe(0);
  await expect.element(page.getByRole("group", { name: "Rating", exact: true })).toHaveFocus();
  await userEvent.keyboard("{Escape}");
  await userEvent.fill(page.getByRole("searchbox"), "missing");
  await expect.element(findLibraryTable()).not.toBeInTheDocument();
  await expect.element(undo).toBeEnabled();
  await userEvent.click(undo);
  await vi.waitFor(() => {
    expect(undoCalls).toBe(2);
  });
  await expect.element(undo).toHaveAttribute("data-pending");
  await expect.element(undo).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  expect(undoCalls).toBe(2);
  current = {
    ...current,
    title: "The Brothers Karamazov",
    metadata_etag: '"dddddddddddddddddddddd"',
  };
  pendingUndo.resolve(
    response({
      values: values(),
      outcome: "restored",
      operations: [{ ...operation, status: "consumed" }],
    }),
  );
  await expect
    .element(page.getByText(/Title for “The Brothers Karamazov” restored/))
    .toBeInTheDocument();
  await expect.element(undo).toBeDisabled();
  await expect.element(page.getByRole("button", { name: "Keyboard shortcuts" })).toHaveFocus();
  await vi.waitFor(() => {
    expect(
      client.getQueryData<InfiniteData<BookListResponse>>(originKey)?.pages[0]?.items[0]?.title,
    ).toBe(current.title);
  });
  fetchSpy.mockRestore();
  await cleanup();
  client.clear();
  const other = bookFixture({
    id: "55555555-5555-4555-8555-555555555555",
    work_id: "66666666-6666-4666-8666-666666666666",
    title: "Another book",
    pages: 300,
  });
  const readingTitle = current.title;
  const recoveryReads = new Map<string, number>();
  const firstRecovery = deferredResponse();
  const correctingSave = deferredResponse();
  vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const path = input instanceof Request ? input.url : input.toString();
    if (path.endsWith("/metadata") && init?.method === "PATCH") {
      const body = parseJsonBody(init.body);
      if (
        typeof body === "object" &&
        body !== null &&
        "pages" in body &&
        "request_id" in body &&
        typeof body.request_id === "string"
      ) {
        requestId = body.request_id;
        return correctingSave.promise;
      }
      return Promise.reject(new TypeError("Lost acknowledgement"));
    }
    if (path.endsWith("/operations/resolve")) {
      const id = path.includes(other.id) ? other.id : current.id;
      const count = (recoveryReads.get(id) ?? 0) + 1;
      recoveryReads.set(id, count);
      if (id === current.id && count === 1) return firstRecovery.promise;
      if (count > 3) throw new Error("Recovery did not settle");
      return Promise.resolve(
        response({
          values: { ...values(), title: id === other.id ? other.title : current.title },
          outcome: "unknown",
          operations: [],
        }),
      );
    }
    if (path.includes("/api/v1/books")) return Promise.resolve(booksResponse([current, other]));
    throw new Error(`Unexpected request ${path}`);
  });
  const recovered = await renderLibrary({
    items: [current, other],
    nextCursor: null,
    visit: { view: "table" },
  });
  const edit = async (title: string, text: string) => {
    await userEvent.click(page.getByRole("gridcell").filter({ hasText: title }), {
      position: { x: 3, y: 3 },
    });
    await userEvent.keyboard("{F2}");
    await findLibraryTable().getByRole("textbox").fill(text);
    await userEvent.keyboard("{Enter}");
  };
  await edit(readingTitle, "Unconfirmed title");
  await vi.waitFor(() => {
    expect(recoveryReads.get(current.id)).toBe(1);
  });
  await userEvent.click(page.getByRole("gridcell", { name: "100", exact: true }));
  await userEvent.keyboard("{F2}");
  await findLibraryTable().getByRole("textbox").fill("251");
  await userEvent.keyboard("{Enter}");
  current = { ...current, pages: 251 };
  firstRecovery.resolve(response({ values: values(), outcome: "unknown", operations: [] }));
  correctingSave.resolve(response({ values: values(), request_id: requestId, operations: [] }));
  await vi.waitFor(() => {
    expect(recoveryReads.get(current.id)).toBe(2);
  });
  await edit(other.title, "Another unconfirmed title");
  await vi.waitFor(() => {
    expect(recoveryReads.get(current.id)).toBe(2);
    expect(recoveryReads.get(other.id)).toBe(1);
    expect(recovered.client.isFetching()).toBe(0);
  });
  await expect
    .element(page.getByText("Submitted value: Unconfirmed title", { exact: true }))
    .toBeInTheDocument();
  await expect
    .element(page.getByText("Submitted value: Another unconfirmed title", { exact: true }))
    .toBeInTheDocument();
});
