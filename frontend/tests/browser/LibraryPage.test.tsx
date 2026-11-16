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
import type { BookListItem, Preferences } from "@/api";
import { encodeGraphQLId } from "@/api/graphql";
import { relayEnvironment, setRelayAccount } from "@/lib/relay/environment";
import { libraryVisit } from "@/lib/relay/library-visit";
import { queryKeys } from "@/lib/query/keys";
import trustedDocuments from "../../../backend/graphql-documents.json";
import { LibraryPage } from "@/pages/library/LibraryPage";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ preference: "system", effective: "dark", setPreference: vi.fn() }),
}));
const originalState: unknown = navigation.currentEntry?.getState();
const originalURL = window.location.href;
const documents: Record<string, string> = trustedDocuments;
const wireRequest = z.object({ id: z.string(), variables: z.record(z.string(), z.unknown()) });
const listVariables = z.object({
  sourceKey: z.string(),
  filter: z.record(z.string(), z.unknown()).nullable().optional(),
  sort: z.array(z.object({ column: z.string(), direction: z.string() })).optional(),
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
const pageInfo = { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null };
function metadata(book: BookListItem) {
  return {
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
    etag: book.reading_etag,
    status: book.reading_state?.status ?? null,
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
function graphPreferences(preferences: Preferences) {
  return {
    hiddenColumns: preferences.hidden_columns,
    density: preferences.density,
    view: preferences.view,
    sortStack: preferences.sort_stack,
    defaults: {
      hiddenColumns: preferences.defaults.hidden_columns,
      density: preferences.defaults.density,
      view: preferences.defaults.view,
      sortStack: preferences.defaults.sort_stack,
    },
  };
}
type Fixture = {
  books: BookListItem[];
  preferences: Preferences;
  calls: { name: string; variables: Record<string, unknown> }[];
  environment: ReturnType<typeof relayEnvironment>;
};
type RenderOptions = {
  items?: BookListItem[];
  visit?: { filters?: Partial<FilterState>; view?: LibraryVisit["view"] };
  preferences?: Preferences;
  url?: string;
  seriesLabels?: { id: string; value: string }[];
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
  handler,
}: RenderOptions = {}) {
  const fixture: Fixture = {
    books: items,
    preferences,
    calls: [],
    environment: relayEnvironment(),
  };
  vi.spyOn(globalThis, "fetch").mockImplementation((_input, init) => {
    const request = wireRequest.parse(JSON.parse(z.string().parse(init?.body)));
    const name = documents[request.id]?.match(/(?:query|mutation) (\w+)/)?.[1];
    if (name === undefined) throw new Error("Unknown trusted operation");
    fixture.calls.push({ name, variables: request.variables });
    const overridden = handler?.(name, request.variables, fixture);
    if (overridden !== undefined) return Promise.resolve(overridden);
    if (name === "readerLibraryQuery") {
      const variables = listVariables.parse(request.variables);
      const q = z.string().optional().parse(variables.filter?.q) ?? "";
      const rows = fixture.books.filter((book) =>
        book.title.toLowerCase().includes(q.toLowerCase()),
      );
      return Promise.resolve(
        response({
          books: {
            edges: rows.map((book) => ({ cursor: book.id, node: graphBook(book) })),
            pageInfo,
          },
        }),
      );
    }
    if (name === "readerShelvesQuery")
      return Promise.resolve(response({ shelves: { edges: [], pageInfo } }));
    if (name === "readerSeriesLabelsQuery")
      return Promise.resolve(
        response({
          seriesLabels: seriesLabels.map((label) => ({
            entityId: encodeGraphQLId("Series", label.id),
            value: label.value,
          })),
        }),
      );
    if (name === "readerAuthorLabelsQuery") return Promise.resolve(response({ authorLabels: [] }));
    if (name === "readerLibraryPreferencesQuery")
      return Promise.resolve(
        response({ libraryPreferences: graphPreferences(fixture.preferences) }),
      );
    if (name === "mutationsUpdateLibraryPreferencesMutation") {
      const patch = z
        .object({
          hiddenColumns: z.array(z.string()).nullable().optional(),
          density: z.enum(["COMPACT", "COMFORTABLE"]).nullable().optional(),
          view: z.enum(["TABLE", "GRID"]).nullable().optional(),
          sortStack: z.string().nullable().optional(),
        })
        .parse(request.variables.patch);
      fixture.preferences = {
        ...fixture.preferences,
        ...("sortStack" in patch ? { sort_stack: patch.sortStack ?? null } : {}),
        ...("hiddenColumns" in patch ? { hidden_columns: patch.hiddenColumns ?? null } : {}),
        ...("density" in patch
          ? { density: patch.density === "COMPACT" ? "compact" : "comfortable" }
          : {}),
        ...("view" in patch ? { view: patch.view === "TABLE" ? "table" : "grid" } : {}),
      };
      return Promise.resolve(
        response({ updateLibraryPreferences: graphPreferences(fixture.preferences) }),
      );
    }
    throw new Error(`Unprovided operation ${name}`);
  });
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.auth.me(), {
    id: "77777777-7777-4777-8777-777777777777",
    display_name: "Reader",
    email: null,
    role: "adult",
    is_child: false,
    has_local_password: true,
    theme_preference: "dark",
  });
  history.replaceState(navigation.currentEntry?.getState() ?? null, "", url);
  navigation.updateCurrentEntry({
    state: {
      reverieLibrary: { filters: { ...emptyFilterState(), ...visit?.filters }, view: visit?.view },
    },
  });
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
    .map((call) => call.variables.patch);
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
    const fixture = await renderLibrary({
      items: [bookFixture()],
      visit: { view: "table" },
    });
    await expect.element(findLibraryTable()).toBeInTheDocument();
    const user = userEvent;
    await user.click(page.getByRole("columnheader", { name: /^Title(?:\s|$)/ }));
    await vi.waitFor(() => {
      expect(preferenceWrites(fixture)).toEqual([{ sortStack: "title" }]);
    });
    expect(page.getByTestId("router-search").element().textContent).not.toContain("sort=");
    expect(window.location.search).not.toContain("sort=");
  });

  test("clearing all filters preserves the visit view and the stored sort", async () => {
    // REV-REQ-0002: Clear removes every condition and preserves view and sort.
    const series = ["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"];
    const shelf = ["cccccccc-cccc-4ccc-8ccc-cccccccccccc"];
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
      // REV-REQ-0002: a condition edit and section Clear preserve sibling sections.
      const first = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
      const second = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
      const shelfId = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
      const shelf = [shelfId];
      await renderLibrary({
        items: [bookFixture()],

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
    test.each([
      {
        resolved: "the stored sort",
        preferences: preferencesFixture({ sort_stack: "-pages" }),

        announced: "Sorted by Pages descending",
      },
      {
        resolved: "the installation order",
        preferences: preferencesFixture(),

        announced: "Sorted by Added descending",
      },
    ])(
      "a stale URL ?sort= is inert: $resolved still applies",
      async ({ preferences, announced }) => {
        const fixture = await renderLibrary({
          items: [bookFixture()],

          preferences,
          url: "/library?sort=title",
        });
        await expect.element(page.getByText(announced)).toBeInTheDocument();
        await expect.element(page.getByText("The Brothers Karamazov").first()).toBeInTheDocument();
        await vi.waitFor(() => {
          const request = fixture.calls.filter((call) => call.name === "readerLibraryQuery").at(-1);
          expect(request?.variables.sort).toEqual(
            preferences.sort_stack === null ? undefined : [{ column: "pages", direction: "DESC" }],
          );
        });
      },
    );

    test("the rail's reset drops the override and returns to the installation order", async () => {
      const fixture = await renderLibrary({
        items: [bookFixture()],
        preferences: preferencesFixture({ sort_stack: "-pages" }),
      });
      await expect.element(page.getByRole("heading", { name: "Library" })).toBeInTheDocument();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: /^Filters/ }));
      await expect.element(page.getByRole("dialog")).toBeInTheDocument();
      await user.click(page.getByRole("button", { name: "Reset sort to the installation order" }));
      await vi.waitFor(() => {
        expect(preferenceWrites(fixture)).toEqual([{ sortStack: null }]);
      });

      await expect.element(page.getByText("Sorted by Added descending")).toBeInTheDocument();
    });

    test("a late preferences response re-sorts the initial list once", async () => {
      let finish: ((value: Response) => void) | undefined;
      const pending = new Promise<Response>((resolve) => {
        finish = resolve;
      });
      const fixture = await renderLibrary({
        items: [bookFixture({ title: "The Seeded Row" })],
        handler: (name, variables) => {
          if (name === "readerLibraryPreferencesQuery")
            return pending.then((value) => value.clone());
          if (name === "readerLibraryQuery" && variables.sort !== undefined)
            return response({
              books: {
                edges: [
                  { cursor: "sorted", node: graphBook(bookFixture({ title: "A Sorted Row" })) },
                ],
                pageInfo,
              },
            });
        },
      });
      await expect.element(page.getByText("The Seeded Row")).toBeInTheDocument();
      finish?.(
        response({
          libraryPreferences: graphPreferences(preferencesFixture({ sort_stack: "-pages" })),
        }),
      );
      await expect.element(page.getByText("A Sorted Row")).toBeInTheDocument();
      const requests = fixture.calls.filter((call) => call.name === "readerLibraryQuery");
      expect(requests).toHaveLength(3);
      expect(requests.at(0)?.variables).not.toHaveProperty("sort");
      expect(requests.at(1)?.variables).not.toHaveProperty("sort");
      expect(requests.at(2)?.variables.sort).toEqual([{ column: "pages", direction: "DESC" }]);
    });
  });
});

