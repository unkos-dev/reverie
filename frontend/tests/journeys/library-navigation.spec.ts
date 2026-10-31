import { expect, test, type Page } from "@playwright/test";

const BOOK = {
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
const PREFERENCES = {
  hidden_columns: null,
  density: null,
  view: null,
  sort_stack: null,
  defaults: { hidden_columns: [], density: "comfortable", view: "grid", sort_stack: "-created_at" },
};

type Fixture = { requests: URLSearchParams[] };
async function intercept(page: Page): Promise<Fixture> {
  const fixture: Fixture = { requests: [] };
  await page.route(/^https?:\/\/[^/]+\/(?:api|auth)\//, async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (path === "/auth/me") {
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
      await route.fulfill({ json: PREFERENCES });
    } else if (path === "/api/v1/books") {
      fixture.requests.push(url.searchParams);
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
          publisher: null,
          pub_date: null,
          metadata_version_summary: { pending: 0, accepted: 0 },
          metadata_versions: [],
          updated_at: BOOK.created_at,
        },
      });
    } else if (path === "/api/v1/shelves") {
      await route.fulfill({ json: { items: [], next_cursor: null } });
    } else if (path === "/api/v1/authors" || path === "/api/v1/suggest") {
      await route.fulfill({ json: { items: [], suggestions: [] } });
    } else {
      await route.fulfill({ status: 404, json: { error: `Unprovided synthetic API: ${path}` } });
    }
  });
  return fixture;
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
  const drawer = page.getByRole("dialog", { name: "Refine" });
  await expect(drawer).toBeVisible();
  return drawer;
}
async function section(page: Page, title: string) {
  const drawer = page.getByRole("dialog", { name: "Refine" });
  const button = drawer.getByRole("button", { name: new RegExp(`^${title}(?:\\s*\\d+)?$`) });
  if ((await button.getAttribute("aria-expanded")) === "false") await button.click();
  return button.locator("xpath=ancestor::section[1]");
}

test("rapid edits reload with their first request and no URL grammar", async ({ page }) => {
  const fixture = await intercept(page);
  await page.goto("/library");
  await ready(page);
  await search(page).fill("The Sea");
  const before = fixture.requests.length;
  await page.reload();
  await ready(page);
  await expect(search(page)).toHaveValue("The Sea");
  expect(fixture.requests[before]?.get("q")).toBe("The Sea");
  await expect(page.getByRole("heading", { name: "The Sea", exact: true })).toBeVisible();
  await expect(page).toHaveURL(/\/library$/);
});

test("book Back and Forward restore a visit; Back past Library does not undo edits", async ({
  page,
}) => {
  await intercept(page);
  await page.goto("/shelves");
  await page.getByRole("link", { name: "Library", exact: true }).first().click();
  await ready(page);
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
  await expect(page.getByRole("dialog", { name: "Refine" })).toBeHidden();
  await expect(opener).toBeFocused();
  await expect(page.getByText("No books match these filters", { exact: true })).toBeVisible();
  expect(
    fixture.requests.some(
      (p) =>
        p.get("pages_gte") === "300" &&
        p.get("pages_lte") === "200" &&
        p.get("title_contains") === "The Sea",
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
