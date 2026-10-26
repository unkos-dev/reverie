import { expect, test, type Page, type Route } from "@playwright/test";
import { z } from "zod";

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

type Fixture = {
  requests: URLSearchParams[];
  observations: { atMs: number; params: string }[];
  hold: (route: Route, url: URL) => Promise<boolean>;
};
async function intercept(page: Page): Promise<Fixture> {
  const started = performance.now();
  const fixture: Fixture = { requests: [], observations: [], hold: () => Promise.resolve(false) };
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
      fixture.observations.push({
        atMs: performance.now() - started,
        params: url.searchParams.toString(),
      });
      if (await fixture.hold(route, url)) return;
      const p = url.searchParams;
      const q = p.get("q") ?? p.get("title_contains") ?? p.get("title_eq") ?? "";
      const lower = p.get("created_at_gte");
      const upper = p.get("created_at_lt");
      const empty =
        !BOOK.title.toLowerCase().includes(q.toLowerCase()) ||
        Number(p.get("pages_gte") ?? 0) > BOOK.pages ||
        Number(p.get("pages_lte") ?? 2147483647) < BOOK.pages ||
        (lower !== null && Date.parse(lower) > Date.parse(BOOK.created_at)) ||
        (upper !== null && Date.parse(upper) <= Date.parse(BOOK.created_at));
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

const TAG_VALUES = Array.from(
  { length: 21 },
  (_, index) => `Tag ${String(index).padStart(2, "0")}`,
);
async function restoreTags(page: Page, values: string[]): Promise<void> {
  const state = z
    .looseObject({
      reverieLibrary: z.looseObject({ filters: z.record(z.string(), z.unknown()) }),
    })
    .parse(await page.evaluate((): unknown => navigation.currentEntry?.getState()));
  state.reverieLibrary.filters.tags = { all: [], any: values, none: [] };
  await page.evaluate((next) => {
    navigation.updateCurrentEntry({ state: next });
  }, state);
  await page.reload();
  await ready(page);
}

test("vocabulary selections stop at twenty while removal and other modes remain usable", async ({
  page,
}, testInfo) => {
  const fixture = await intercept(page);
  await page.route("**/api/v1/tags/suggest?*", (route) => {
    const query = new URL(route.request().url()).searchParams.get("q") ?? "";
    return route.fulfill({
      json: {
        suggestions: TAG_VALUES.map((value, index) => ({
          id: `00000000-0000-4000-8000-${String(index).padStart(12, "0")}`,
          value,
        }))
          .filter(({ value }) => value.toLowerCase().includes(query.toLowerCase()))
          .slice(0, 20),
      },
    });
  });
  await page.goto("/library");
  await ready(page);
  await search(page).fill("Sea");
  await restoreTags(page, TAG_VALUES.slice(0, 19));
  await refine(page);
  const tags = await section(page, "Tags");
  const input = page.getByRole("combobox", { name: /^Add tags/ });
  await input.fill("Tag");
  await page.getByRole("option", { name: "Tag 19", exact: true }).click();
  await expect.poll(() => fixture.requests.at(-1)?.getAll("tag_any").length).toBe(20);
  await expect(input).toHaveAccessibleDescription(/20 selected\. Remove one to add another\./);
  await input.fill("Tag 20");
  const option = page.getByRole("option", { name: "Tag 20", exact: true });
  await expect(option).toBeDisabled();
  await page.screenshot({ path: testInfo.outputPath("selection-disabled-option.png") });
  await input.press("Enter");
  await input.press("Tab");
  await expect(
    tags.getByRole("grid", { name: "Selected tags (any of)" }).getByRole("row"),
  ).toHaveCount(20);
  expect(fixture.requests.every((params) => params.getAll("tag_any").length <= 20)).toBe(true);
  await tags.getByText("20 selected. Remove one to add another.").scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath("selection-limit.png") });
  await tags.getByRole("button", { name: "Remove Tag 00", exact: true }).click();
  await input.fill("Tag 20");
  await expect(option).toBeEnabled();
  await option.click();
  await expect.poll(() => fixture.requests.at(-1)?.getAll("tag_any").includes("Tag 20")).toBe(true);
  expect(fixture.requests.at(-1)?.getAll("tag_any")).toHaveLength(20);
  await page.keyboard.press("Escape");
  await tags.getByRole("button", { name: "Match mode" }).click();
  await page.getByRole("option", { name: "none of", exact: true }).click();
  await input.fill("Tag 00");
  await page.getByRole("option", { name: "Tag 00", exact: true }).click();
  await expect.poll(() => fixture.requests.at(-1)?.getAll("tag_none")).toEqual(["Tag 00"]);
  expect(fixture.requests.at(-1)?.getAll("tag_any")).toHaveLength(20);
});

