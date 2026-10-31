import { expect, test, type Page } from "@playwright/test";

const setup = {
  setup_required: false,
  local_auth_enabled: true,
  oidc_enabled: false,
  email_recovery_enabled: true,
  password_min_length: 15,
  password_max_length: 256,
};
const me = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "admin",
  is_child: false,
  has_local_password: true,
  theme_preference: "system",
  csrf_token: "synthetic-csrf",
};

async function authenticated(page: Page): Promise<void> {
  await page.route("**/auth/me", async (route) => {
    await route.fulfill({ json: me });
  });
  await page.route("**/auth/setup/status", async (route) => {
    await route.fulfill({ json: setup });
  });
}

test("concurrent identity and account-query 401s produce one document navigation", async ({
  page,
}) => {
  await authenticated(page);
  const received = new Set<string>();
  let release: (() => void) | undefined;
  const together = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(/\/(?:auth\/me|api\/v1\/account\/email-verifications)$/, async (route) => {
    received.add(new URL(route.request().url()).pathname);
    if (received.size === 2) release?.();
    await together;
    await route.fulfill({ status: 401 });
  });
  await page.addInitScript(() => {
    navigation.addEventListener("navigate", (event) => {
      if (new URL(event.destination.url).pathname !== "/login") return;
      const count = Number(sessionStorage.getItem("loginNavigations") ?? "0");
      sessionStorage.setItem("loginNavigations", String(count + 1));
    });
  });
  await page.route("**/login", async (route) => {
    expect(route.request().isNavigationRequest()).toBe(true);
    await route.fulfill({ contentType: "text/html", body: "<main>Sign in again</main>" });
  });
  await page.goto("/account/email");
  await expect(page).toHaveURL(/\/login$/);
  await expect(page.getByRole("main")).toHaveText("Sign in again");
  expect(received.size).toBe(2);
  expect(await page.evaluate(() => sessionStorage.getItem("loginNavigations"))).toBe("1");
});

test("verification clears the fragment before GET and confirms only on activation", async ({
  page,
}) => {
  await authenticated(page);
  const token = "A".repeat(43);
  await page.route("**/auth/me", async (route) => {
    expect(await page.evaluate(() => location.hash)).toBe("");
    await route.fulfill({ json: me });
  });
  let reads = 0;
  const confirmations: unknown[] = [];
  await page.route("**/api/v1/account/email-verifications", async (route) => {
    expect(await page.evaluate(() => location.hash)).toBe("");
    reads += 1;
    await route.fulfill({
      json: { email: "holder@example.test", email_verified: false, mail_enabled: true },
    });
  });
  await page.route("**/api/v1/account/email-verifications/confirmations", async (route) => {
    confirmations.push(route.request().postDataJSON());
    await route.fulfill({ status: 204 });
  });
  await page.goto(`/verify-email#${token}`);
  const confirm = page.getByRole("button", { name: "Verify email" });
  await expect(confirm).toBeVisible();
  await expect(page).toHaveURL(/\/verify-email$/);
  expect(reads).toBe(1);
  expect(confirmations).toEqual([]);
  await confirm.click();
  await expect(page.getByRole("heading", { name: "Email address verified" })).toBeFocused();
  expect(confirmations).toEqual([{ token }]);
});
