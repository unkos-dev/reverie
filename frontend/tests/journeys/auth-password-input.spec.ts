import { expect, test } from "@playwright/test";

for (const kind of ["setup", "account", "link"] as const) {
  test(`${kind} password input preserves pasted Unicode code points`, async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.route("**/auth/setup/status", async (route) => {
      await route.fulfill({
        json: {
          setup_required: kind === "setup",
          local_auth_enabled: true,
          oidc_enabled: false,
          email_recovery_enabled: true,
          password_min_length: 15,
          password_max_length: 256,
        },
      });
    });
    await page.route("**/auth/me", async (route) => {
      await route.fulfill({
        json: {
          id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
          display_name: "Alice",
          email: "alice@example.com",
          role: "admin",
          is_child: false,
          has_local_password: true,
          theme_preference: "system",
          csrf_token: "synthetic-csrf",
        },
      });
    });
    const token = "A".repeat(43);
    await page.route("**/auth/credential-links/lookups", async (route) => {
      await route.fulfill({ json: { email: "alice@example.com", has_password: false } });
    });
    const scenario = {
      setup: {
        endpoint: "/auth/setup",
        path: "/setup",
        label: "Password",
        button: "Create administrator",
      },
      account: {
        endpoint: "/api/v1/account/password",
        path: "/account/password",
        label: "New password",
        button: "Change password",
      },
      link: {
        endpoint: "/auth/credential-links/redemptions",
        path: `/set-password#${token}`,
        label: "New password",
        button: "Set password",
      },
    }[kind];
    const submissions: unknown[] = [];
    await page.route(`**${scenario.endpoint}`, async (route) => {
      submissions.push(route.request().postDataJSON());
      await route.fulfill({
        status: 422,
        json: {
          type: "about:blank",
          title: "Validation Error",
          status: 422,
          detail: "Password refused.",
        },
      });
    });
    await page.goto(scenario.path);
    if (kind === "setup") {
      await page.getByLabel("Display name", { exact: true }).fill("Alice");
      await page.getByLabel("Email", { exact: true }).fill("alice@example.com");
    }
    if (kind === "account") await page.getByLabel("Current password", { exact: true }).fill("old");
    const password = page.getByLabel(scenario.label, { exact: true });
    const submit = page.getByRole("button", { name: scenario.button });
    for (const length of [14, 15]) {
      const value = "🙂".repeat(length);
      await page.evaluate(async (text) => {
        await navigator.clipboard.writeText(text);
      }, value);
      await password.fill("");
      await password.click();
      await page.keyboard.press("ControlOrMeta+V");
      await expect(password).toHaveValue(value);
      if (kind === "link") {
        const confirm = page.getByLabel("Confirm password", { exact: true });
        await confirm.fill("");
        await confirm.click();
        await page.keyboard.press("ControlOrMeta+V");
        await expect(confirm).toHaveValue(value);
      }
      await submit.click();
      if (length === 14) {
        await expect(password).toHaveAccessibleDescription(/Use at least 15 characters\./);
        await expect(password).toBeFocused();
        expect(submissions).toEqual([]);
      } else {
        await expect(page.getByRole("alert")).toHaveText("Password refused.");
        await expect(password).toHaveValue(value);
        const expected = {
          setup: { email: "alice@example.com", display_name: "Alice", password: value },
          account: { current_password: "old", new_password: value },
          link: { token, password: value },
        }[kind];
        expect(submissions).toEqual([expected]);
      }
    }
  });
}
