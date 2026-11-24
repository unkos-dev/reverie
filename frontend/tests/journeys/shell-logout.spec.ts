import { expect, test, type Page, type Route } from "@playwright/test";
import { readFileSync } from "node:fs";
import { z } from "zod";

const documents = z
  .record(z.string(), z.string())
  .parse(
    JSON.parse(
      readFileSync(new URL("../../../backend/graphql-documents.json", import.meta.url), "utf8"),
    ),
  );

const ACCOUNT = {
  id: "33333333-3333-4333-8333-333333333333",
  display_name: "Ada Lovelace",
  email: "ada@example.com",
  role: "adult",
  is_child: false,
  has_local_password: true,
  theme_preference: "dark",
  csrf_token: "synthetic-shell-journey-csrf-value",
};

async function intercept(page: Page): Promise<{
  logoutRequests: string[];
  release: (failure: boolean) => void;
}> {
  const logoutRequests: string[] = [];
  let resolveLogout: (failure: boolean) => void = () => {};
  const pending = new Promise<boolean>((resolve) => {
    resolveLogout = resolve;
  });
  let signedOut = false;
  async function handle(route: Route): Promise<void> {
    const path = new URL(route.request().url()).pathname;
    if (path === "/api/graphql") {
      const body = z.object({ id: z.string() }).parse(route.request().postDataJSON());
      const operation = documents[body.id]?.match(/(?:query|mutation) (\w+)/)?.[1];
      const page = { edges: [], pageInfo: { endCursor: null, hasNextPage: false } };
      if (operation === "libraryBootstrapQuery" || operation === "LibraryPageQuery") {
        await route.fulfill({
          json: {
            data: {
              viewer: {
                id: "user_01h455vb4pex5vsknk084sn02q",
                libraryPreferences: {
                  hiddenColumns: null,
                  density: null,
                  view: null,
                  sort: null,
                  defaults: {
                    hiddenColumns: ["SUBTITLE"],
                    density: "COMFORTABLE",
                    view: "GRID",
                    sort: [{ column: "CREATED_AT", direction: "DESC" }],
                  },
                },
              },
              authorLabels: [],
              seriesLabels: [],
              shelfLabels: [],
              ...(operation === "LibraryPageQuery" ? { shelves: page, books: page } : {}),
            },
          },
        });
      } else {
        await route.fulfill({
          status: 400,
          json: { error: `Unprovided synthetic document: ${operation ?? body.id}` },
        });
      }
    } else if (path === "/auth/me") {
      await route.fulfill(
        signedOut ? { status: 401, json: { title: "Unauthorised" } } : { json: ACCOUNT },
      );
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
      await route.fulfill({
        json: {
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
        },
      });
    } else if (path === "/auth/logout") {
      logoutRequests.push(route.request().method());
      const failure = await pending;
      signedOut = true;
      if (failure) await route.abort("failed");
      else await route.fulfill({ status: 204 });
    } else if (path === "/api/v1/books" || path === "/api/v1/shelves") {
      await route.fulfill({ json: { items: [], next_cursor: null } });
    } else {
      await route.fulfill({ status: 404, json: { title: `Unprovided synthetic API: ${path}` } });
    }
  }
  await page.route(/^https?:\/\/[^/]+\/(?:api|auth)\//, handle);
  return { logoutRequests, release: resolveLogout };
}

async function signOut(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Ada Lovelace" }).click();
  await page.getByRole("menuitem", { name: "Sign out", exact: true }).click();
}

test("sign out waits for one POST and hard-navigates to login", async ({ page }) => {
  const fixture = await intercept(page);
  await page.goto("/library");
  await expect(page.getByRole("button", { name: "Ada Lovelace" })).toBeVisible();
  const documentURL = await page.evaluate(() => performance.timeOrigin);
  await signOut(page);
  await expect.poll(() => fixture.logoutRequests).toEqual(["POST"]);
  await expect(page).toHaveURL(/\/library$/);
  fixture.release(false);
  await expect(page).toHaveURL(/\/login$/);
  expect(await page.evaluate(() => performance.timeOrigin)).not.toBe(documentURL);
});

test("failed logout logs the failure and hard-navigates to login", async ({ page }) => {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  const fixture = await intercept(page);
  await page.goto("/library");
  await signOut(page);
  await expect.poll(() => fixture.logoutRequests).toEqual(["POST"]);
  fixture.release(true);
  await expect(page).toHaveURL(/\/login$/);
  expect(errors.some((message) => message.includes("[UserMenu] logout request failed"))).toBe(true);
});
