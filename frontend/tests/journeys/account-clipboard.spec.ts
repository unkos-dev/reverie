import { expect, test } from "@playwright/test";

test("credential links remain selectable when the native clipboard API is unavailable", async ({
  page,
  baseURL,
}) => {
  const origin = "http://reverie.test";
  const userId = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
  await page.route(`${origin}/**`, async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    let body: unknown;
    if (path === "/auth/me")
      body = {
        id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        display_name: "Alice",
        email: "alice@example.com",
        role: "admin",
        is_child: false,
        has_local_password: true,
        theme_preference: "dark",
        csrf_token: "synthetic-clipboard-csrf",
      };
    else if (path === "/auth/setup/status")
      body = {
        setup_required: false,
        local_auth_enabled: true,
        oidc_enabled: false,
        email_recovery_enabled: false,
        password_min_length: 15,
        password_max_length: 256,
      };
    else if (path === "/api/v1/account/email-verifications")
      body = {
        email: "alice@example.com",
        email_verified: false,
        mail_enabled: false,
      };
    else if (path === "/api/v1/users")
      body = [
        {
          id: userId,
          display_name: "Bob",
          email: null,
          role: "adult",
          is_child: false,
          disabled: false,
          has_password: true,
          credential_link_expires_at: null,
          created_at: "2026-05-25T00:00:00Z",
          updated_at: "2026-05-25T00:00:00Z",
        },
      ];
    else if (path === `/api/v1/users/${userId}/credential-links`)
      body = {
        token: "test-link",
        expires_at: "2026-10-28T12:00:00Z",
      };
    else if (path === "/api/v1/shelves") body = { items: [], next_cursor: null };
    else if (path.startsWith("/api/") || path.startsWith("/auth/")) {
      throw new Error(`Unexpected clipboard fixture request: ${path}`);
    } else {
      const response = await page.request.get(new URL(`${path}${url.search}`, baseURL).href);
      await route.fulfill({ response });
      return;
    }
    await route.fulfill({ json: body });
  });
  await page.goto(`${origin}/admin/users`);
  expect(await page.evaluate(() => isSecureContext)).toBe(false);
  expect(await page.evaluate(() => navigator.clipboard)).toBeUndefined();
  await page.getByRole("button", { name: "Reset password", exact: true }).click();
  await page.getByRole("button", { name: "Create link", exact: true }).click();
  await page.getByRole("button", { name: "Copy link", exact: true }).click();
  await expect(
    page.getByText("Couldn't copy the link. Select the link and copy it manually."),
  ).toBeVisible();
  await expect(page.getByLabel("Credential link")).toHaveValue(
    `${origin}/reset-password#test-link`,
  );
  await expect(page.getByText("Link copied.", { exact: true })).toHaveCount(0);
});
