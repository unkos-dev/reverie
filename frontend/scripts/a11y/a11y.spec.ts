import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

import { filterAllowed, parseTargets, urlMatches } from "./allowlist.mjs";

// WCAG 2.2 AA target needs the full ladder. wcag22aa ALONE selects only the
// rules new in 2.2 (e.g. target-size) and returns ZERO color-contrast findings,
// making the gate pass trivially. Do not narrow it.
const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

// Same env contract as the retired runner: comma-separated, root-relative
// paths. parseTargets fails closed on an empty list or an off-origin target.
// Future targets (authed Home/Library/Detail) are added here as they become
// scannable.
const TARGETS = parseTargets(process.env.A11Y_TARGETS);

for (const target of TARGETS) {
  test(`WCAG 2.2 AA: ${target}`, async ({ page }, testInfo) => {
    await page.goto(target);
    // Web-first readiness marker: the design-system page renders its content
    // inside a single top-level <main>. Preferred over networkidle, which
    // Playwright discourages.
    await expect(page.getByRole("main")).toBeVisible();

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

    const remaining = filterAllowed(results.violations);

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
          remaining,
          violations: results.violations,
        },
        null,
        2,
      ),
      contentType: "application/json",
    });

    // toEqual prints the full remaining structure (id/impact/help/nodes) on
    // failure; the attachment carries the contrast ratios. No console needed.
    expect(
      remaining,
      "WCAG 2.2 AA violations outside the documented allowlist (see attached axe-results.json)",
    ).toEqual([]);
  });
}