// REV-REQ-0080: a newer filter intent replaces the previous displayed rows, including an empty result.
test("a successful empty response replaces placeholders", async () => {
  let finishPending: ((value: Response) => void) | undefined;
  const pending = new Promise<Response>((resolve) => {
    finishPending = resolve;
  });
  await renderLibrary({
    items: [bookFixture({ title: "Origin" })],
    handler: (name, variables) => {
      if (
        name === "readerLibraryQuery" &&
        z.object({ q: z.string().optional() }).parse(variables.filter).q === "missing"
      )
        return pending.then((value) => value.clone());
    },
  });
  await expect.element(page.getByText("Origin")).toBeInTheDocument();
  await userEvent.fill(page.getByRole("searchbox"), "missing");
  await expect.element(page.getByText("Updating results…", { exact: true })).toBeInTheDocument();
  await userEvent.click(page.getByRole("button", { name: "Clear search", exact: true }));
  await expect
    .element(page.getByText("Updating results…", { exact: true }))
    .not.toBeInTheDocument();
  await userEvent.fill(page.getByRole("searchbox"), "missing");
  await expect.element(page.getByText("Origin")).toBeInTheDocument();
  finishPending?.(response({ books: { edges: [], pageInfo } }));
  await expect.element(page.getByText("No books match these filters")).toBeInTheDocument();
  await expect.element(page.getByText("Origin")).not.toBeInTheDocument();
});