test("oversized saved vocabulary selections survive reload and traversal without requests, then recover", async ({
  page,
}, testInfo) => {
  const fixture = await intercept(page);
  await page.goto("/shelves");
  await page.getByRole("link", { name: "Library", exact: true }).first().click();
  await ready(page);
  await search(page).fill("Sea");
  await expect.poll(() => fixture.requests.at(-1)?.get("q")).toBe("Sea");
  const before = fixture.requests.length;
  await restoreTags(page, TAG_VALUES);
  await expect(search(page)).toHaveValue("Sea");
  await expect(page.getByRole("alert")).toContainText(
    "Choose 20 or fewer values in each filter mode",
  );
  await page.goBack();
  await expect(page).toHaveURL(/\/shelves$/);
  await page.goForward();
  await ready(page);
  await expect(search(page)).toHaveValue("Sea");
  await expect(page.getByRole("alert")).toContainText("Results are paused.");
  expect(fixture.requests).toHaveLength(before);
  await refine(page);
  const tags = await section(page, "Tags");
  await expect(
    tags.getByRole("grid", { name: "Selected tags (any of)" }).getByRole("row"),
  ).toHaveCount(21);
  await expect(tags.getByRole("combobox")).toHaveAttribute("aria-invalid", "true");
  await expect(tags.getByRole("combobox")).toHaveAccessibleDescription(
    /Choose 20 or fewer values\./,
  );
  await tags.getByText("Choose 20 or fewer values.").scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath("selection-over-limit.png") });
  await tags.getByRole("button", { name: "Remove Tag 00", exact: true }).click();
  await expect.poll(() => fixture.requests.length).toBe(before + 1);
  expect(fixture.requests.at(-1)?.get("q")).toBe("Sea");
  expect(fixture.requests.at(-1)?.getAll("tag_any")).toHaveLength(20);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await restoreTags(page, TAG_VALUES);
  await expect(page.getByRole("alert")).toContainText("Results are paused.");
  await refine(page);
  const restored = await section(page, "Tags");
  await restored.getByRole("button", { name: /^Clear/ }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(search(page)).toHaveValue("Sea");
  await expect.poll(() => fixture.requests.at(-1)?.getAll("tag_any")).toEqual([]);
});

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

test("replacement navigation starts a clean destination and preserves earlier visit history", async ({
  page,
}) => {
  await intercept(page);
  await page.goto("/library");
  await ready(page);
  await search(page).fill("Sea");
  await page.goto("/");
  await ready(page);
  await expect(search(page)).toHaveValue("");
  await page.goBack();
  await ready(page);
  await expect(search(page)).toHaveValue("Sea");
});

for (const dismissal of ["Escape", "close", "outside"]) {
  test(`Refine ${dismissal} retains live text and range edits and returns focus`, async ({
    page,
  }, testInfo) => {
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
    if (dismissal === "Escape") await upper.press("Escape");
    else if (dismissal === "close")
      await page.getByRole("button", { name: "Close", exact: true }).click();
    else await page.mouse.click(10, 400);
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
    if (testInfo.project.name === "library-chromium" && dismissal === "close") {
      await pages.getByRole("textbox", { name: "Pages minimum" }).fill("2147483647");
      await pages.getByRole("textbox", { name: "Pages minimum" }).press("Enter");
      await page.screenshot({ path: testInfo.outputPath("maximum-pages.png") });
    }
    await pages.getByRole("button", { name: "Clear Pages filters" }).click();
    await expect(title.getByRole("textbox", { name: "Filter value" })).toHaveValue("The Sea ");
  });
}

