import { expect, test, type Page } from "@playwright/test";

const ADMIN_ME = {
  id: "11111111-1111-4111-8111-111111111111",
  display_name: "Root Admin",
  email: null,
  role: "admin",
  is_child: false,
  has_local_password: true,
  theme_preference: "system",
  csrf_token: null,
};

const SHELF = {
  id: "shelf-1",
  name: "Reading List",
  is_system: false,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  item_count: 3,
};

const BOOK_LIST_ITEM = {
  id: "b1",
  work_id: "w1",
  title: "Guards! Guards!",
  subtitle: null,
  authors: ["Terry Pratchett"],
  contributors: [],
  series: { id: "series-1", name: "Discworld", position: 8 },
  isbn_13: null,
  pages: null,
  tags: [],
  genres: [],
  moods: [],
  content_rating: null,
  cover_url: "",
  ingestion_status: "complete",
  validation_status: "clean",
  enrichment_status: "complete",
  reading_state: null,
  created_at: "2026-01-01T00:00:00Z",
};

const FILTERED_BOOK_LIST_ITEM = {
  ...BOOK_LIST_ITEM,
  id: "b2",
  work_id: "w2",
  title: "Mort",
};

const BOOK_DETAIL = {
  ...BOOK_LIST_ITEM,
  description: "A dragon appears.",
  language: "en",
  isbn_10: null,
  publisher: null,
  pub_date: null,
  tags: [],
  metadata_version_summary: { pending: 0, accepted: 0 },
  metadata_versions: [],
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

const STATS = {
  total_manifestations: 1,
  total_works: 1,
  storage_total_bytes: 0,
  storage_cover_bytes: 0,
  storage_by_format: [],
  validation_breakdown: [],
  clean_non_epub_count: 0,
  enrichment_breakdown: [],
  metadata_coverage: {
    total: 1,
    has_description: 1,
    has_language: 1,
    has_isbn_13: 0,
    has_cover: 0,
  },
};

function bodyFor(url: URL, role: string): unknown {
  const pathname = url.pathname;
  if (pathname === "/auth/setup/status")
    return {
      setup_required: false,
      local_auth_enabled: true,
      oidc_enabled: false,
      email_recovery_enabled: false,
      password_min_length: 15,
      password_max_length: 256,
    };
  if (pathname === "/auth/me/preferences")
    return {
      hidden_columns: null,
      density: null,
      view: null,
      sort_stack: null,
      defaults: {
        hidden_columns: [],
        density: "comfortable",
        view: "grid",
        sort_stack: "-created_at",
      },
    };
  if (pathname === "/auth/me") return { ...ADMIN_ME, role };
  if (pathname.startsWith("/api/v1/books/")) return BOOK_DETAIL;
  if (pathname.startsWith("/api/v1/books")) {
    if (url.searchParams.get("series") === "series-1") {
      return { items: [FILTERED_BOOK_LIST_ITEM], next_cursor: null };
    }
    return { items: [BOOK_LIST_ITEM], next_cursor: null };
  }
  if (pathname.startsWith("/api/v1/shelves/")) {
    return { ...SHELF, items: [], next_cursor: null };
  }
  if (pathname.startsWith("/api/v1/shelves")) {
    return { items: [SHELF], next_cursor: null };
  }
  if (pathname.startsWith("/api/v1/series/")) {
    return { id: "series-1", name: "Discworld", sort_name: "discworld", works: [] };
  }
  if (pathname === "/api/v1/users") return [{ ...ADMIN_ME, ...USER_TIMESTAMPS, disabled: false }];
  if (pathname === "/api/v1/dashboard/enrichment-failures/counts")
    return { by_failure: [], total: 0 };
  if (pathname === "/api/v1/ingestion/status") return { blockers: [] };
  if (pathname === "/api/v1/ingestion/inputs/counts") return { by_reason: [], attention_total: 0 };
  if (pathname === "/api/v1/dashboard/stats") return STATS;
  if (pathname.startsWith("/api/v1/dashboard/activity")) return { batches: [] };
  throw new Error(`Unhandled fetch path in application-navigation.spec.ts: ${pathname}`);
}

const USER_TIMESTAMPS = {
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

async function openLibrary(page: Page, role = "admin"): Promise<void> {
  await page.route(/^https?:\/\/[^/]+\/(?:api|auth)\//, async (route) => {
    await route.fulfill({ json: bodyFor(new URL(route.request().url()), role) });
  });
  await page.goto("/library");
  await expect(page.getByRole("heading", { name: "Library", exact: true })).toBeVisible();
}

test("book navigation preserves the shell through browser back and forward", async ({ page }) => {
  await openLibrary(page);
  const main = await page.getByRole("main").elementHandle();
  await page.getByRole("link", { name: /Guards! Guards!/ }).click();
  await expect(page.getByRole("link", { name: /Discworld · #8/ })).toBeVisible();
  expect(await page.getByRole("main").evaluate((node, original) => node === original, main)).toBe(
    true,
  );
  await page.goBack();
  await expect(page.getByRole("heading", { name: "Library", exact: true })).toBeVisible();
  expect(await page.getByRole("main").evaluate((node, original) => node === original, main)).toBe(
    true,
  );
  await page.goForward();
  await expect(page.getByRole("link", { name: /Discworld · #8/ })).toBeVisible();
  expect(await page.getByRole("main").evaluate((node, original) => node === original, main)).toBe(
    true,
  );
  await expect(page).toHaveURL(/\/b\/b1$/);
});

test("metadata clear confirmation stays interactive through pending, refusal and retry", async ({
  page,
}) => {
  await openLibrary(page);
  let attempts = 0;
  let refuse = () => {};
  const held = new Promise<void>((resolve) => {
    refuse = resolve;
  });
  await page.route("**/api/v1/books/b1/metadata", async (route) => {
    if (route.request().method() === "GET") {
      await route.fulfill({
        headers: { ETag: '"metadata-version"' },
        json: {
          ...BOOK_DETAIL,
          publisher: "Original Publisher",
          contributors: { author: ["Terry Pratchett"], editor: [], translator: [] },
          work_identifiers: {},
          manifestation_identifiers: {},
        },
      });
      return;
    }
    expect(route.request().method()).toBe("PATCH");
    expect(route.request().postDataJSON()).toEqual({ publisher: null });
    attempts += 1;
    if (attempts === 1) {
      await held;
      await route.fulfill({
        status: 422,
        json: {
          type: "about:blank",
          title: "Validation Error",
          detail: "Publisher cannot be cleared.",
          status: 422,
        },
      });
    } else {
      await route.fulfill({ json: { fields: {} } });
    }
  });
  await page.getByRole("link", { name: /Guards! Guards!/ }).click();
  await page.getByRole("tab", { name: "Versions" }).click();
  await page.getByRole("button", { name: "Edit metadata" }).click();
  const publisher = page.getByLabel("Publisher", { exact: true });
  await expect(publisher).toHaveValue("Original Publisher");
  await publisher.fill("");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const confirmation = page.getByRole("alertdialog", { name: "Clear populated field?" });
  await expect(confirmation.getByRole("button", { name: "Cancel" })).toBeFocused();
  const confirm = confirmation.getByRole("button", { name: "Clear and save" });
  await confirm.click();
  await expect(confirm).toHaveAttribute("data-pending");
  await expect(confirm).toBeFocused();
  await confirm.press("Enter");
  await expect.poll(() => attempts).toBe(1);
  refuse();
  await expect(confirmation).toHaveCount(0);
  await expect(page.getByRole("alert")).toContainText("Publisher cannot be cleared.");
  await expect(publisher).toHaveValue("");
  expect(attempts).toBe(1);
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await confirm.click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page).toHaveURL(/\/b\/b1$/);
  expect(attempts).toBe(2);
});

test("every routed screen is reachable by click from Library", async ({ page }) => {
  await openLibrary(page);
  await page.getByRole("link", { name: /Guards! Guards!/ }).click();
  await expect(page).toHaveURL(/\/b\/b1$/);
  await expect(page.getByRole("heading", { name: /Guards! Guards!/ })).toBeVisible();
  await page.getByRole("link", { name: /Discworld · #8/ }).click();
  await expect(page).toHaveURL(/\/series\/series-1$/);
  await page.getByRole("link", { name: "Shelves", exact: true }).click();
  await expect(page).toHaveURL(/\/shelves$/);
  await page
    .getByRole("navigation", { name: "Primary" })
    .getByRole("link", { name: /Reading List/ })
    .click();
  await expect(page).toHaveURL(/\/shelves\/shelf-1$/);
  await page.getByRole("link", { name: "Users", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/users$/);
  await page.getByRole("link", { name: "Ingestion", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/dashboard$/);
  await page.getByRole("link", { name: "Library", exact: true }).click();
  await expect(page).toHaveURL(/\/library$/);
});

test("series facet refetches results and clearing restores the unfiltered grid", async ({
  page,
}) => {
  await openLibrary(page);
  await expect(page.getByRole("link", { name: /Guards! Guards!/ })).toBeVisible();
  const filters = page.getByRole("button", { name: /^Filters/ });
  await expect(filters).toHaveAttribute("aria-controls", "library-filter-drawer");
  await filters.click();
  await page.getByRole("button", { name: "Series", expanded: false }).click();
  await page.getByText("Discworld", { exact: true }).click();
  await expect(page).toHaveURL(/\/library$/);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("checkbox", { name: "Discworld" })).toHaveCount(0);
  await expect(page.getByRole("link", { name: /Mort/ })).toBeVisible();
  await expect(page.getByRole("link", { name: /Guards! Guards!/ })).toHaveCount(0);
  await filters.click();
  const active = page.getByRole("checkbox", { name: "Discworld" });
  await expect(active).toBeChecked();
  await page.getByText("Discworld", { exact: true }).click();
  await expect(active).not.toBeChecked();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("checkbox", { name: "Discworld" })).toHaveCount(0);
  await expect(page.getByRole("link", { name: /Guards! Guards!/ })).toBeVisible();
});

test("non-admin never sees the admin cluster", async ({ page }) => {
  await openLibrary(page, "adult");
  await expect(
    page.getByRole("navigation", { name: "Primary" }).getByRole("link", { name: /Reading List/ }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "Users", exact: true })).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Ingestion", exact: true })).toHaveCount(0);
  await expect(page.getByRole("group", { name: "Admin", exact: true })).toHaveCount(0);
});
