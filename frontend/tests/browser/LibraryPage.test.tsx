import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, it, vi } from "vite-plus/test";
import {
  emptyFilterState,
  filterStateToParams,
  LibraryVisitSchema,
  type FilterState,
  type LibraryVisit,
} from "@/routes/library-params";
import { RouterProvider, createMemoryRouter, useLocation } from "react-router";
import { StrictMode, useSyncExternalStore, type ReactElement } from "react";
import { RelayEnvironmentProvider } from "react-relay";
import { z } from "zod";
import { parseSortParam, serializeSortParam, type BookListItem, type Preferences } from "@/api";
import { encodeGraphQLId } from "@/api/graphql";
import { relayEnvironment, setRelayAccount } from "@/lib/relay/environment";
import { queryClient as appQueryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { loader, type PreparedLibrary } from "@/routes/library";
import trustedDocuments from "../../../backend/graphql-documents.json";
import { LibraryPage } from "@/pages/library/LibraryPage";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ preference: "system", effective: "dark", setPreference: vi.fn() }),
}));
const revisionIds = new Map<string, string>();
function revisionId(bookId: string, etag: string): string {
  const key = `${bookId} ${etag}`;
  const known = revisionIds.get(key);
  if (known !== undefined) return known;
  const created = encodeGraphQLId("MetadataRevision", crypto.randomUUID());
  revisionIds.set(key, created);
  return created;
}

const originalState: unknown = navigation.currentEntry?.getState();
const originalURL = window.location.href;
const documents: Record<string, string> = trustedDocuments;
const wireRequest = z.object({ id: z.string(), variables: z.record(z.string(), z.unknown()) });
const listVariables = z.object({
  sourceKey: z.string(),
  filter: z.record(z.string(), z.unknown()).nullable().optional(),
  sort: z.array(z.object({ column: z.string(), direction: z.string() })).nullish(),
  cursor: z.string().nullable().optional(),
});
const metadataVariables = z.object({
  id: z.string(),
  baseline: z.string(),
  requestId: z.uuid(),
  patch: z.record(z.string(), z.unknown()),
});
const readingVariables = z.object({
  id: z.string(),
  baseline: z.string(),
  patch: z.object({
    status: z.string().nullable().optional(),
    rating: z.number().nullable().optional(),
  }),
});
beforeEach(() => {
  __seedCsrfTokenForTesting("synthetic-library-browser-csrf-token");
  setRelayAccount("library-browser");
});
afterEach(async () => {
  await cleanup();
  appQueryClient.clear();
  setRelayAccount(null);
  vi.restoreAllMocks();
  localStorage.clear();
  history.replaceState(originalState ?? null, "", originalURL);
});