for (const unavailableAfterReload of [false, true]) {
  test(`table chunk recovery reloads the page and preserves the visit; still unavailable=${String(unavailableAfterReload)}`, async ({
    page,
  }, testInfo) => {
    await intercept(page);
    let unavailable = true;
    let blockedRequests = 0;
    await page.route(/\/assets\/LibraryTableView-[^/]+\.js$/, async (route) => {
      if (unavailable) {
        blockedRequests += 1;
        await route.abort("failed");
      } else {
        await route.continue();
      }
    });
    await page.goto("/library");
    await ready(page);
    await search(page).fill("Sea");
    await page.getByRole("radio", { name: "Table", exact: true }).click();
    const failure = page.getByRole("alert").filter({ hasText: "Couldn't load the table view." });
    await expect(failure).toBeVisible();
    expect(blockedRequests).toBeGreaterThan(0);
    await expect(search(page)).toBeEnabled();
    const reload = failure.getByRole("button", { name: "Reload page", exact: true });
    await reload.focus();
    await expect(reload).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(failure.getByRole("button", { name: "Switch to grid view" })).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(reload).toBeFocused();
    await page.screenshot({ path: testInfo.outputPath("table-chunk-failure.png") });
    unavailable = unavailableAfterReload;
    await Promise.all([
      page.waitForEvent("domcontentloaded"),
      unavailableAfterReload ? reload.click() : reload.press("Enter"),
    ]);
    await ready(page);
    await expect(search(page)).toHaveValue("Sea");
    await expect(page.getByRole("radio", { name: "Table", exact: true })).toBeChecked();
    if (unavailableAfterReload) {
      await expect(failure).toBeVisible();
      await failure.getByRole("button", { name: "Switch to grid view", exact: true }).click();
      await expect(page.getByRole("radio", { name: "Grid", exact: true })).toBeChecked();
      await expect(failure).toHaveCount(0);
      await expect(search(page)).toHaveValue("Sea");
    } else {
      await expect(page.getByTestId("library-table")).toBeVisible();
      await expect(page.getByRole("link", { name: BOOK.title, exact: true })).toBeVisible();
      await expect(failure).toHaveCount(0);
      await page.screenshot({ path: testInfo.outputPath("table-chunk-recovered.png") });
    }
  });
}

test("clear preserves view and search applies one character without delays", async ({ page }) => {
  const fixture = await intercept(page);
  await page.goto("/library");
  await ready(page);
  await search(page).fill("S");
  await expect.poll(() => fixture.requests.at(-1)?.get("q")).toBe("S");
  await page.getByRole("radio", { name: "Table", exact: true }).click();
  await expect(page.getByTestId("library-table")).toBeVisible();
  await page.getByRole("button", { name: "Clear all", exact: true }).click();
  await expect(search(page)).toHaveValue("");
  await expect(page.getByTestId("library-table")).toBeVisible();
  await expect(page).toHaveURL(/\/library$/);
});