// Data loss boundary: exact operations, retained source routing, acknowledgement recovery and native input Undo.
test("metadata Undo survives table refreshes", async () => {
  let current = bookFixture({ pages: 100 });
  const titleOperation = {
    operationId: "33333333-3333-4333-8333-333333333333",
    fieldName: "title",
    expiresAt: new Date(Date.now() + 3600000).toISOString(),
    status: "eligible",
  };
  const pagesOperation = {
    ...titleOperation,
    operationId: "44444444-4444-4444-8444-444444444444",
    fieldName: "pages",
  };
  let finishUndo: ((value: Response) => void) | undefined;
  const pendingUndo = new Promise<Response>((resolve) => {
    finishUndo = resolve;
  });
  let titleUndoCalls = 0;
  let titleRequest = "";
  const fixture = await renderLibrary({
    items: [current],
    visit: { view: "table" },
    handler: (name, variables, owner) => {
      if (name === "mutationsSaveCellsMutation") {
        const write = metadataVariables.parse(variables);
        expect(write.baseline).toBe(current.metadata_etag);
        if ("title" in write.patch) {
          titleRequest = write.requestId;
          current = {
            ...current,
            title: z.string().parse(write.patch.title),
            metadata_etag: '"cccccccccccccccccccccc"',
          };
          owner.books = [current];
          return Promise.reject(new TypeError("Lost acknowledgement"));
        }
        current = {
          ...current,
          pages: z.number().parse(write.patch.pages),
          metadata_etag: '"dddddddddddddddddddddd"',
        };
        owner.books = [current];
        return response({
          updateMetadata: {
            requestId: write.requestId,
            metadata: metadata(current),
            operations: [pagesOperation],
          },
        });
      }
      if (name === "mutationsResolveCellsQuery") {
        if (variables.requestId != null) expect(variables.requestId).toBe(titleRequest);
        else expect(variables.operationId).toBe(titleOperation.operationId);
        return response({
          metadataOperation: {
            outcome: "saved",
            metadata: metadata(current),
            operations: [titleOperation],
          },
        });
      }
      if (name === "mutationsUndoCellsMutation") {
        if (variables.operationId === pagesOperation.operationId) {
          current = { ...current, pages: 100, metadata_etag: '"eeeeeeeeeeeeeeeeeeeeee"' };
          owner.books = [current];
          return response({
            undoMetadata: { outcome: "restored", metadata: metadata(current), operations: [] },
          });
        }
        titleUndoCalls += 1;
        if (titleUndoCalls === 1) return refusal("if-match-mismatch");
        expect(variables.baseline).toBe(current.metadata_etag);
        return pendingUndo;
      }
    },
  });
  await userEvent.click(page.getByRole("button", { name: "Edit title of The Brothers Karamazov" }));
  const editor = page.getByRole("dialog").getByRole("textbox");
  await userEvent.type(editor, "Draft");
  await userEvent.keyboard("{Control>}z{/Control}");
  await expect.element(editor).toHaveValue("The Brothers Karamazov");
  expect(titleRequest).toBe("");
  await editor.fill("Saved title");
  await userEvent.keyboard("{Enter}");
  await expect
    .element(
      page.getByRole("button", { name: "Undo Title for “The Brothers Karamazov”", exact: true }),
    )
    .toBeEnabled();
  await userEvent.click(page.getByRole("button", { name: "100, edit pages", exact: true }));
  await page.getByRole("dialog").getByRole("textbox").fill("250");
  await userEvent.keyboard("{Enter}");
  await userEvent.click(
    page.getByRole("button", { name: "Undo Pages for “Saved title”", exact: true }),
  );
  const undo = page.getByRole("button", {
    name: "Undo Title for “The Brothers Karamazov”",
    exact: true,
  });
  await expect.element(undo).toBeEnabled();
  await userEvent.fill(page.getByRole("searchbox"), "missing");
  await expect.element(findLibraryTable()).not.toBeInTheDocument();
  await userEvent.click(undo);
  await expect.element(undo).toHaveAttribute("data-pending");
  await expect.element(undo).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  expect(titleUndoCalls).toBe(2);
  current = {
    ...current,
    title: "The Brothers Karamazov",
    metadata_etag: '"ffffffffffffffffffffff"',
  };
  fixture.books = [current];
  finishUndo?.(
    response({
      undoMetadata: {
        outcome: "restored",
        metadata: metadata(current),
        operations: [{ ...titleOperation, status: "consumed" }],
      },
    }),
  );
  await expect
    .element(page.getByText(/Title for “The Brothers Karamazov” restored/))
    .toBeInTheDocument();
  await expect
    .element(page.getByRole("button", { name: "Undo last edit", exact: true }))
    .toBeDisabled();
  await expect.element(page.getByRole("button", { name: "Keyboard shortcuts" })).toHaveFocus();
  await userEvent.fill(page.getByRole("searchbox"), "");
  await expect.element(page.getByText("The Brothers Karamazov").first()).toBeInTheDocument();
});