function bookFixture(overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    metadata_etag: '"aaaaaaaaaaaaaaaaaaaaaa"',
    reading_etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
    id: "11111111-1111-4111-8111-111111111111",
    work_id: "22222222-2222-4222-8222-222222222222",
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
  hidden_columns: ["subtitle"],
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

function response(data: unknown): Response {
  return new Response(JSON.stringify({ data }), {
    headers: { "Content-Type": "application/json" },
  });
}
function refusal(code: string, errors?: Record<string, string>): Response {
  return new Response(
    JSON.stringify({ errors: [{ message: code, extensions: { code, errors } }] }),
    { headers: { "Content-Type": "application/json" } },
  );
}
function refused(root: string, typename: string, message: string, extra: object = {}): Response {
  return response({ [root]: { __typename: typename, message, ...extra } });
}
function savedCells(requestId: string | null, book: BookListItem, operations: unknown[]): Response {
  return response({
    updateMetadata: {
      __typename: "MetadataSaved",
      requestId,
      operations,
      manifestation: graphBook(book),
    },
  });
}
function restoredCells(book: BookListItem, operation: unknown): Response {
  return response({
    undoMetadata: {
      __typename: "MetadataRestored",
      operation,
      manifestation: graphBook(book),
    },
  });
}
const pageInfo = { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null };
function metadata(book: BookListItem) {
  return {
    __typename: "MetadataRevision",
    id: revisionId(book.id, book.metadata_etag),
    workId: encodeGraphQLId("Work", book.work_id),
    manifestationId: encodeGraphQLId("Manifestation", book.id),
    etag: book.metadata_etag,
    title: book.title,
    subtitle: book.subtitle,
    isbn13: book.isbn_13,
    pages: book.pages,
  };
}
function reading(book: BookListItem) {
  return {
    __typename: "ReadingRevision",
    etag: book.reading_etag,
    id: `reading_rev_${revisionId(book.id, book.reading_etag)}`,
    status: book.reading_state?.status?.toUpperCase() ?? null,
    rating: book.reading_state?.rating ?? null,
    progressPct: book.reading_state?.progress_pct ?? null,
    startedAt: book.reading_state?.started_at ?? null,
    finishedAt: book.reading_state?.finished_at ?? null,
    notes: null,
    lastReadAt: null,
  };
}
function graphBook(book: BookListItem) {
  return {
    __typename: "Manifestation",
    id: encodeGraphQLId("Manifestation", book.id),
    coverUrl: book.cover_url,
    isbn13: book.isbn_13,
    pages: book.pages,
    createdAt: book.created_at,
    work: {
      __typename: "Work",
      id: encodeGraphQLId("Work", book.work_id),
      title: book.title,
      subtitle: book.subtitle,
      selectedSeries: null,
      authors: {
        edges: book.authors.map((name, index) => ({
          cursor: String(index),
          node: {
            __typename: "Author",
            id: encodeGraphQLId(
              "Author",
              `30000000-0000-4000-8000-${String(index).padStart(12, "0")}`,
            ),
            name,
          },
        })),
        pageInfo,
      },
    },
    metadata: metadata(book),
    readingState: reading(book),
  };
}
const VIEWER_ID = encodeGraphQLId("User", "77777777-7777-4777-8777-777777777777");
const ME = {
  id: "77777777-7777-4777-8777-777777777777",
  display_name: "Reader",
  email: null,
  role: "adult",
  is_child: false,
  has_local_password: true,
  theme_preference: "dark",
};
function graphSort(raw: string | null) {
  return raw === null
    ? null
    : parseSortParam(raw).map((level) => ({
        column: level.field.toUpperCase(),
        direction: level.desc ? "DESC" : "ASC",
      }));
}
function graphPreferences(preferences: Preferences) {
  return {
    hiddenColumns: preferences.hidden_columns?.map((key) => key.toUpperCase()) ?? null,
    density: preferences.density?.toUpperCase() ?? null,
    view: preferences.view?.toUpperCase() ?? null,
    sort: graphSort(preferences.sort_stack),
    defaults: {
      hiddenColumns: preferences.defaults.hidden_columns.map((key) => key.toUpperCase()),
      density: preferences.defaults.density.toUpperCase(),
      view: preferences.defaults.view.toUpperCase(),
      sort: graphSort(preferences.defaults.sort_stack),
    },
  };
}
const preferenceInput = z.object({
  density: z.enum(["COMPACT", "COMFORTABLE"]).nullable().optional(),
  view: z.enum(["TABLE", "GRID"]).nullable().optional(),
  sort: z
    .array(z.object({ column: z.string(), direction: z.enum(["ASC", "DESC"]) }))
    .nullable()
    .optional(),
  columns: z.array(z.object({ column: z.string(), hidden: z.boolean() })).optional(),
  resetHiddenColumns: z.boolean().optional(),
});
function applyPreferences(preferences: Preferences, variables: unknown): Preferences {
  const input = preferenceInput.parse(variables);
  let hidden = preferences.hidden_columns;
  if (input.resetHiddenColumns === true) hidden = null;
  for (const change of input.columns ?? []) {
    const key = change.column.toLowerCase();
    const current = (hidden ?? preferences.defaults.hidden_columns).filter((item) => item !== key);
    hidden = change.hidden ? [...current, key] : current;
  }
  const densities = { COMPACT: "compact", COMFORTABLE: "comfortable" } as const;
  const views = { TABLE: "table", GRID: "grid" } as const;
  return {
    ...preferences,
    hidden_columns: hidden,
    ...(input.density === undefined
      ? {}
      : { density: input.density === null ? null : densities[input.density] }),
    ...(input.view === undefined ? {} : { view: input.view === null ? null : views[input.view] }),
    ...(input.sort === undefined
      ? {}
      : {
          sort_stack:
            input.sort === null
              ? null
              : serializeSortParam(
                  input.sort.map((level) => ({
                    field: z
                      .enum(["title", "author", "created_at", "pages"])
                      .parse(level.column.toLowerCase()),
                    desc: level.direction === "DESC",
                  })),
                ),
        }),
  };
}
const ids = z.array(z.string()).catch([]);
type Fixture = {
  books: BookListItem[];
  preferences: Preferences;
  calls: { name: string; variables: Record<string, unknown> }[];
  environment: ReturnType<typeof relayEnvironment>;
  prepared: PreparedLibrary | null | undefined;
};
function visitOf(fixture: Fixture): PreparedLibrary["owner"] {
  if (fixture.prepared == null) throw new Error("The Library loader has not prepared a visit.");
  return fixture.prepared.owner;
}
async function traverseToCurrentEntry(): Promise<void> {
  history.pushState(null, "", window.location.href);
  const arrived = new Promise<void>((resolve) => {
    navigation.addEventListener(
      "currententrychange",
      () => {
        resolve();
      },
      { once: true },
    );
  });
  history.back();
  await arrived;
}
type RenderOptions = {
  items?: BookListItem[];
  visit?: { filters?: Partial<FilterState>; view?: LibraryVisit["view"] };
  preferences?: Preferences;
  url?: string;
  seriesLabels?: { id: string; value: string }[];
  shelfLabels?: { id: string; value: string }[];
  handler?: (
    name: string,
    variables: Record<string, unknown>,
    fixture: Fixture,
  ) => Response | Promise<Response> | undefined;
};
async function renderLibrary({
  items = [bookFixture()],
  visit,
  preferences = preferencesFixture(),
  url = "/library",
  seriesLabels = [],
  shelfLabels = [],
  handler,
}: RenderOptions = {}) {
  const fixture: Fixture = {
    books: items,
    preferences,
    calls: [],
    environment: relayEnvironment(),
    prepared: undefined,
  };
  const names = new Map([...seriesLabels, ...shelfLabels].map((label) => [label.id, label.value]));
  function labels(variables: Record<string, unknown>) {
    return {
      authorLabels: ids
        .parse(variables.authorIds)
        .map((id) => ({ id, name: names.get(id) ?? "Author" })),
      seriesLabels: ids
        .parse(variables.seriesIds)
        .map((id) => ({ id, name: names.get(id) ?? "Series" })),
      shelfLabels: ids
        .parse(variables.shelfIds)
        .map((id) => ({ id, name: names.get(id) ?? "Shelf", itemCount: 0 })),
    };
  }
  function viewer() {
    return { id: VIEWER_ID, libraryPreferences: graphPreferences(fixture.preferences) };
  }
  vi.spyOn(globalThis, "fetch").mockImplementation((_input, init) => {
    const request = wireRequest.parse(JSON.parse(z.string().parse(init?.body)));
    const name = documents[request.id]?.match(/(?:query|mutation) (\w+)/)?.[1];
    if (name === undefined) throw new Error("Unknown trusted operation");
    const input = z.record(z.string(), z.unknown()).safeParse(request.variables.input);
    const variables = input.success ? input.data : request.variables;
    fixture.calls.push({ name, variables });
    const overridden = handler?.(name, variables, fixture);
    if (overridden !== undefined) return Promise.resolve(overridden);
    if (name === "LibraryPageQuery") {
      const variables = listVariables.parse(request.variables);
      const q = z.string().optional().parse(variables.filter?.q) ?? "";
      const [level = { column: "CREATED_AT", direction: "DESC" }] = variables.sort ?? [];
      const key = (book: BookListItem): string | number => {
        if (level.column === "PAGES") return book.pages ?? -1;
        return level.column === "TITLE" ? book.title : book.created_at;
      };
      const rows = fixture.books
        .filter((book) => book.title.toLowerCase().includes(q.toLowerCase()))
        .toSorted((left, right) => {
          const order = String(key(left)).localeCompare(String(key(right)), undefined, {
            numeric: true,
          });
          return level.direction === "DESC" ? -order : order;
        });
      return Promise.resolve(
        response({
          viewer: viewer(),
          ...labels(request.variables),
          shelves: { edges: [], pageInfo },
          ...(request.variables.eligible === true
            ? {
                books: {
                  edges: rows.map((book) => ({ cursor: book.id, node: graphBook(book) })),
                  pageInfo,
                },
              }
            : {}),
        }),
      );
    }
    if (name === "libraryBootstrapQuery") {
      const found = labels(variables);
      return Promise.resolve(
        response({
          viewer: viewer(),
          authorLabels: found.authorLabels.map(({ id }) => ({ id })),
          seriesLabels: found.seriesLabels.map(({ id }) => ({ id })),
          shelfLabels: found.shelfLabels.map(({ id }) => ({ id })),
        }),
      );
    }
    if (name === "LibraryPageLabelsRefetchQuery")
      return Promise.resolve(response(labels(variables)));
    if (name === "mutationsUpdateLibraryPreferencesMutation") {
      fixture.preferences = applyPreferences(fixture.preferences, variables);
      return Promise.resolve(
        response({
          updateLibraryPreferences: { __typename: "LibraryPreferencesUpdated", viewer: viewer() },
        }),
      );
    }
    throw new Error(`Unprovided operation ${name}`);
  });
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.auth.me(), ME);
  appQueryClient.setQueryData(queryKeys.auth.me(), ME);
  history.replaceState(navigation.currentEntry?.getState() ?? null, "", url);
  navigation.updateCurrentEntry({
    state: {
      reverieLibrary: { filters: { ...emptyFilterState(), ...visit?.filters }, view: visit?.view },
    },
  });
  await traverseToCurrentEntry();
  function Probe(): ReactElement {
    const location = useLocation();
    const raw = useSyncExternalStore(
      (changed) => {
        navigation.addEventListener("currententrychange", changed);
        return () => {
          navigation.removeEventListener("currententrychange", changed);
        };
      },
      () => JSON.stringify(navigation.currentEntry?.getState()),
    );
    const state = z.object({ reverieLibrary: LibraryVisitSchema }).parse(JSON.parse(raw));
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(filterStateToParams(state.reverieLibrary.filters))) {
      if (Array.isArray(value)) for (const item of value) params.append(key, item);
      else params.set(key, String(value));
    }
    if (state.reverieLibrary.view !== undefined) params.set("view", state.reverieLibrary.view);
    return (
      <>
        <div data-testid="visit-params">{params.toString()}</div>
        <div data-testid="router-search">{location.search}</div>
      </>
    );
  }
  const router = createMemoryRouter(
    [
      {
        path: "/library",
        loader: async (args) => {
          fixture.prepared = await loader(args);
          return fixture.prepared;
        },
        element: (
          <>
            <LibraryPage />
            <Probe />
          </>
        ),
      },
    ],
    { initialEntries: [url] },
  );
  await render(
    <StrictMode>
      <QueryClientProvider client={client}>
        <RelayEnvironmentProvider environment={fixture.environment}>
          <RouterProvider router={router} />
        </RelayEnvironmentProvider>
      </QueryClientProvider>
    </StrictMode>,
  );
  return fixture;
}
function preferenceWrites(fixture: Fixture) {
  return fixture.calls
    .filter((call) => call.name === "mutationsUpdateLibraryPreferencesMutation")
    .map((call) => call.variables);
}
function findLibraryTable() {
  return page.getByTestId("library-table");
}
async function editTitle(title: string, value: string): Promise<void> {
  await userEvent.hover(page.getByRole("rowheader", { name: new RegExp(title) }));
  await userEvent.click(page.getByRole("button", { name: `Edit title of ${title}` }));
  await page.getByRole("dialog").getByRole("textbox").fill(value);
  await userEvent.keyboard("{Enter}");
}
describe("LibraryPage", () => {
  test("table header click persists the sort to the account and never touches the URL", async () => {
    // REV-REQ-0001: a table header click stores the sort as the override and adds no `sort` parameter.
    const fixture = await renderLibrary({
      items: [bookFixture()],
      visit: { view: "table" },
    });
    await expect.element(findLibraryTable()).toBeInTheDocument();
    const user = userEvent;
    await user.click(page.getByRole("columnheader", { name: /^Title(?:\s|$)/ }));
    await vi.waitFor(() => {
      expect(preferenceWrites(fixture)).toEqual([
        { sort: [{ column: "TITLE", direction: "ASC" }] },
      ]);
    });
    expect(page.getByTestId("router-search").element().textContent).not.toContain("sort=");
    expect(window.location.search).not.toContain("sort=");
  });

  test("clearing all filters preserves the visit view and the stored sort", async () => {
    // REV-REQ-0002: Clear removes every condition and preserves view and sort.
    const series = [
      encodeGraphQLId("Series", "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
      encodeGraphQLId("Series", "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
    ];
    const shelf = [encodeGraphQLId("Shelf", "cccccccc-cccc-4ccc-8ccc-cccccccccccc")];
    await renderLibrary({
      items: [],

      visit: { filters: { series, shelf, status: { any: ["unread"], none: [] } }, view: "table" },
      preferences: preferencesFixture({ sort_stack: "title" }),
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
      // REV-REQ-0002: a condition edit and section Clear preserve sibling sections, and the books
      // request filters by exactly the conditions that restrict results.
      const first = encodeGraphQLId("Series", "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
      const second = encodeGraphQLId("Series", "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
      const shelfId = encodeGraphQLId("Shelf", "cccccccc-cccc-4ccc-8ccc-cccccccccccc");
      const shelf = [shelfId];
      const fixture = await renderLibrary({
        items: [bookFixture()],

        visit: {
          filters: {
            series: [first, second],
            shelf,
            status: { any: ["unread"], none: [] },
            title: { op: "contains", text: "   " },
          },
        },
        seriesLabels: [
          { id: first, value: "Discworld" },
          { id: second, value: "Culture" },
        ],
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
        const request = fixture.calls.filter((call) => call.name === "LibraryPageQuery").at(-1);
        expect(request?.variables.filter).toEqual({
          seriesAny: [second],
          shelfAny: [shelfId],
          statusAny: ["unread"],
        });
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

        visit: { filters: { status: { any: ["unread"], none: [] }, pages: { gte: 500 } } },
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
    const SORTED_BOOKS = [
      bookFixture({
        id: "41111111-1111-4111-8111-111111111111",
        title: "Short",
        pages: 90,
        created_at: "2026-01-03T00:00:00Z",
      }),
      bookFixture({
        id: "42222222-2222-4222-8222-222222222222",
        title: "Long",
        pages: 900,
        created_at: "2026-01-01T00:00:00Z",
      }),
      bookFixture({
        id: "43333333-3333-4333-8333-333333333333",
        title: "Middle",
        pages: 300,
        created_at: "2026-01-02T00:00:00Z",
      }),
    ];
    test.each(
      [
        {
          resolved: "the stored sort",
          preferences: preferencesFixture({ sort_stack: "-pages" }),
          announced: "Sorted by Pages descending",
          sort: [{ column: "PAGES", direction: "DESC" }],
          order: ["Long", "Middle", "Short"],
        },
        {
          resolved: "the installation order",
          preferences: preferencesFixture(),
          announced: "Sorted by Added descending",
          sort: null,
          order: ["Short", "Middle", "Long"],
        },
      ].flatMap((sorted) => (["grid", "table"] as const).map((view) => ({ ...sorted, view }))),
    )(
      "a stale URL ?sort= is inert: $resolved still applies in $view",
      async ({ preferences, announced, sort, order, view }) => {
        // REV-REQ-0001: a URL `sort` value leaves the stored or installation order, and Covers and
        // Table order the same books by the same sort stack.
        const fixture = await renderLibrary({
          items: SORTED_BOOKS,
          preferences,
          visit: { view },
          url: "/library?sort=title",
        });
        await expect.element(page.getByText(announced)).toBeInTheDocument();
        await vi.waitFor(() => {
          const shown = [...document.querySelectorAll<HTMLElement>("[data-book-id]")].map(
            (row) =>
              SORTED_BOOKS.find(
                (book) => encodeGraphQLId("Manifestation", book.id) === row.dataset.bookId,
              )?.title,
          );
          expect(shown).toEqual(order);
        });
        const requests = fixture.calls.filter((call) => call.name === "LibraryPageQuery");
        expect(requests.at(0)?.variables.sort ?? null).toEqual(sort);
      },
    );

    test("the rail's reset drops the override and returns to the installation order", async () => {
      // REV-REQ-0001: the Covers sort control and each sort stack editor action store their result
      // as the override, the reset clears it, and none of them adds a `sort` parameter to the URL.
      const fixture = await renderLibrary({
        items: SORTED_BOOKS,
        preferences: preferencesFixture({ sort_stack: "-pages" }),
      });
      await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
      const user = userEvent;
      const writes: unknown[] = [];
      async function expectWrite(sort: unknown): Promise<void> {
        writes.push({ sort });
        await vi.waitFor(() => {
          expect(preferenceWrites(fixture)).toEqual(writes);
        });
        expect(page.getByTestId("router-search").element().textContent).not.toContain("sort=");
        expect(window.location.search).not.toContain("sort=");
      }
      await user.click(page.getByRole("button", { name: /Sort books$/ }));
      await user.click(page.getByRole("option", { name: "Title, ascending" }));
      await expectWrite([{ column: "TITLE", direction: "ASC" }]);
      await user.click(page.getByRole("button", { name: /^Filters/ }));
      await expect.element(page.getByRole("dialog")).toBeInTheDocument();
      await user.click(page.getByRole("button", { name: /Add sort field/ }));
      await user.click(page.getByRole("option", { name: "Pages" }));
      await expectWrite([
        { column: "TITLE", direction: "ASC" },
        { column: "PAGES", direction: "ASC" },
      ]);
      await user.click(page.getByRole("button", { name: "Move Pages earlier in sort priority" }));
      await expectWrite([
        { column: "PAGES", direction: "ASC" },
        { column: "TITLE", direction: "ASC" },
      ]);
      await user.click(page.getByRole("button", { name: /Change Pages sort direction/ }));
      await expectWrite([
        { column: "PAGES", direction: "DESC" },
        { column: "TITLE", direction: "ASC" },
      ]);
      await user.click(page.getByRole("button", { name: "Remove Title from sort" }));
      await expectWrite([{ column: "PAGES", direction: "DESC" }]);
      await user.click(page.getByRole("button", { name: "Reset sort to the installation order" }));
      await expectWrite(null);
      await expect.element(page.getByText("Sorted by Added descending")).toBeInTheDocument();
      await vi.waitFor(() => {
        const request = fixture.calls.filter((call) => call.name === "LibraryPageQuery").at(-1);
        expect(request?.variables.sort ?? null).toBeNull();
        const shown = [...document.querySelectorAll<HTMLElement>("[data-book-id]")].map(
          (row) =>
            SORTED_BOOKS.find(
              (book) => encodeGraphQLId("Manifestation", book.id) === row.dataset.bookId,
            )?.title,
        );
        expect(shown).toEqual(["Short", "Middle", "Long"]);
      });
    });
  });
});

// REV-REQ-0080: a newer filter intent replaces the previous displayed rows, including an empty result.
// Data loss boundary: after the row disappears, Undo retries the same saved operation for the same book.
test("metadata Undo keeps its exact target after the row disappears", async () => {
  const original = bookFixture({ pages: 100 });
  const saved = { ...original, title: "Saved title", metadata_etag: '"cccccccccccccccccccccc"' };
  const latest = { ...saved, pages: 250, metadata_etag: '"dddddddddddddddddddddd"' };
  const restored = { ...latest, title: original.title, metadata_etag: '"eeeeeeeeeeeeeeeeeeeeee"' };
  const operation = {
    operationId: "33333333-3333-4333-8333-333333333333",
    fieldName: "title",
    expiresAt: new Date(Date.now() + 3600000).toISOString(),
    status: "ELIGIBLE",
  };
  const undoMutation = "useCellEditUndoMutation";
  const resolveQuery = "useCellEditUndoEligibilityQuery";
  const fixture = await renderLibrary({
    items: [original],
    visit: { view: "table" },
    handler: (name, variables, owner) => {
      if (name === "useCellEditSaveMutation") {
        owner.books = [saved];
        return savedCells(metadataVariables.parse(variables).requestId, saved, [operation]);
      }
      if (name === resolveQuery) {
        return response({
          metadataOperation: {
            outcome: "SAVED",
            metadata: metadata(latest),
            operations: [operation],
          },
        });
      }
      if (name === undoMutation) {
        if (variables.baseline !== latest.metadata_etag)
          return refused("undoMetadata", "StaleBaseline", "if-match-mismatch");
        owner.books = [restored];
        return restoredCells(restored, { ...operation, status: "CONSUMED" });
      }
    },
  });
  await editTitle(original.title, saved.title);
  const undo = page.getByRole("button", {
    name: `Undo Title for “${original.title}”`,
    exact: true,
  });
  await expect.element(undo).toBeEnabled();
  fixture.books = [latest];
  await userEvent.fill(page.getByRole("searchbox"), "missing");
  await expect.element(findLibraryTable()).not.toBeInTheDocument();
  await userEvent.click(undo);
  await expect
    .element(page.getByText(`Title for “${original.title}” restored.`, { exact: true }))
    .toBeInTheDocument();

  const target = {
    id: encodeGraphQLId("Manifestation", original.id),
    operationId: operation.operationId,
  };
  expect(
    fixture.calls.filter(({ name }) => name === undoMutation || name === resolveQuery),
  ).toEqual([
    { name: undoMutation, variables: { ...target, baseline: saved.metadata_etag } },
    { name: resolveQuery, variables: target },
    { name: undoMutation, variables: { ...target, baseline: latest.metadata_etag } },
  ]);
});

// REV-REQ-0019 and the data loss boundary: an open editor retains its original matched write precondition.
test("a table edit keeps the validator its editor opened with", async () => {
  const opened = bookFixture();
  const pending = new Promise<Response>(() => {});
  const fixture = await renderLibrary({
    items: [opened],
    visit: { view: "table" },
    preferences: preferencesFixture({ hidden_columns: [] }),
    handler: (name) => (name === "useCellEditSaveMutation" ? pending : undefined),
  });
  await userEvent.click(page.getByRole("button", { name: "Edit title of The Brothers Karamazov" }));
  await page.getByRole("dialog").getByRole("textbox").fill("Draft title");
  fixture.books = [
    { ...opened, subtitle: "Edited elsewhere", metadata_etag: '"cccccccccccccccccccccc"' },
  ];
  visitOf(fixture).refresh();
  await expect.element(page.getByText("Edited elsewhere")).toBeInTheDocument();
  await userEvent.keyboard("{Enter}");
  await vi.waitFor(() => {
    expect(
      fixture.calls
        .filter((call) => call.name === "useCellEditSaveMutation")
        .map((call) => call.variables.baseline),
    ).toEqual([opened.metadata_etag]);
  });
});

// REV-REQ-0081, REV-REQ-0019 and the data loss boundary: Retry retains each attempt and classifies each target.
it("Library Retry sends the retained edit again", async () => {
  const original = bookFixture();
  let current = original;
  let mode:
    | "temporary"
    | "lost"
    | "conflict"
    | "validation"
    | "success"
    | "locked"
    | "permission"
    | "reading" = "temporary";
  const writes: z.infer<typeof metadataVariables>[] = [];
  const readingWrites: z.infer<typeof readingVariables>[] = [];
  let reloads = 0;
  let rejectDelayed: ((error: Error) => void) | undefined;
  const delayed = new Promise<Response>((_resolve, reject) => {
    rejectDelayed = reject;
  });
  const fixture = await renderLibrary({
    visit: { view: "table" },
    handler: (name, variables, owner) => {
      if (name === "useCellEditSaveMutation") {
        const write = metadataVariables.parse(variables);
        writes.push(write);
        if (mode === "temporary") return Promise.reject(new TypeError("Offline"));
        if (mode === "lost") return delayed;
        if (mode === "conflict")
          return refused("updateMetadata", "StaleBaseline", "if-match-mismatch");
        if (mode === "validation")
          return refused("updateMetadata", "MetadataInvalid", "validation", {
            fieldErrors: [{ __typename: "FieldError", field: "isbn_13", message: "Invalid ISBN" }],
          });
        if (mode === "permission") return refusal("forbidden");
        if (mode === "locked")
          return refused("updateMetadata", "FieldsLocked", "field-locked", { fields: ["isbn_13"] });
        current = {
          ...current,
          title: z.string().optional().parse(write.patch.title) ?? current.title,
          isbn_13: z.string().optional().parse(write.patch.isbn13) ?? current.isbn_13,
          metadata_etag: `"${"d".repeat(20)}${String(writes.length).padStart(2, "0")}"`,
        };
        owner.books = [current];
        return savedCells(write.requestId, current, []);
      }
      if (name === "useCellEditReloadQuery") {
        const manifestation = graphBook(current);
        if (++reloads === 1)
          return new Response(
            JSON.stringify({ data: { manifestation }, errors: [{ message: "Reload failed" }] }),
            { headers: { "Content-Type": "application/json" } },
          );
        return response({ manifestation });
      }
      if (name === "useCellEditReadingMutation") {
        const write = readingVariables.parse(variables);
        readingWrites.push(write);
        return mode === "reading"
          ? Promise.reject(new TypeError("Offline"))
          : refused("updateReadingState", "StaleBaseline", "The baseline is stale.");
      }
    },
  });
  await editTitle(original.title, "Retained title");
  const outcomes = page.getByRole("region", { name: "Edit outcomes" });
  await expect
    .element(outcomes.getByRole("button", { name: "Retry", exact: true }))
    .toBeInTheDocument();
  expect(writes).toHaveLength(1);
  mode = "lost";
  await userEvent.click(outcomes.getByRole("button", { name: "Retry", exact: true }));
  await expect
    .element(page.getByRole("button", { name: "Retrying…", exact: true }).first())
    .toBeInTheDocument();
  rejectDelayed?.(new TypeError("Lost acknowledgement"));
  await expect.element(outcomes.getByRole("button", { name: "Retry", exact: true })).toBeEnabled();
  mode = "conflict";
  await userEvent.click(outcomes.getByRole("button", { name: "Retry", exact: true }));
  await expect.element(outcomes.getByRole("button", { name: "Reload", exact: true })).toBeEnabled();
  expect(writes).toHaveLength(3);
  expect(writes[1]).toEqual(writes[0]);
  expect(writes[2]).toEqual(writes[0]);
  current = { ...current, title: "Fresh canonical", metadata_etag: '"eeeeeeeeeeeeeeeeeeeeee"' };
  fixture.books = [current];
  await expect
    .element(outcomes.getByRole("button", { name: "Reload", exact: true }))
    .toBeInTheDocument();
  expect(outcomes.getByRole("button", { name: "Retry", exact: true }).elements()).toHaveLength(0);
  const beforeReload = writes.length;
  const reload = outcomes.getByRole("button", { name: "Reload", exact: true });
  await userEvent.click(reload);
  await expect.element(reload).not.toHaveAttribute("data-pending");
  expect(reloads).toBe(1);
  await expect.element(outcomes.getByText("Submitted value: Retained title")).toBeInTheDocument();
  await expect.element(page.getByRole("link", { name: original.title, exact: true })).toBeVisible();
  expect(writes).toHaveLength(beforeReload);
  await userEvent.click(reload);
  await expect.element(reload).not.toHaveAttribute("data-pending");
  expect(reloads).toBe(2);
  expect(writes).toHaveLength(beforeReload);
  mode = "success";
  await userEvent.click(page.getByRole("button", { name: `Edit title of ${current.title}` }));
  await page.getByRole("dialog").getByRole("textbox").fill("Corrected title");
  await userEvent.keyboard("{Enter}");
  await expect.element(page.getByText("Corrected title").first()).toBeInTheDocument();
  expect(writes.at(-1)?.baseline).toBe('"eeeeeeeeeeeeeeeeeeeeee"');
  expect(writes.at(-1)?.requestId).not.toBe(writes.at(-2)?.requestId);
  mode = "validation";
  await userEvent.click(
    page.getByRole("button", { name: "9780374528379, edit ISBN", exact: true }),
  );
  await page.getByRole("dialog").getByRole("textbox").fill("bad");
  await userEvent.keyboard("{Enter}");
  await expect.element(outcomes.getByText("Submitted value: bad")).toBeInTheDocument();
  await expect
    .element(page.getByRole("dialog", { name: "Edit ISBN", includeHidden: true }))
    .not.toBeInTheDocument();
  page
    .getByRole("button", { name: "9780374528379, edit ISBN, not saved", exact: true })
    .element()
    .focus();
  await userEvent.keyboard("{F2}");
  const isbnInput = page.getByRole("dialog", { name: "Edit ISBN" }).getByRole("textbox");
  await expect.element(isbnInput).toHaveValue("bad");
  const invalidRequest = writes.at(-1);
  mode = "success";
  await isbnInput.fill("9780140449242");
  await userEvent.keyboard("{Enter}");
  await expect
    .element(page.getByRole("button", { name: "9780140449242, edit ISBN", exact: true }))
    .toBeInTheDocument();
  expect(writes.at(-1)?.baseline).toBe(invalidRequest?.baseline);
  expect(writes.at(-1)?.requestId).not.toBe(invalidRequest?.requestId);
  mode = "locked";
  await editTitle(current.title, "Locked title");
  await expect.element(outcomes.getByText(/field-locked/)).toBeInTheDocument();
  expect(outcomes.getByRole("button", { name: /Retry|Reload/ }).elements()).toHaveLength(0);
  mode = "permission";
  await editTitle(current.title, "Forbidden title");
  await expect.element(outcomes.getByText("Submitted value: Forbidden title")).toBeInTheDocument();
  await expect.element(outcomes.getByText(/forbidden/)).toBeInTheDocument();
  expect(outcomes.getByRole("button", { name: /Retry|Reload/ }).elements()).toHaveLength(0);
  mode = "reading";
  await userEvent.click(page.getByRole("button", { name: "Add rating", exact: true }), {
    position: { x: 1, y: 1 },
  });
  const ratingEditor = page.getByRole("dialog", { name: "Rating", exact: true });
  await expect.element(ratingEditor).toBeVisible();
  await userEvent.click(ratingEditor.getByRole("radio", { name: "5 stars", exact: true }));
  await expect
    .element(outcomes.getByRole("button", { name: "Retry", exact: true }))
    .toBeInTheDocument();
  current = { ...current, reading_etag: '"ffffffffffffffffffffff"' };
  fixture.books = [current];
  mode = "conflict";
  await userEvent.click(outcomes.getByRole("button", { name: "Retry", exact: true }));
  await expect
    .element(outcomes.getByRole("button", { name: "Reload", exact: true }))
    .toBeInTheDocument();
  expect(readingWrites).toHaveLength(2);
  expect(readingWrites[1]).toEqual(readingWrites[0]);
  expect(readingWrites[0]?.baseline).toBe(original.reading_etag);
  await expect.element(outcomes.getByText("Submitted value: 5")).toBeInTheDocument();
  await cleanup();
  setRelayAccount(null);
  vi.restoreAllMocks();
  setRelayAccount("batch-browser");
  const second = bookFixture({
    id: "55555555-5555-4555-8555-555555555555",
    work_id: "88888888-8888-4888-8888-888888888888",
    title: "Temporary",
    reading_etag: '"cccccccccccccccccccccc"',
  });
  const third = bookFixture({
    id: "66666666-6666-4666-8666-666666666666",
    work_id: "99999999-9999-4999-8999-999999999999",
    title: "Conflicted",
    reading_etag: '"dddddddddddddddddddddd"',
  });
  const batchWrites: z.infer<typeof readingVariables>[] = [];
  const books = [original, second, third];
  const baselines = books.map((book) => book.reading_etag);
  await renderLibrary({
    items: books,
    visit: { view: "table" },
    handler: (name, variables, owner) => {
      if (name === "useCellEditReadingMutation") {
        const write = readingVariables.parse(variables);
        batchWrites.push(write);
        if (write.id === encodeGraphQLId("Manifestation", third.id))
          return refused("updateReadingState", "StaleBaseline", "The baseline is stale.");
        if (
          write.id === encodeGraphQLId("Manifestation", second.id) &&
          batchWrites.filter((item) => item.id === write.id).length === 1
        )
          return refusal("internal");
        const index = books.findIndex(
          (book) => encodeGraphQLId("Manifestation", book.id) === write.id,
        );
        const target = books.at(index) ?? original;
        const updated = {
          ...target,
          reading_etag: '"eeeeeeeeeeeeeeeeeeeeee"',
          reading_state: {
            status: "reading" as const,
            rating: null,
            progress_pct: null,
            started_at: null,
            finished_at: null,
          },
        };
        books[index] = updated;
        owner.books = [...books];
        return response({
          updateReadingState: {
            __typename: "ReadingStateUpdated",
            manifestation: graphBook(updated),
          },
        });
      }
    },
  });
  await expect.element(findLibraryTable()).toBeInTheDocument();
  page.getByRole("checkbox", { name: "Select all loaded books" }).element().focus();
  await userEvent.keyboard(" ");
  await userEvent.click(page.getByRole("button", { name: "Set status", exact: true }));
  await userEvent.click(page.getByRole("menuitem", { name: "Reading", exact: true }));
  await expect
    .element(page.getByRole("button", { name: "Retry failed books", exact: true }))
    .toBeInTheDocument();
  expect(batchWrites.map((write) => write.baseline)).toEqual(baselines);
  await expect.element(page.getByRole("checkbox", { name: /^Select Temporary/ })).toBeChecked();
  await expect.element(page.getByRole("checkbox", { name: /^Select Conflicted/ })).toBeChecked();
  await userEvent.click(page.getByRole("button", { name: "Retry failed books", exact: true }));
  await vi.waitFor(() => {
    expect(batchWrites).toHaveLength(4);
  });
  expect(batchWrites[3]).toEqual(batchWrites[1]);
  await expect
    .element(page.getByRole("group", { name: "Batch actions" }).getByRole("status"))
    .toHaveTextContent("Set status for 2 of 3 books. 1 could not be saved.");
  await expect.element(page.getByRole("checkbox", { name: /^Select Conflicted/ })).toBeChecked();
  await expect.element(page.getByText("Submitted value: reading").first()).toBeInTheDocument();
}, 60000);
