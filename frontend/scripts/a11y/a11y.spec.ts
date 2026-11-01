import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

import { parseTargets, urlMatches } from "./allowlist.mjs";

// WCAG 2.2 AA target needs the full ladder. wcag22aa ALONE selects only the
// rules new in 2.2 (e.g. target-size) and returns ZERO color-contrast findings,
// making the gate pass trivially. Do not narrow it.
const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

// Same env contract as the retired runner: comma-separated, root-relative
// paths. parseTargets fails closed on an empty list or an off-origin target.
// The default list lives in allowlist.mjs. Authenticated Home, Library, and
// Detail are the remaining gap: they need a storage-state fixture and a backend
// in this lane before they can join it.
const TARGETS = parseTargets(process.env.A11Y_TARGETS);

for (const target of TARGETS) {
  test(`WCAG 2.2 AA: ${target}`, async ({ page }, testInfo) => {
    await page.route("**/auth/setup/status", async (route) => {
      await route.fulfill({
        json: {
          setup_required: false,
          local_auth_enabled: true,
          oidc_enabled: false,
          email_recovery_enabled: true,
          password_min_length: 15,
          password_max_length: 256,
        },
      });
    });
    await page.goto(target);
    // Web-first readiness marker: every scannable target renders its content
    // inside a single top-level <main>, which is the bar a route clears before
    // it joins the list. Preferred over networkidle, which Playwright discourages.
    await expect(page.getByRole("main")).toBeVisible();
    if (target === "/forgot-password") {
      await expect(page.getByRole("textbox", { name: "Email", exact: true })).toBeVisible();
      await expect(page.getByRole("button", { name: "Send reset link" })).toBeVisible();
    } else if (target === "/verify-email" || target === "/reset-password") {
      await expect(page.getByRole("heading", { name: "This link can't be used" })).toBeVisible();
    }

    const results = await new AxeBuilder({ page }).withTags(WCAG_TAGS).analyze();

    // Liveness: an empty violation set from a blank/crashed page is
    // indistinguishable from "0 violations". Prove the scan genuinely ran
    // against the intended page before trusting a green result.
    const scannedPath = new URL(page.url()).pathname;
    expect(typeof results.testEngine.name, "axe did not report a testEngine").toBe("string");
    expect(urlMatches(scannedPath, target), `scanned "${scannedPath}" != target "${target}"`).toBe(
      true,
    );
    expect(
      results.passes.length + results.inapplicable.length,
      "0 passes and 0 inapplicable rules: blank or error page",
    ).toBeGreaterThan(0);

    // Attach the raw result BEFORE asserting, so a red run always ships the
    // full evidence (rule id, impact, help, per-node html + contrast data).
    // Replaces the old a11y-results.json artifact.
    await testInfo.attach("axe-results.json", {
      body: JSON.stringify(
        {
          target,
          url: scannedPath,
          testEngine: results.testEngine,
          counts: {
            violations: results.violations.length,
            passes: results.passes.length,
            inapplicable: results.inapplicable.length,
          },
          violations: results.violations,
        },
        null,
        2,
      ),
      contentType: "application/json",
    });

    expect(results.violations, "WCAG 2.2 AA violations (see attached axe-results.json)").toEqual(
      [],
    );
  });
}
