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
const graphId = (kind: string, id: string) => Buffer.from(`${kind}:${id}`).toString("base64");
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
};
async function intercept(page: Page, withFacets = false): Promise<Fixture> {
  const fixture: Fixture = { requests: [] };
  let preferences = structuredClone(PREFERENCES);
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
        const books = [BOOK].filter(
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
        data = { manifestation: graphBook(BOOK) };
      } else if (
        operation === "readerShelvesQuery" ||
        operation === "readerShelvesPaginationQuery"
      ) {
        data = {
          shelves: {
            edges: (withFacets ? SHELVES : []).map((shelf) => ({
              cursor: shelf.id,
              node: {
                id: graphId("Shelf", shelf.id),
                name: shelf.name,
                isSystem: false,
                itemCount: 1,
                createdAt: BOOK.created_at,
                updatedAt: BOOK.created_at,
              },
            })),
            pageInfo: { endCursor: null, hasNextPage: false },
          },
        };
      } else if (operation === "readerLibraryPreferencesQuery") {
        data = {
          libraryPreferences: {
            hiddenColumns: null,
            density: null,
            view: null,
            sortStack: null,
            defaults: {
              hiddenColumns: ["subtitle"],
              density: "comfortable",
              view: "grid",
              sortStack: "-created_at",
            },
          },
        };
      } else if (operation === "mutationsUpdateLibraryPreferencesMutation") {
        data = {
          updateLibraryPreferences: {
            hiddenColumns: null,
            density: null,
            view: null,
            sortStack: null,
            ...z.record(z.string(), z.unknown()).parse(body.variables.patch),
            defaults: {
              hiddenColumns: ["subtitle"],
              density: "comfortable",
              view: "grid",
              sortStack: "-created_at",
            },
          },
        };
      } else if (operation === "readerSeriesSuggestionsQuery") {
        data = {
          seriesSuggestions: withFacets
            ? SERIES.map((item) => ({ entityId: graphId("Series", item.id), value: item.value }))
            : [],
        };
      } else if (operation === "readerSeriesLabelsQuery") {
        data = {
          seriesLabels: SERIES.map((item) => ({
            entityId: graphId("Series", item.id),
            value: item.value,
          })),
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
  // REV-REQ-0075: the resumed visit reaches the first list request intact.
  const fixture = await intercept(page, true);
  await page.goto("/library");
  await ready(page);
  await chooseFacets(page);
  await search(page).fill("The Sea");
  const before = fixture.requests.length;
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
  await expect(page).toHaveURL(new RegExp(`/b/${BOOK.id}$`));
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