// REV-REQ-0019 and the data loss boundary: an open editor retains its original matched write precondition.
test("a table edit keeps the validator its editor opened with", async () => {
  const opened = bookFixture();
  const pending = new Promise<Response>(() => {});
  const fixture = await renderLibrary({
    items: [opened],
    visit: { view: "table" },
    preferences: preferencesFixture({ hidden_columns: [] }),
    handler: (name) => (name === "mutationsSaveCellsMutation" ? pending : undefined),
  });
  await userEvent.click(page.getByRole("button", { name: "Edit title of The Brothers Karamazov" }));
  await page.getByRole("dialog").getByRole("textbox").fill("Draft title");
  fixture.books = [
    { ...opened, subtitle: "Edited elsewhere", metadata_etag: '"cccccccccccccccccccccc"' },
  ];
  await libraryVisit(fixture.environment).refresh();
  await expect.element(page.getByText("Edited elsewhere")).toBeInTheDocument();
  await userEvent.keyboard("{Enter}");
  await vi.waitFor(() => {
    expect(
      fixture.calls
        .filter((call) => call.name === "mutationsSaveCellsMutation")
        .map((call) => call.variables.baseline),
    ).toEqual([opened.metadata_etag]);
  });
});

// REV-REQ-0081 E1-E4, REV-REQ-0019 and the data loss boundary: Retry retains each attempt and classifies each target.
it("Library Retry sends the retained edit again", async () => {
  const original = bookFixture();
  let current = original;
  let mode: "temporary" | "lost" | "conflict" | "validation" | "success" | "locked" | "reading" =
    "temporary";
  const writes: z.infer<typeof metadataVariables>[] = [];
  const readingWrites: z.infer<typeof readingVariables>[] = [];
  const operation = {
    operationId: "33333333-3333-4333-8333-333333333333",
    fieldName: "title",
    expiresAt: new Date(Date.now() + 3600000).toISOString(),
    status: "eligible",
  };
  let saved = false;
  let rejectDelayed: ((error: Error) => void) | undefined;
  const delayed = new Promise<Response>((_resolve, reject) => {
    rejectDelayed = reject;
  });
  const fixture = await renderLibrary({
    visit: { view: "table" },
    handler: (name, variables, owner) => {
      if (name === "mutationsSaveCellsMutation") {
        const write = metadataVariables.parse(variables);
        writes.push(write);
        if (mode === "temporary") return Promise.reject(new TypeError("Offline"));
        if (mode === "lost") {
          saved = true;
          current = {
            ...current,
            title: "Retained title",
            metadata_etag: '"cccccccccccccccccccccc"',
          };
          owner.books = [current];
          return delayed;
        }
        if (mode === "conflict") return refusal("if-match-mismatch");
        if (mode === "validation") return refusal("validation", { isbn_13: "Invalid ISBN" });
        if (mode === "locked") return refusal("field-locked");
        current = {
          ...current,
          title: z.string().optional().parse(write.patch.title) ?? current.title,
          isbn_13: z.string().optional().parse(write.patch.isbn13) ?? current.isbn_13,
          metadata_etag: '"dddddddddddddddddddddd"',
        };
        owner.books = [current];
        return response({
          updateMetadata: {
            requestId: write.requestId,
            metadata: metadata(current),
            operations: [],
          },
        });
      }
      if (name === "mutationsResolveCellsQuery")
        return response({
          metadataOperation: {
            outcome: saved && mode === "conflict" ? "saved" : "unknown",
            metadata: metadata(current),
            operations: saved ? [operation] : [],
          },
        });
      if (name === "mutationsCellsQuery")
        return response({ manifestation: { id: variables.id, metadata: metadata(current) } });
      if (name === "mutationsUndoCellsMutation")
        return response({
          undoMetadata: {
            outcome: "restored",
            metadata: metadata(original),
            operations: [{ ...operation, status: "consumed" }],
          },
        });
      if (name === "mutationsReadingMutation") {
        const write = readingVariables.parse(variables);
        readingWrites.push(write);
        return mode === "reading"
          ? Promise.reject(new TypeError("Offline"))
          : refusal("if-match-mismatch");
      }
      if (name === "mutationsReadingQuery")
        return response({ manifestation: { id: variables.id, readingState: reading(current) } });
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
  await libraryVisit(fixture.environment).refresh();
  await userEvent.click(outcomes.getByRole("button", { name: "Retry", exact: true }));
  await expect.element(page.getByRole("button", { name: /^Undo Title/ })).toBeEnabled();
  expect(writes).toHaveLength(3);
  expect(writes[1]).toEqual(writes[0]);
  expect(writes[2]).toEqual(writes[0]);
  await userEvent.click(page.getByRole("button", { name: /^Undo Title/ }));
  await expect
    .element(page.getByRole("button", { name: "Undo last edit", exact: true }))
    .toBeDisabled();
  saved = false;
  current = { ...current, title: "Fresh canonical", metadata_etag: '"eeeeeeeeeeeeeeeeeeeeee"' };
  fixture.books = [current];
  await libraryVisit(fixture.environment).refresh();
  await expect.element(page.getByText("Fresh canonical").first()).toBeInTheDocument();
  await editTitle(current.title, "Refused title");
  await expect
    .element(outcomes.getByRole("button", { name: "Reload", exact: true }))
    .toBeInTheDocument();
  expect(outcomes.getByRole("button", { name: "Retry", exact: true }).elements()).toHaveLength(0);
  const beforeReload = writes.length;
  await userEvent.click(outcomes.getByRole("button", { name: "Reload", exact: true }));
  await expect.element(outcomes.getByText("Submitted value: Refused title")).toBeInTheDocument();
  expect(writes).toHaveLength(beforeReload);
  await expect
    .element(outcomes.getByRole("button", { name: "Reload", exact: true }))
    .not.toHaveAttribute("data-pending");
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
  page
    .getByRole("button", { name: "9780374528379, edit ISBN, not saved", exact: true })
    .element()
    .focus();
  await userEvent.keyboard("{F2}");
  await expect.element(page.getByRole("dialog").getByRole("textbox")).toHaveValue("bad");
  const invalidRequest = writes.at(-1);
  mode = "success";
  await page.getByRole("dialog").getByRole("textbox").fill("9780140449242");
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
  mode = "reading";
  await expect
    .element(page.getByText("Updating results…", { exact: true }))
    .not.toBeInTheDocument();
  page.getByRole("button", { name: "Add rating", exact: true }).element().focus();
  await userEvent.keyboard("{F2}");
  await userEvent.click(page.getByRole("radio", { name: "5 stars", exact: true }));
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
      if (name === "mutationsReadingQuery")
        return response({
          manifestation: {
            id: variables.id,
            readingState: reading(
              books.find((book) => encodeGraphQLId("Manifestation", book.id) === variables.id) ??
                original,
            ),
          },
        });
      if (name === "mutationsReadingMutation") {
        const write = readingVariables.parse(variables);
        batchWrites.push(write);
        if (write.id === encodeGraphQLId("Manifestation", third.id))
          return refusal("if-match-mismatch");
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
        return response({ updateReadingState: reading(updated) });
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
