import { defineConfig } from "@playwright/test";

import { DEFAULT_TARGETS } from "./scripts/a11y/allowlist.mjs";

// Playwright project scoped to the WCAG 2.2 AA accessibility gate only (one
// spec under scripts/a11y). Not a general e2e harness.
export default defineConfig({
  testDir: "./scripts/a11y",
  testMatch: "**/*.spec.ts",
  forbidOnly: !!process.env.CI,
  // axe results are deterministic; a retry would mask a real regression rather
  // than paper over flake. Pairs with trace: "retain-on-failure" (on-first-retry
  // would never fire at 0 retries).
  retries: 0,
  reporter: process.env.CI ? [["github"], ["list"]] : [["list"]],
  use: {
    baseURL: process.env.A11Y_BASE_URL ?? "http://localhost:5173",
    // Explicit desktop-first viewport (Playwright's default is 1280x720). No
    // standard mandates either; SC 1.4.10 reflow (320px) is deferred with the
    // multi-target work.
    viewport: { width: 1920, height: 1080 },
    trace: "retain-on-failure",
  },
  // A custom A11Y_BASE_URL means the caller owns the server (matches the old
  // runner, where the server lifecycle was always external); only manage the
  // local dev server for the default base.
  webServer: process.env.A11Y_BASE_URL
    ? undefined
    : {
        // --strictPort so a busy 5173 fails the run. Without it Vite drifts to
        // the next free port while Playwright keeps polling 5173 until its
        // timeout, which surfaces a port collision as a gate hang. Matches the
        // flag scripts/dev-server.sh passes for the same reason.
        command: "npm run dev -- --strictPort",
        // url (not the deprecated port) is polled for a 2xx, so pointing it at
        // a scanned route doubles as a route-exists readiness check. Derived
        // from DEFAULT_TARGETS so the probe cannot outlive the route it polls.
        url: `http://localhost:5173${DEFAULT_TARGETS[0]}`,
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
      },
});
