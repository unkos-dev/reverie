import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { z } from "zod";
import type { Preferences } from "../../src/api/preferences";

const documents = z
  .record(z.string(), z.string())
  .parse(
    JSON.parse(
      readFileSync(new URL("../../../backend/graphql-documents.json", import.meta.url), "utf8"),
    ),
  );
const ID_PREFIXES: Record<string, string> = {
  Author: "author",
  Manifestation: "book",
  Series: "series",
  Shelf: "shelf",
  Work: "work",
  MetadataRevision: "metadata_rev",
  User: "user",
};
const graphId = (kind: string, id: string) => {
  let bits = BigInt(`0x${id.replaceAll("-", "")}`);
  let suffix = "";
  for (let index = 0; index < 26; index += 1) {
    suffix = "0123456789abcdefghjkmnpqrstvwxyz".charAt(Number(bits & 31n)) + suffix;
    bits >>= 5n;
  }
  return `${ID_PREFIXES[kind]}_${suffix}`;
};
const revisionIds = new Map<string, string>();
const revisionId = (bookId: string, etag: string) => {
  const key = `${bookId} ${etag}`;
  const created = revisionIds.get(key) ?? graphId("MetadataRevision", crypto.randomUUID());
  revisionIds.set(key, created);
  return created;
};
const variablesSchema = z
  .object({
    sourceKey: z.string(),
    cursor: z.string().nullish(),
    count: z.number().nullish(),
    filter: z
      .object({
        q: z.string().nullish(),
        titleContains: z.string().nullish(),
        titleEq: z.string().nullish(),
        pagesGte: z.number().nullish(),
        pagesLte: z.number().nullish(),
        shelfAny: z.array(z.string()).optional(),
        seriesAny: z.array(z.string()).optional(),
      })
      .loose()
      .nullish(),
  })
  .loose();