test("local date calendar, partial input and year prefixes keep controls mounted through updating and empty results", async ({
  page,
}, testInfo) => {
  await page.clock.setFixedTime("2026-10-25T04:00:00Z");
  const fixture = await intercept(page);
  await page.goto("/library");
  await ready(page);
  await refine(page);
  const added = await section(page, "Added");
  const after = added.getByRole("group", { name: "Added on or after" });
  const before = added.getByRole("group", { name: "Added on or before" });
  const afterTrigger = after.getByRole("button");
  await afterTrigger.click();
  await expect(page.getByRole("grid").last()).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("calendar-after.png") });
  await page.getByRole("button", { name: /25 October 2026/ }).click();
  await expect
    .poll(() => fixture.requests.at(-1)?.get("created_at_gte"))
    .toBe("2026-10-24T16:00:00.000Z");
  await page.screenshot({ path: testInfo.outputPath("local-day.png") });
  await before.getByRole("button").click();
  await expect(page.getByRole("grid").last()).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("calendar-before.png") });
  await page.keyboard.press("Escape");
  await expect(before.getByRole("button")).toBeFocused();
  await before.getByRole("spinbutton", { name: /day/i }).fill("12");
  expect(fixture.requests.at(-1)?.has("created_at_lt")).toBe(false);
  await expect(after.getByRole("spinbutton", { name: /year/i })).toHaveText("2026");
  await added.getByRole("button", { name: "Clear Added filters" }).click();
  await afterTrigger.click();
  await page.getByRole("button", { name: /25 October 2026/ }).click();
  await before.getByRole("button").click();
  await page.getByRole("button", { name: /25 October 2026/ }).click();
  const year = after.getByRole("spinbutton", { name: /year/i });
  await year.focus();
  await year.press("2");
  await expect
    .poll(() => fixture.requests.some((p) => p.get("created_at_gte")?.startsWith("0002-")))
    .toBe(true);
  await expect(year).toBeFocused();
  await year.press("0");
  await year.press("2");
  await year.press("7");
  await expect(
    added.getByText("Start date is after end date. No books match this range."),
  ).toBeVisible();
  await expect(year).toBeFocused();
  await page.screenshot({ path: testInfo.outputPath("date-contradiction.png") });
  await year.press("Backspace");
  await year.press("Backspace");
  await year.press("Backspace");
  await year.press("Backspace");
  await expect(year).toHaveAttribute("data-placeholder");
  await after.getByRole("spinbutton", { name: /month/i }).press("Backspace");
  await after.getByRole("spinbutton", { name: /month/i }).press("Backspace");
  await after.getByRole("spinbutton", { name: /day/i }).press("Backspace");
  await after.getByRole("spinbutton", { name: /day/i }).press("Backspace");
  await expect.poll(() => fixture.requests.at(-1)?.has("created_at_gte")).toBe(false);
  await testInfo.attach("date-requests.json", {
    body: JSON.stringify(fixture.observations, null, 2),
    contentType: "application/json",
  });
});

test("delayed date updates retain rows and focus; cached prefix edits remain usable", async ({
  page,
}, testInfo) => {
  await page.clock.setFixedTime("2026-10-25T04:00:00Z");
  const fixture = await intercept(page);
  await page.goto("/library");
  await ready(page);
  await refine(page);
  const added = await section(page, "Added");
  const after = added.getByRole("group", { name: "Added on or after" });
  const before = added.getByRole("group", { name: "Added on or before" });
  await after.getByRole("button").click();
  await page.getByRole("button", { name: /25 October 2026/ }).click();
  await before.getByRole("button").click();
  await page.getByRole("button", { name: /25 October 2026/ }).click();
  let release: () => void = () => {
    throw new Error("delayed request not started");
  };
  fixture.hold = async (route, url) => {
    if (!url.searchParams.get("created_at_gte")?.startsWith("2027-")) return false;
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    await route.fulfill({ json: { items: [], next_cursor: null } });
    return true;
  };
  const year = after.getByRole("spinbutton", { name: /year/i });
  await year.focus();
  const prefixResponse = page.waitForResponse((response) =>
    response.url().includes("created_at_gte=0002-"),
  );
  await year.press("2");
  expect(await (await prefixResponse).finished()).toBeNull();
  await expect(page.getByText("Updating results…")).toBeHidden();
  await year.press("0");
  await year.press("2");
  await year.press("7");
  await expect
    .poll(() => fixture.requests.at(-1)?.get("created_at_gte")?.startsWith("2027-"))
    .toBe(true);
  await expect(page.getByText("Updating results…")).toBeVisible();
  await expect(page.getByTestId("library-grid").locator("h3")).toBeVisible();
  await expect(year).toBeFocused();
  await expect(
    added.getByText("Start date is after end date. No books match this range."),
  ).toBeHidden();
  await page.screenshot({ path: testInfo.outputPath("date-updating.png") });
  release();
  await expect(
    added.getByText("Start date is after end date. No books match this range."),
  ).toBeVisible();
  await expect(year).toBeFocused();
  const requestCount = fixture.requests.length;
  await year.press("2");
  await expect(year).toHaveText("2");
  await expect(
    added.getByText("Start date is after end date. No books match this range."),
  ).toBeHidden();
  expect(fixture.requests.length).toBe(requestCount);
  await expect(year).toBeFocused();
  await page.screenshot({ path: testInfo.outputPath("cached-year-prefix.png") });
  await testInfo.attach("date-requests.json", {
    body: JSON.stringify(fixture.observations, null, 2),
    contentType: "application/json",
  });
});

