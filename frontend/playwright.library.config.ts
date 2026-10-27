import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/journeys",
  testMatch: "*.spec.ts",
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [["github"], ["list"]] : [["list"]],
  outputDir: "./test-results/library",
  use: {
    baseURL: "http://127.0.0.1:5174",
    viewport: { width: 1280, height: 800 },
    colorScheme: "dark",
    locale: "en-AU",
    timezoneId: "Australia/Perth",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    { name: "library-chromium", use: { browserName: "chromium" } },
    { name: "library-firefox", use: { browserName: "firefox" } },
  ],
  webServer: {
    command: "just js::build && vp preview --host 127.0.0.1 --port 5174 --strictPort",
    url: "http://127.0.0.1:5174/library",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
