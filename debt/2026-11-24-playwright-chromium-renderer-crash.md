---
severity: medium
surfaces: [developer, ci]
adopted: 2026-11-24
adopted-because: the Chromium 159 renderer can crash during Vitest Browser Mode tests with Playwright 1.65.1
lift-when-class: dep-unblocks
lift-when: a Playwright release later than 1.65.1 passes the frontend browser suite with its bundled Chromium and no VITEST_BROWSER_CONNECTION_CLOSED renderer crashes
---

# Playwright rollback for Chromium renderer crashes

Playwright 1.65.1 bundles Chromium 159. A click can crash the renderer when it starts a view transition inside a frame
and interrupts the button's background colour transition. Vitest Browser Mode runs component tests in frames and reports
the disconnect as `VITEST_BROWSER_CONNECTION_CLOSED`.

The frontend pins `@playwright/test`, `playwright`, and `playwright-core` to 1.65.0, which bundles Chromium 158.
Renovate skips 1.65.1 and can still propose later releases.

To lift the restriction, upgrade all three packages together to a release later than 1.65.1. Install its bundled
browsers and run `just js::test`. Repeat `EditMetadataDialog.test.tsx` in the `components-chromium` project to check for
intermittent renderer crashes. If the suite and repeats pass without `VITEST_BROWSER_CONNECTION_CLOSED`, remove the
Renovate rule, this entry, and its README link with the upgrade.