test("overlong search and filter text prevent whole requests through reload and traversal, then recover", async ({
  page,
}, testInfo) => {
  const fixture = await intercept(page);
  await page.goto("/shelves");
  await page.getByRole("link", { name: "Library", exact: true }).first().click();
  await ready(page);
  await expect(page.getByTestId("library-grid")).toBeVisible();
  const before = fixture.requests.length;
  const overlong = "😀".repeat(201);
  await search(page).fill(overlong);
  await expect(search(page)).toBeFocused();
  await expect(search(page)).toHaveAttribute("aria-invalid", "true");
  await expect(search(page)).toHaveAccessibleDescription(
    "Use 200 characters or fewer, excluding surrounding spaces.",
  );
  await expect(page.getByRole("alert")).toContainText("Results are paused.");
  await page.screenshot({ path: testInfo.outputPath("text-search-invalid.png") });
  await page.reload();
  await ready(page);
  await expect(search(page)).toHaveValue(overlong);
  await expect(page.getByRole("alert")).toContainText("Results are paused.");
  await page.goBack();
  await expect(page).toHaveURL(/\/shelves$/);
  await page.goForward();
  await ready(page);
  await expect(search(page)).toHaveValue(overlong);
  expect(fixture.requests).toHaveLength(before);
  let releaseCorrection: () => void = () => {
    throw new Error("Correction request has not started");
  };
  fixture.hold = async (route, url) => {
    if (url.searchParams.get("q") !== "😀".repeat(200)) return false;
    await new Promise<void>((resolve) => {
      releaseCorrection = resolve;
    });
    await route.fulfill({ json: { items: [], next_cursor: null } });
    return true;
  };
  await search(page).fill(`  ${"😀".repeat(200)}  `);
  await expect(search(page)).not.toHaveAttribute("aria-invalid", "true");
  await expect.poll(() => fixture.requests.at(-1)?.get("q")).toBe("😀".repeat(200));
  await expect(search(page)).toBeFocused();
  await expect(page.getByRole("button", { name: /^Filters/ })).toBeVisible();
  await expect(page.getByText("Updating results…")).toBeVisible();
  await expect(page.locator('[aria-busy="true"]')).toBeVisible();
  await expect(page.getByText("No books yet")).toHaveCount(0);
  await expect(page.getByText("No books match these filters")).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath("text-correction-pending.png") });
  releaseCorrection();
  await expect(page.getByText("No books match these filters")).toBeVisible();
  await expect(search(page)).toBeFocused();
  await page.getByRole("button", { name: "Clear search", exact: true }).click();
  await expect(page.getByTestId("library-grid")).toBeVisible();
  await refine(page);
  const subtitle = await section(page, "Subtitle");
  const input = subtitle.getByRole("textbox", { name: "Filter value" });
  const activeCount = fixture.requests.length;
  await input.fill("x".repeat(201));
  await expect(input).toBeFocused();
  await expect(input).toHaveAttribute("aria-invalid", "true");
  await expect(input).toHaveAccessibleDescription(
    "Use 200 characters or fewer, excluding surrounding spaces.",
  );
  await page.screenshot({ path: testInfo.outputPath("text-filter-invalid.png") });
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: /^Filters/ })).toBeFocused();
  await expect(page.getByRole("alert")).toContainText("Results are paused.");
  await page.screenshot({ path: testInfo.outputPath("text-filter-closed.png") });
  expect(fixture.requests).toHaveLength(activeCount);
  await refine(page);
  const restored = await section(page, "Subtitle");
  await expect(restored.getByRole("textbox", { name: "Filter value" })).toHaveValue(
    "x".repeat(201),
  );
  await restored.getByRole("button", { name: /Operator/ }).click();
  await page.getByRole("option", { name: "is empty", exact: true }).click();
  await expect.poll(() => fixture.requests.at(-1)?.get("subtitle_empty")).toBe("true");
  await restored.getByRole("button", { name: /Operator/ }).click();
  const dormantCount = fixture.requests.length;
  await page.getByRole("option", { name: "contains", exact: true }).click();
  await expect(restored.getByRole("textbox", { name: "Filter value" })).toHaveValue(
    "x".repeat(201),
  );
  await page.keyboard.press("Escape");
  await expect(page.getByRole("alert")).toContainText("Results are paused.");
  expect(fixture.requests).toHaveLength(dormantCount);
  await page.reload();
  await ready(page);
  await expect(page.getByRole("alert")).toContainText("Results are paused.");
  let releaseClear: () => void = () => {
    throw new Error("Clear request has not started");
  };
  fixture.hold = async (route, url) => {
    if (url.search !== "") return false;
    await new Promise<void>((resolve) => {
      releaseClear = resolve;
    });
    await route.fulfill({ json: { items: [BOOK], next_cursor: null } });
    return true;
  };
  const clearCount = fixture.requests.length;
  await page.getByRole("button", { name: "Clear all", exact: true }).click();
  await expect.poll(() => fixture.requests.length).toBe(clearCount + 1);
  expect(fixture.requests.at(-1)?.toString()).toBe("");
  await expect(search(page)).toHaveValue("");
  await expect(page.getByText("Updating results…")).toBeVisible();
  await expect(page.locator('[aria-busy="true"]')).toBeVisible();
  await expect(page.getByText("No books yet")).toHaveCount(0);
  await expect(page.getByText("No books match these filters")).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath("text-clear-pending.png") });
  await refine(page);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: /^Filters/ })).toBeFocused();
  releaseClear();
  await expect(page.getByTestId("library-grid")).toBeVisible();
  await expect(page.getByText("Updating results…")).toHaveCount(0);
  await expect(page.locator('[aria-busy="true"]')).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
  await testInfo.attach("text-requests.json", {
    body: JSON.stringify(fixture.observations, null, 2),
    contentType: "application/json",
  });
});

