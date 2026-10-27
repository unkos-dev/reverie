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

for (const oidc of [false, true]) {
  test(`a cold session lapse replaces the document with ${oidc ? "OIDC" : "local sign-in"}`, async ({
    page,
  }) => {
    await authenticated(page);
    await page.route("**/auth/setup/status", async (route) => {
      await route.fulfill({ json: { ...setup, oidc_enabled: oidc } });
    });
    await page.route("**/auth/me", async (route) => {
      await route.fulfill({ status: 401 });
    });
    const target = oidc ? "/auth/oidc/login" : "/login";
    let navigations = 0;
    await page.route(`**${target}`, async (route) => {
      expect(route.request().isNavigationRequest()).toBe(true);
      navigations += 1;
      await route.fulfill({ contentType: "text/html", body: "<main>Session recovery</main>" });
    });
    await page.goto("/account/password");
    await expect(page).toHaveURL(new RegExp(`${target}$`));
    await expect(page.getByRole("main")).toHaveText("Session recovery");
    expect(navigations).toBe(1);
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
  let navigations = 0;
  await page.route("**/login", async (route) => {
    expect(route.request().isNavigationRequest()).toBe(true);
    navigations += 1;
    await route.fulfill({ contentType: "text/html", body: "<main>Sign in again</main>" });
  });
  await page.goto("/account/email");
  await expect(page).toHaveURL(/\/login$/);
  await expect(page.getByRole("main")).toHaveText("Sign in again");
  expect(received.size).toBe(2);
  expect(navigations).toBe(1);
});

test("an account-query 401 redirects an otherwise authenticated document", async ({ page }) => {
  await authenticated(page);
  await page.route("**/api/v1/account/email-verifications", async (route) => {
    await route.fulfill({ status: 401 });
  });
  let navigations = 0;
  await page.route("**/login", async (route) => {
    expect(route.request().isNavigationRequest()).toBe(true);
    navigations += 1;
    await route.fulfill({ contentType: "text/html", body: "<main>Sign in again</main>" });
  });
  await page.goto("/account/email");
  await expect(page.getByRole("main")).toHaveText("Sign in again");
  await expect(page).toHaveURL(/\/login$/);
  expect(navigations).toBe(1);
});

test("an operational account-query failure stays on the current document", async ({ page }) => {
  await authenticated(page);
  let redirects = 0;
  await page.route("**/login", async (route) => {
    redirects += 1;
    await route.fulfill({ status: 500 });
  });
  await page.route("**/api/v1/account/email-verifications", async (route) => {
    await route.fulfill({ status: 500 });
  });
  await page.goto("/account/email");
  await expect(page.getByRole("alert")).toHaveText("Could not load your email settings.");
  await expect(page.getByRole("button", { name: "Try again" })).toBeEnabled();
  await expect(page).toHaveURL(/\/account\/email$/);
  expect(redirects).toBe(0);
});

test("a password-change mutation 401 replaces the document once", async ({ page }) => {
  await authenticated(page);
  const submissions: unknown[] = [];
  await page.route("**/api/v1/account/password", async (route) => {
    submissions.push(route.request().postDataJSON());
    await route.fulfill({ status: 401 });
  });
  let navigations = 0;
  await page.route("**/login", async (route) => {
    expect(route.request().isNavigationRequest()).toBe(true);
    navigations += 1;
    await route.fulfill({ contentType: "text/html", body: "<main>Sign in again</main>" });
  });
  await page.goto("/account/password");
  await page.getByLabel("Current password", { exact: true }).fill("old-password-1");
  await page.getByLabel("New password", { exact: true }).fill("new-password-two-2");
  await page.getByRole("button", { name: "Change password" }).click();
  await expect(page).toHaveURL(/\/login$/);
  await expect(page.getByRole("main")).toHaveText("Sign in again");
  expect(navigations).toBe(1);
  expect(submissions).toEqual([
    { current_password: "old-password-1", new_password: "new-password-two-2" },
  ]);
});

test("a pre-auth sign-in 401 stays on the document and retains credentials", async ({ page }) => {
  await authenticated(page);
  await page.route("**/auth/local/login", async (route) => {
    await route.fulfill({ status: 401 });
  });
  const documents: string[] = [];
  page.on("request", (request) => {
    if (request.isNavigationRequest() && request.frame() === page.mainFrame())
      documents.push(request.url());
  });
  await page.goto("/login");
  await page.getByLabel("Email", { exact: true }).fill("alice@example.com");
  await page.getByLabel("Password", { exact: true }).fill("wrong-password-1");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveText("Incorrect email or password.");
  await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeEnabled();
  await expect(page.getByLabel("Email", { exact: true })).toHaveValue("alice@example.com");
  await expect(page.getByLabel("Password", { exact: true })).toHaveValue("wrong-password-1");
  await expect(page).toHaveURL(/\/login$/);
  expect(documents).toHaveLength(1);
});

test("an unmatched URL boots the branded not-found page", async ({ page }) => {
  await authenticated(page);
  await page.goto("/definitely-not-a-route");
  await expect(page.getByRole("heading", { name: "Not found" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Back to library" })).toHaveAttribute(
    "href",
    "/library",
  );
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