const BOOK = {
  metadata_etag: '"aaaaaaaaaaaaaaaaaaaaaa"',
  reading_etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
  id: "11111111-1111-4111-8111-111111111111",
  work_id: "22222222-2222-4222-8222-222222222222",
  title: "The Sea",
  subtitle: null,
  authors: ["A Reader"],
  contributors: [],
  series: null,
  isbn_13: null,
  pages: 200,
  tags: [],
  genres: [],
  moods: [],
  content_rating: null,
  cover_url: "",
  ingestion_status: "complete",
  validation_status: "clean",
  enrichment_status: "complete",
  reading_state: null,
  created_at: "2026-10-25T04:00:00Z",
};
const RETURN_BOOKS = Array.from({ length: 120 }, (_, index) => ({
  ...BOOK,
  id: `00000000-0000-4000-8000-${String(index + 1).padStart(12, "0")}`,
  work_id: `10000000-0000-4000-8000-${String(index + 1).padStart(12, "0")}`,
  title: `Journey book ${String(index + 1).padStart(3, "0")}`,
}));
const PREFERENCES: Preferences = {
  hidden_columns: null,
  density: null,
  view: null,
  sort_stack: null,
  defaults: { hidden_columns: [], density: "comfortable", view: "grid", sort_stack: "-created_at" },
};
const SERIES = [
  { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", value: "Sea adventures" },
  { id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", value: "Sea stories" },
];
const SHELVES = [
  { id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc", name: "Favourites" },
  { id: "dddddddd-dddd-4ddd-8ddd-dddddddddddd", name: "Read next" },
].map((shelf) => ({
  ...shelf,
  is_system: false,
  created_at: BOOK.created_at,
  updated_at: BOOK.created_at,
  item_count: 1,
}));

function graphBook(book: typeof BOOK) {
  return {
    __typename: "Manifestation",
    id: graphId("Manifestation", book.id),
    coverUrl: "",
    isbn13: null,
    pages: book.pages,
    createdAt: book.created_at,
    work: {
      id: graphId("Work", book.work_id),
      title: book.title,
      subtitle: null,
      description: null,
      language: "en",
      authors: {
        edges: [
          {
            cursor: "author",
            node: {
              id: graphId("Author", "44444444-4444-4444-8444-444444444444"),
              name: "A Reader",
            },
          },
        ],
        pageInfo: { hasNextPage: false, endCursor: null },
      },
      selectedSeries: null,
    },
    metadata: {
      id: revisionId(book.id, book.metadata_etag),
      manifestationId: graphId("Manifestation", book.id),
      workId: graphId("Work", book.work_id),
      etag: book.metadata_etag,
      title: book.title,
      subtitle: null,
      isbn13: null,
      pages: book.pages,
    },
    readingState: {
      etag: book.reading_etag,
      status: null,
      rating: null,
      progressPct: null,
      startedAt: null,
      finishedAt: null,
    },
    tags: { edges: [], pageInfo: { hasNextPage: false, endCursor: null } },
    versions: { pendingCount: 0 },
    isbn10: null,
    isbnMismatch: false,
    publisher: null,
    publicationDate: null,
    ingestionStatus: "complete",
    validationStatus: "clean",
    enrichmentStatus: "complete",
  };
}

type Fixture = {
  requests: z.infer<typeof variablesSchema>[];
  books: (typeof BOOK)[];
  bootstrap: "healthy" | "failing";
  invisible: Set<string>;
};
const lookupIds = z.array(z.string()).catch([]);
async function intercept(
  page: Page,
  withFacets = false,
  options: { books?: (typeof BOOK)[]; view?: "table" } = {},
): Promise<Fixture> {
  const fixture: Fixture = {
    requests: [],
    books: [...(options.books ?? [BOOK])],
    bootstrap: "healthy",
    invisible: new Set(),
  };
  let preferences = structuredClone(PREFERENCES);
  let libraryPreferences: Record<string, unknown> = {
    hiddenColumns: null,
    density: null,
    view: options.view === undefined ? null : "TABLE",
    sort: null,
    defaults: {
      hiddenColumns: ["SUBTITLE"],
      density: "COMFORTABLE",
      view: "GRID",
      sort: [{ column: "CREATED_AT", direction: "DESC" }],
    },
  };
  const viewer = () => ({
    id: graphId("User", "33333333-3333-4333-8333-333333333333"),
    libraryPreferences,
  });
  const names = new Map([
    ...SERIES.map((item) => [graphId("Series", item.id), item.value] as const),
    ...SHELVES.map((item) => [graphId("Shelf", item.id), item.name] as const),
  ]);
  const visible = (ids: unknown) => lookupIds.parse(ids).filter((id) => !fixture.invisible.has(id));
  const labels = (variables: Record<string, unknown>) => ({
    authorLabels: visible(variables.authorIds).map((id) => ({ id, name: "A Reader" })),
    seriesLabels: visible(variables.seriesIds).map((id) => ({ id, name: names.get(id) ?? "" })),
    shelfLabels: visible(variables.shelfIds).map((id) => ({
      id,
      name: names.get(id) ?? "",
      itemCount: 1,
    })),
  });
  const shelves = () => ({
    edges: (withFacets ? SHELVES : []).map((shelf) => ({
      cursor: shelf.id,
      node: { id: graphId("Shelf", shelf.id), name: shelf.name, isSystem: false, itemCount: 1 },
    })),
    pageInfo: { endCursor: null, hasNextPage: false },
  });
  await page.route(/^https?:\/\/[^/]+\/(?:api|auth)\//, async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (path === "/api/graphql") {
      const body = z
        .object({ id: z.string(), variables: z.record(z.string(), z.unknown()) })
        .parse(route.request().postDataJSON());
      const operation = documents[body.id]?.match(/(?:query|mutation) (\w+)/)?.[1];
      let data: unknown;
      if (operation === "readerLibraryQuery" || operation === "readerLibraryPaginationQuery") {
        const variables = variablesSchema.parse(body.variables);
        fixture.requests.push(variables);
        const filter = variables.filter;
        const q = filter?.q ?? filter?.titleContains ?? filter?.titleEq ?? "";
        const books = fixture.books.filter(
          (book) =>
            book.title.toLowerCase().includes(q.toLowerCase()) &&
            book.pages >= (filter?.pagesGte ?? 0) &&
            book.pages <= (filter?.pagesLte ?? 2147483647),
        );
        const offset = Number(variables.cursor ?? 0);
        const pageBooks = books.slice(offset, offset + (variables.count ?? 50));
        const end = offset + pageBooks.length;
        data = {
          books: {
            edges: pageBooks.map((book, index) => ({
              cursor: String(offset + index + 1),
              node: graphBook(book),
            })),
            pageInfo: { hasNextPage: end < books.length, endCursor: end > 0 ? String(end) : null },
          },
        };
        await route.fulfill({ json: { data } });
        return;
      } else if (operation === "readerDetailQuery") {
        const book = fixture.books.find(
          (item) => graphId("Manifestation", item.id) === body.variables.id,
        );
        data = { manifestation: book === undefined ? null : graphBook(book) };
      } else if (
        operation === "readerShelvesQuery" ||
        operation === "readerShelvesPaginationQuery"
      ) {
        data = { shelves: shelves() };
      } else if (operation === "libraryBootstrapQuery") {
        if (fixture.bootstrap === "failing") {
          await route.fulfill({
            json: { errors: [{ message: "internal", extensions: { code: "internal" } }] },
          });
          return;
        }
        const found = labels(body.variables);
        data = {
          viewer: viewer(),
          authorLabels: found.authorLabels.map(({ id }) => ({ id })),
          seriesLabels: found.seriesLabels.map(({ id }) => ({ id })),
          shelfLabels: found.shelfLabels.map(({ id }) => ({ id })),
        };
      } else if (operation === "LibraryPageQuery") {
        data = { viewer: viewer(), ...labels(body.variables), shelves: shelves() };
      } else if (operation === "LibraryPageLabelsRefetchQuery") {
        data = labels(body.variables);
      } else if (operation === "TypeaheadMultiSelectRefetchQuery") {
        const fields = {
          authors: "author",
          series: "series",
          genres: "genre",
          moods: "mood",
          tags: "tag",
        };
        data = Object.fromEntries(
          Object.entries(fields)
            .filter(([flag]) => body.variables[flag] === true)
            .map(([flag, field]) => [
              `${field}Suggestions`,
              flag === "series" && withFacets
                ? SERIES.map((item) => ({ id: graphId("Series", item.id), name: item.value }))
                : [],
            ]),
        );
      } else if (operation === "mutationsUpdateLibraryPreferencesMutation") {
        const input = z.record(z.string(), z.unknown()).parse(body.variables.input);
        libraryPreferences = {
          ...libraryPreferences,
          ...Object.fromEntries(
            ["density", "view", "sort"]
              .filter((key) => key in input)
              .map((key) => [key, input[key]]),
          ),
        };
        data = {
          updateLibraryPreferences: { __typename: "LibraryPreferencesUpdated", viewer: viewer() },
        };
      } else {
        await route.fulfill({
          status: 400,
          json: { error: `Unprovided synthetic document: ${operation ?? body.id}` },
        });
        return;
      }
      await route.fulfill({ json: { data } });
    } else if (path === "/auth/me") {
      await route.fulfill({
        json: {
          id: "33333333-3333-4333-8333-333333333333",
          display_name: "Reader",
          email: null,
          role: "adult",
          is_child: false,
          has_local_password: true,
          theme_preference: "dark",
          csrf_token: "synthetic-journey-csrf-value",
        },
      });
    } else if (path === "/auth/setup/status") {
      await route.fulfill({
        json: {
          setup_required: false,
          local_auth_enabled: true,
          oidc_enabled: false,
          email_recovery_enabled: false,
          password_min_length: 15,
          password_max_length: 256,
        },
      });
    } else if (path === "/auth/me/preferences") {
      if (route.request().method() === "PATCH") {
        const patch = z
          .object({
            hidden_columns: z.array(z.string()).nullable().optional(),
            density: z.enum(["comfortable", "compact"]).nullable().optional(),
            view: z.enum(["grid", "table"]).nullable().optional(),
            sort_stack: z.string().nullable().optional(),
          })
          .parse(route.request().postDataJSON());
        preferences = { ...preferences, ...patch };
      }
      await route.fulfill({ json: preferences });
    } else if (path === "/api/v1/books") {
      const p = url.searchParams;
      const q = p.get("q") ?? p.get("title_contains") ?? p.get("title_eq") ?? "";
      const empty =
        !BOOK.title.toLowerCase().includes(q.toLowerCase()) ||
        Number(p.get("pages_gte") ?? 0) > BOOK.pages ||
        Number(p.get("pages_lte") ?? 2147483647) < BOOK.pages;
      await route.fulfill({ json: { items: empty ? [] : [BOOK], next_cursor: null } });
    } else if (path === `/api/v1/books/${BOOK.id}`) {
      await route.fulfill({
        json: {
          ...BOOK,
          description: null,
          language: "en",
          isbn_10: null,
          isbn_mismatch: false,
          publisher: null,
          pub_date: null,
          metadata_version_summary: { pending: 0, accepted: 0 },
          metadata_versions: [],
          updated_at: BOOK.created_at,
        },
      });
    } else if (path === "/api/v1/shelves") {
      await route.fulfill({ json: { items: withFacets ? SHELVES : [], next_cursor: null } });
    } else if (path === "/api/v1/series/suggest") {
      await route.fulfill({ json: { suggestions: withFacets ? SERIES : [] } });
    } else if (path === "/api/v1/series") {
      await route.fulfill({
        json: { items: SERIES.filter((item) => url.searchParams.getAll("id").includes(item.id)) },
      });
    } else if (path === "/api/v1/authors" || path === "/api/v1/suggest") {
      await route.fulfill({ json: { items: [], suggestions: [] } });
    } else {
      await route.fulfill({ status: 404, json: { error: `Unprovided synthetic API: ${path}` } });
    }
  });
  return fixture;
}

async function chooseFacets(page: Page): Promise<void> {
  const drawer = await refine(page);
  const shelves = await section(page, "Shelf");
  for (const shelf of SHELVES) await shelves.getByRole("option", { name: shelf.name }).click();
  const series = await section(page, "Series");
  const seriesInput = series.getByRole("combobox", { name: "Add Series" });
  await seriesInput.scrollIntoViewIfNeeded();
  await seriesInput.click();
  await seriesInput.fill("Sea");
  for (const item of SERIES) {
    await page.getByRole("option", { name: item.value, exact: true }).click();
  }
  await page.keyboard.press("Escape");
  await drawer.getByRole("button", { name: "Close filters", exact: true }).click();
}
const search = (page: Page) => page.getByRole("searchbox", { name: "Search your library" });
async function ready(page: Page): Promise<void> {
  await expect(search(page)).toBeVisible();
  await page.evaluate(async () => {
    await document.activeViewTransition?.finished;
  });
}
async function refine(page: Page) {
  await page.getByRole("button", { name: /^Filters/ }).click();
  const drawer = page.getByRole("dialog", { name: "Filters" });
  await expect(drawer).toBeVisible();
  return drawer;
}
async function section(page: Page, title: string) {
  const drawer = page.getByRole("dialog", { name: "Filters" });
  const button = drawer.getByRole("button", { name: new RegExp(`^${title}(?:\\s*\\d+)?$`) });
  if ((await button.getAttribute("aria-expanded")) === "false") await button.click();
  return button.locator("xpath=ancestor::section[1]");
}

test("rapid edits reload with their first request and no URL grammar", async ({ page }) => {
  // REV-REQ-0075: the resumed visit reaches the first list request intact, a failed lookup keeps
  // every saved selection, and a selection the lookup omits is dropped from the request and chips.
  const fixture = await intercept(page, true);
  await page.goto("/library");
  await ready(page);
  await chooseFacets(page);
  await search(page).fill("The Sea");
  fixture.bootstrap = "failing";
  await page.reload();
  await expect(page.getByRole("heading", { name: "Something went wrong" })).toBeVisible();
  fixture.bootstrap = "healthy";
  let before = fixture.requests.length;
  await page.reload();
  await ready(page);
  await expect(search(page)).toHaveValue("The Sea");
  expect(fixture.requests[before]?.filter?.q).toBe("The Sea");
  expect(fixture.requests[before]?.filter?.shelfAny).toEqual(
    SHELVES.map((item) => graphId("Shelf", item.id)),
  );
  expect(fixture.requests[before]?.filter?.seriesAny).toEqual(
    SERIES.map((item) => graphId("Series", item.id)),
  );
  fixture.invisible.add(graphId("Series", SERIES[1].id));
  before = fixture.requests.length;
  await page.reload();
  await ready(page);
  await expect(search(page)).toHaveValue("The Sea");
  expect(fixture.requests[before]?.filter?.q).toBe("The Sea");
  expect(fixture.requests[before]?.filter?.shelfAny).toEqual(
    SHELVES.map((item) => graphId("Shelf", item.id)),
  );
  expect(fixture.requests[before]?.filter?.seriesAny).toEqual([graphId("Series", SERIES[0].id)]);
  const chips = page.getByTestId("filter-chips");
  await expect(chips.getByRole("row", { name: /^Series: / })).toHaveCount(1);
  await expect(
    chips.getByRole("row", { name: "Series: Sea adventures", exact: true }),
  ).toBeVisible();
  for (const shelf of SHELVES)
    await expect(
      chips.getByRole("row", { name: `Shelf: ${shelf.name}`, exact: true }),
    ).toBeVisible();
  await expect(page.getByRole("heading", { name: "The Sea", exact: true })).toBeVisible();
  await expect(page).toHaveURL(/\/library$/);
});

test("book Back and Forward restore a visit; Back past Library does not undo edits", async ({
  page,
}) => {
  // REV-REQ-0075 and REV-REQ-0077: traversal restores predicates without filter history entries.
  const fixture = await intercept(page, true);
  await page.goto("/shelves");
  await page.getByRole("link", { name: "Library", exact: true }).first().click();
  await ready(page);
  await chooseFacets(page);
  await search(page).fill("Sea");
  await search(page).fill("The Sea");
  await page
    .getByRole("link", { name: /The Sea/ })
    .last()
    .click();
  await expect(page).toHaveURL(new RegExp(`/b/${graphId("Manifestation", BOOK.id)}$`));
  await page.goBack();
  await ready(page);
  await expect(search(page)).toHaveValue("The Sea");
  for (const series of SERIES)
    await expect(
      page
        .getByTestId("filter-chips")
        .getByRole("row", { name: `Series: ${series.value}`, exact: true }),
    ).toBeVisible();
  expect(fixture.requests.at(-1)?.filter?.shelfAny).toEqual(
    SHELVES.map((item) => graphId("Shelf", item.id)),
  );
  expect(fixture.requests.at(-1)?.filter?.seriesAny).toEqual(
    SERIES.map((item) => graphId("Series", item.id)),
  );
  await page.goForward();
  await expect(page.getByRole("heading", { name: "The Sea" })).toBeVisible();
  await page.goBack();
  await ready(page);
  await expect(search(page)).toHaveValue("The Sea");
  await page.goBack();
  await expect(page).toHaveURL(/\/shelves$/);
});

test("fresh same-route entry and direct context start clean; traversing restores each destination", async ({
  page,
  context,
}) => {
  await intercept(page);
  await page.goto("/library?entry=first");
  await ready(page);
  await search(page).fill("Sea");
  await page.getByRole("link", { name: "Library", exact: true }).first().click();
  await expect(search(page)).toHaveValue("");
  await page.goBack();
  await expect(search(page)).toHaveValue("Sea");
  await page.goForward();
  await expect(search(page)).toHaveValue("");
  const direct = await context.newPage();
  await intercept(direct);
  await direct.goto("/library?q=legacy&view=table");
  await ready(direct);
  await expect(search(direct)).toHaveValue("");
  await expect(direct.getByTestId("library-grid")).toBeVisible();
});

test("Refine Escape retains live text and range edits and returns focus", async ({ page }) => {
  const fixture = await intercept(page);
  await page.goto("/library");
  await ready(page);
  const opener = page.getByRole("button", { name: /^Filters/ });
  await refine(page);
  const title = await section(page, "Title");
  await title.getByRole("textbox", { name: "Filter value" }).fill("The Sea ");
  const pages = await section(page, "Pages");
  const lower = pages.getByRole("textbox", { name: "Pages minimum" });
  await lower.fill("300");
  await lower.press("Enter");
  const upper = pages.getByRole("textbox", { name: "Pages maximum" });
  await upper.fill("200");
  await upper.press("Enter");
  await upper.press("Escape");
  await expect(page.getByRole("dialog", { name: "Filters" })).toBeHidden();
  await expect(opener).toBeFocused();
  await expect(page.getByText("No books match these filters", { exact: true })).toBeVisible();
  expect(
    fixture.requests.some(
      (p) =>
        p.filter?.pagesGte === 300 &&
        p.filter.pagesLte === 200 &&
        p.filter.titleContains === "The Sea",
    ),
  ).toBe(true);
  await refine(page);
  await expect(
    (await section(page, "Title")).getByRole("textbox", { name: "Filter value" }),
  ).toHaveValue("The Sea ");
  await expect(
    (await section(page, "Pages")).getByText(
      "Minimum is above maximum. No books match this range.",
    ),
  ).toBeVisible();
  await pages.getByRole("button", { name: "Clear Pages filters" }).click();
  await expect(title.getByRole("textbox", { name: "Filter value" })).toHaveValue("The Sea ");
});

test("unchanged returns preserve each visit's books and known-book position", async ({ page }) => {
  // REV-REQ-0080 R1a, R1b: identical visits retain independent extents and known-book positions.
  await intercept(page, false, { books: RETURN_BOOKS, view: "table" });
  await page.goto("/library");
  await ready(page);
  const table = page.getByRole("grid", { name: "Library books", exact: true });
  const first = table.locator(
    `[data-book-id="${graphId("Manifestation", "00000000-0000-4000-8000-000000000009")}"]`,
  );
  await expect(table).toBeVisible();
  await table.evaluate((element) => {
    element.scrollTo({ top: 400, behavior: "instant" });
  });
  await expect(first).toBeVisible();
  await first.getByRole("link", { name: "Journey book 009", exact: true }).focus();
  const firstScroll = await first.evaluate((element) => {
    const grid = element.closest('[role="grid"]');
    const header = grid?.querySelector('[role="columnheader"]');
    if (grid === null || header == null) throw new Error("The book's table must be rendered.");
    return new Promise<number>((resolve) => {
      grid.addEventListener(
        "scrollend",
        () => {
          resolve(grid.scrollTop);
        },
        { once: true },
      );
      grid.scrollTo({
        top:
          grid.scrollTop +
          element.getBoundingClientRect().top -
          header.getBoundingClientRect().bottom +
          8,
        behavior: "instant",
      });
    });
  });
  const firstTop = await first.evaluate((element) => element.getBoundingClientRect().top);
  await expect(page.getByText("50 books loaded", { exact: true }).first()).toBeVisible();
  await first.getByRole("link", { name: "Journey book 009", exact: true }).dispatchEvent("click");
  await expect(page).toHaveURL(
    new RegExp(`/b/${graphId("Manifestation", "00000000-0000-4000-8000-000000000009")}$`),
  );
  await expect(page.getByRole("heading", { name: "Journey book 009", exact: true })).toBeVisible();
  await page.goBack();
  await ready(page);
  await expect(page.getByText("Updating results…", { exact: true })).toBeHidden();
  await expect(page.getByText("50 books loaded", { exact: true }).first()).toBeVisible();
  await expect
    .poll(async () =>
      Math.abs((await first.evaluate((element) => element.getBoundingClientRect().top)) - firstTop),
    )
    .toBeLessThanOrEqual(1);
  await expect(table).toHaveJSProperty("scrollTop", firstScroll);

  await page.goForward();
  await expect(page).toHaveURL(
    new RegExp(`/b/${graphId("Manifestation", "00000000-0000-4000-8000-000000000009")}$`),
  );
  await expect(page.getByRole("heading", { name: "Journey book 009", exact: true })).toBeVisible();
  await page.getByRole("link", { name: "Library", exact: true }).first().click();
  await ready(page);
  await page.getByRole("button", { name: "Load more", exact: true }).click();
  await expect(page.getByText("100 books loaded", { exact: true }).first()).toBeVisible();
  await table.evaluate((element) => {
    element.scrollTo({ top: 1700, behavior: "instant" });
  });
  const second = table.locator(
    `[data-book-id="${graphId("Manifestation", "00000000-0000-4000-8000-000000000039")}"]`,
  );
  await expect(second).toBeAttached();
  await second.getByRole("link", { name: "Journey book 039", exact: true }).focus();
  const secondScroll = await second.evaluate((element) => {
    const grid = element.closest('[role="grid"]');
    const header = grid?.querySelector('[role="columnheader"]');
    if (grid === null || header == null) throw new Error("The book's table must be rendered.");
    return new Promise<number>((resolve) => {
      grid.addEventListener(
        "scrollend",
        () => {
          resolve(grid.scrollTop);
        },
        { once: true },
      );
      grid.scrollTo({
        top:
          grid.scrollTop +
          element.getBoundingClientRect().top -
          header.getBoundingClientRect().bottom +
          8,
        behavior: "instant",
      });
    });
  });
  const secondTop = await second.evaluate((element) => element.getBoundingClientRect().top);
  await second.getByRole("link", { name: "Journey book 039", exact: true }).dispatchEvent("click");
  await expect(page).toHaveURL(
    new RegExp(`/b/${graphId("Manifestation", "00000000-0000-4000-8000-000000000039")}$`),
  );
  await expect(page.getByRole("heading", { name: "Journey book 039", exact: true })).toBeVisible();
  await page.goBack();
  await ready(page);
  await expect(page.getByText("Updating results…", { exact: true })).toBeHidden();
  await expect(page.getByText("100 books loaded", { exact: true }).first()).toBeVisible();
  await expect
    .poll(async () =>
      Math.abs(
        (await second.evaluate((element) => element.getBoundingClientRect().top)) - secondTop,
      ),
    )
    .toBeLessThanOrEqual(1);
  await expect(table).toHaveJSProperty("scrollTop", secondScroll);

  await page.goBack();
  await expect(page).toHaveURL(
    new RegExp(`/b/${graphId("Manifestation", "00000000-0000-4000-8000-000000000009")}$`),
  );
  await expect(page.getByRole("heading", { name: "Journey book 009", exact: true })).toBeVisible();
  await page.goBack();
  await ready(page);
  await expect(page.getByText("Updating results…", { exact: true })).toBeHidden();
  await expect(page.getByText("50 books loaded", { exact: true }).first()).toBeVisible();
  await expect
    .poll(async () =>
      Math.abs((await first.evaluate((element) => element.getBoundingClientRect().top)) - firstTop),
    )
    .toBeLessThanOrEqual(1);
  await expect(table).toHaveJSProperty("scrollTop", firstScroll);
});

test("a moved book returns to its saved position", async ({ page }) => {
  // REV-REQ-0080 R2: a named book keeps its viewport position when its list index changes.
  const fixture = await intercept(page, false, { books: RETURN_BOOKS, view: "table" });
  await page.goto("/library");
  await ready(page);
  const table = page.getByRole("grid", { name: "Library books", exact: true });
  await expect(table).toBeVisible();
  await table.evaluate((element) => {
    element.scrollTo({ top: 400, behavior: "instant" });
  });
  const book = table.locator(
    `[data-book-id="${graphId("Manifestation", "00000000-0000-4000-8000-000000000009")}"]`,
  );
  await expect(book).toBeVisible();
  await book.getByRole("link", { name: "Journey book 009", exact: true }).focus();
  await book.evaluate((element) => {
    const grid = element.closest('[role="grid"]');
    const header = grid?.querySelector('[role="columnheader"]');
    if (grid === null || header == null) throw new Error("The book's table must be rendered.");
    return new Promise<number>((resolve) => {
      grid.addEventListener(
        "scrollend",
        () => {
          resolve(grid.scrollTop);
        },
        { once: true },
      );
      grid.scrollTo({
        top:
          grid.scrollTop +
          element.getBoundingClientRect().top -
          header.getBoundingClientRect().bottom +
          8,
        behavior: "instant",
      });
    });
  });
  const savedTop = await book.evaluate((element) => element.getBoundingClientRect().top);
  await book.getByRole("link", { name: "Journey book 009", exact: true }).dispatchEvent("click");
  await expect(page).toHaveURL(
    new RegExp(`/b/${graphId("Manifestation", "00000000-0000-4000-8000-000000000009")}$`),
  );
  await expect(page.getByRole("heading", { name: "Journey book 009", exact: true })).toBeVisible();
  fixture.books.unshift(
    {
      ...BOOK,
      id: "00000000-0000-4000-8000-000000000121",
      work_id: "10000000-0000-4000-8000-000000000121",
      title: "New arrival one",
      created_at: "2026-11-17T00:00:00Z",
    },
    {
      ...BOOK,
      id: "00000000-0000-4000-8000-000000000122",
      work_id: "10000000-0000-4000-8000-000000000122",
      title: "New arrival two",
      created_at: "2026-11-17T00:00:00Z",
    },
  );
  const beforeReturn = fixture.requests.length;
  await page.goBack();
  await ready(page);
  await expect.poll(() => fixture.requests.length).toBeGreaterThan(beforeReturn);
  await expect(page.getByText("Updating results…", { exact: true })).toBeHidden();
  await expect
    .poll(async () =>
      Math.abs((await book.evaluate((element) => element.getBoundingClientRect().top)) - savedTop),
    )
    .toBeLessThanOrEqual(1);
});

test("a user scroll wins during a held return", async ({ page }) => {
  // REV-REQ-0080 Y1: native user scrolling supersedes a delayed return completion.
  await intercept(page, false, { books: RETURN_BOOKS, view: "table" });
  await page.goto("/library");
  await ready(page);
  const table = page.getByRole("grid", { name: "Library books", exact: true });
  await expect(table).toBeVisible();
  await table.evaluate((element) => {
    element.scrollTo({ top: 400, behavior: "instant" });
  });
  const original = table.locator(
    `[data-book-id="${graphId("Manifestation", "00000000-0000-4000-8000-000000000009")}"]`,
  );
  await expect(original).toBeVisible();
  await original
    .getByRole("link", { name: "Journey book 009", exact: true })
    .dispatchEvent("click");
  await expect(page).toHaveURL(
    new RegExp(`/b/${graphId("Manifestation", "00000000-0000-4000-8000-000000000009")}$`),
  );
  await expect(page.getByRole("heading", { name: "Journey book 009", exact: true })).toBeVisible();
  let release: (() => void) | undefined;
  const paused = new Promise<void>((resolve) => {
    release = resolve;
  });
  const held = page.waitForRequest((request) => {
    if (!request.url().endsWith("/api/graphql")) return false;
    const body = z.object({ id: z.string() }).parse(request.postDataJSON());
    return documents[body.id]?.includes("query readerLibraryQuery(");
  });
  await page.route("**/api/graphql", async (route) => {
    const body = z.object({ id: z.string() }).parse(route.request().postDataJSON());
    if (documents[body.id]?.includes("query readerLibraryQuery(")) {
      await paused;
    }
    await route.fallback();
  });
  await page.goBack();
  const request = await held;
  await ready(page);
  await expect(page.getByText("Updating results…", { exact: true })).toBeVisible();
  const chosen = table.locator(
    `[data-book-id="${graphId("Manifestation", "00000000-0000-4000-8000-000000000014")}"]`,
  );
  await expect(chosen).toBeAttached();
  const chosenScroll = await chosen.evaluate((element) => {
    const grid = element.closest('[role="grid"]');
    const header = grid?.querySelector('[role="columnheader"]');
    if (grid === null || header == null) throw new Error("The book's table must be rendered.");
    return new Promise<number>((resolve) => {
      grid.addEventListener(
        "scrollend",
        () => {
          resolve(grid.scrollTop);
        },
        { once: true },
      );
      grid.scrollTo({
        top:
          grid.scrollTop +
          element.getBoundingClientRect().top -
          header.getBoundingClientRect().bottom +
          8,
        behavior: "instant",
      });
    });
  });
  const chosenTop = await chosen.evaluate((element) => element.getBoundingClientRect().top);
  release?.();
  await request.response();
  await expect(page.getByText("Updating results…", { exact: true })).toBeHidden();
  await expect
    .poll(async () =>
      Math.abs(
        (await chosen.evaluate((element) => element.getBoundingClientRect().top)) - chosenTop,
      ),
    )
    .toBeLessThanOrEqual(1);
  await expect(table).toHaveJSProperty("scrollTop", chosenScroll);
});