test("a genuine server 422 keeps Search, Filters and Clear available for recovery", async ({
  page,
}, testInfo) => {
  const fixture = await intercept(page);
  fixture.hold = async (route, url) => {
    if (url.searchParams.get("q") !== "restricted") return false;
    await route.fulfill({
      status: 422,
      contentType: "application/problem+json",
      body: JSON.stringify({
        type: "https://reverie.example/probs/invalid-query",
        title: "Invalid query",
        status: 422,
        detail: "Synthetic server validation without field identifiers",
      }),
    });
    return true;
  };
  await page.goto("/library");
  await ready(page);
  await search(page).fill("restricted");
  await expect(page.getByRole("alert")).toContainText("These filters could not be used.");
  await expect(search(page)).toBeFocused();
  await expect(search(page)).not.toHaveAttribute("aria-invalid", "true");
  await expect(page.getByRole("button", { name: /^Filters/ })).toBeVisible();
  await expect(page.getByText("No books match these filters")).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath("text-server-422.png") });
  const rejectedResponses: number[] = [];
  page.on("response", (response) => {
    if (response.status() === 422) rejectedResponses.push(response.status());
  });
  await page.reload();
  await ready(page);
  await expect(search(page)).toHaveValue("restricted");
  await expect(page.getByRole("alert")).toContainText("These filters could not be used.");
  expect(rejectedResponses).toHaveLength(1);
  await search(page).fill("The Sea");
  await expect(page.getByTestId("library-grid")).toBeVisible();
  await expect(page.getByRole("grid", { name: "Active filters" })).toContainText(
    'Search "The Sea"',
  );
  await expect(page.getByText("Updating results…")).toHaveCount(0);
  await search(page).fill("restricted");
  await expect(page.getByRole("grid", { name: "Active filters" })).toContainText(
    'Search "restricted"',
  );
  await expect(page.getByRole("alert")).toContainText("These filters could not be used.");
  await page.getByRole("button", { name: "Clear all", exact: true }).click();
  await expect(search(page)).toHaveValue("");
  await expect(page.getByTestId("library-grid")).toBeVisible();
});
