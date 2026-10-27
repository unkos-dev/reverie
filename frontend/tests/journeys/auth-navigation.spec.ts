import { expect, test, type Page } from "@playwright/test";

const status = {
  setup_required: false,
  local_auth_enabled: true,
  oidc_enabled: true,
  email_recovery_enabled: true,
  password_min_length: 15,
  password_max_length: 256,
};

async function signInOptions(page: Page): Promise<void> {
  await page.route("**/auth/setup/status", async (route) => {
    await route.fulfill({ json: status });
  });
  await page.route("**/auth/me", async (route) => {
    await route.fulfill({ json: { csrf_token: "synthetic-csrf", theme_preference: "system" } });
  });
}

for (const activation of ["pointer", "Enter"]) {
  test(`local sign-in by ${activation} posts once before replacing the document`, async ({
    page,
  }) => {
    await signInOptions(page);
    const submissions: unknown[] = [];
    await page.route("**/auth/local/login", async (route) => {
      submissions.push(route.request().postDataJSON());
      await route.fulfill({ status: 204 });
    });
    await page.route("**/library", async (route) => {
      expect(route.request().isNavigationRequest()).toBe(true);
      await route.fulfill({ contentType: "text/html", body: "<main>Signed in</main>" });
    });
    await page.goto("/login");
    await page.getByLabel("Email", { exact: true }).fill("ada@example.com");
    await page.getByLabel("Password", { exact: true }).fill("hunter2hunter2");
    if (activation === "Enter") await page.getByLabel("Password", { exact: true }).press("Enter");
    else await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page).toHaveURL(/\/library$/);
    await expect(page.getByRole("main")).toHaveText("Signed in");
    expect(submissions).toEqual([{ email: "ada@example.com", password: "hunter2hunter2" }]);
  });
}

for (const outcome of [
  undefined,
  "denied",
  "expired",
  "unavailable",
  "keys",
  "account",
  "throttled",
]) {
  test(`OIDC starts only on activation after outcome ${String(outcome)}`, async ({ page }) => {
    await signInOptions(page);
    let starts = 0;
    await page.route("**/auth/oidc/login", async (route) => {
      starts += 1;
      expect(route.request().isNavigationRequest()).toBe(true);
      await route.fulfill({ contentType: "text/html", body: "<main>Provider sign-in</main>" });
    });
    const entry = outcome === undefined ? "/login" : `/login?outcome=${outcome}`;
    await page.goto(entry);
    const provider = page.getByRole("button", { name: "Sign in with OIDC" });
    await expect(provider).toBeVisible();
    await expect(page.getByLabel("Email", { exact: true })).toBeVisible();
    expect(starts).toBe(0);
    await provider.click();
    await expect(page).toHaveURL(/\/auth\/oidc\/login$/);
    await expect(page.getByRole("main")).toHaveText("Provider sign-in");
    expect(starts).toBe(1);
  });
}

test("Back and Forward restore fixed outcomes without starting provider navigation", async ({
  page,
}) => {
  await signInOptions(page);
  let starts = 0;
  await page.route("**/auth/oidc/login", async (route) => {
    starts += 1;
    await route.fulfill({ status: 500 });
  });
  await page.goto("/login?outcome=denied");
  await expect(page.getByRole("heading", { name: "Sign-in cancelled" })).toBeFocused();
  await page.goto("/login?outcome=keys");
  await expect(page.getByRole("heading", { name: "Sign-in could not be verified" })).toBeFocused();
  await page.goBack();
  await expect(page).toHaveURL(/\?outcome=denied$/);
  await expect(page.getByRole("heading", { name: "Sign-in cancelled" })).toBeFocused();
  await page.goForward();
  await expect(page.getByRole("heading", { name: "Sign-in could not be verified" })).toBeFocused();
  expect(starts).toBe(0);
});
