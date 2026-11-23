---
type: ADR
profile-version: 1
id: "REV-ADR-0040"
title: "Accessibility gate and render verification on Playwright"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-07-13"
decision-makers:
  - "John Unkovich"
---

# Accessibility gate and render verification on Playwright

## Context and problem statement

The WCAG 2.2 AA accessibility gate runs axe-core against frontend routes and fails on any violation outside approved
exceptions. It originally ran on `agent-browser`, a first-party CDP runner, because contributors on ARM64 hosts could
not run `@axe-core/cli`'s x64-only bundled chromedriver. That same ARM64 compatibility requirement also made
`agent-browser` the interactive tool for render verification of UI work.

`agent-browser` is an interim runner that requires maintaining a bespoke CDP driver and a hand-rolled dev-server
lifecycle block, and ties interactive render verification to the same binary. Standard, vendor-supported browser
automation covers both needs on ARM64 and x86 alike, and the project already standardises on Playwright elsewhere.

Should browser automation for the accessibility gate and for frontend render verification stay on `agent-browser`, or
move to the Playwright stack?

## Decision drivers

- The gate must be reproducible for contributors on ARM64 and x86 hosts as well as x86 CI without a bespoke browser
  driver.
- One browser-automation stack avoids maintenance overhead and skew between separate browser runners.
- Violations must fail cleanly with precise per-node evidence rather than coarse rule-wide exclusions.
- Automated tooling cannot catch every accessibility concern such as keyboard order, screen-reader semantics, focus
  management and motion budget; the process still names what a human audit covers.

## Considered options

- **`@playwright/test` + `@axe-core/playwright`**: axe-core on Playwright-managed browsers, with first-party target
  resolution and liveness checks.
- **Keep `agent-browser` + axe-core**: the prior first-party CDP runner.
- **`@axe-core/cli`**: the off-the-shelf axe CLI using Selenium and chromedriver.

## Decision outcome

Chosen option: **`@playwright/test` + `@axe-core/playwright` for the CI gate, with `playwright-cli` as the interactive
render-verification tool**, retiring `agent-browser`, because Playwright resolves both original grounds for the CDP
runner.

The automated accessibility gate runs Playwright with axe against product routes using the full WCAG 2.2 AA tag set.
Interactive render verification uses Playwright, while manual audits retain responsibility for keyboard, screen-reader,
focus and motion behaviour that axe cannot establish.

### Consequences

- Positive: the gate runs on a vendor-supported test runner with a single lifecycle owner, on ARM64 and x86, retiring
  the bespoke CDP driver and hand-rolled server lifecycle block.
- Positive: failures ship a Playwright trace with DOM snapshots, console output and attached axe results instead of a
  bare JSON report.
- Positive: the project converges on one browser-automation stack.
- Positive: target parsing and liveness checks remain small, pure first-party code.
- Negative: unauthenticated routes that depend on live backend state and authenticated post-login views require a
  backend and stored-session fixture before they can join the Vite-only gate.

## Pros and cons of the options

### `@playwright/test` + `@axe-core/playwright`

- Positive: Playwright ships ARM64 and x86 browsers, so contributors on ARM64 and x86 hosts can reproduce the gate
  locally without a browser substitution.
- Positive: `webServer` owns the dev-server lifecycle, removing the hand-rolled start/poll/kill block.
- Positive: standardises on the existing automation stack and retires the second runner.
- Neutral: the underlying axe engine is unchanged, so scan semantics stay consistent.

### Keep `agent-browser` + axe-core

- Negative: keeps a bespoke first-party CDP runner and duplicated dev-server lifecycle where a vendor-supported runner
  suffices.
- Negative: maintains a second browser-automation stack alongside Playwright.

### `@axe-core/cli`

- Negative: its bundled chromedriver and Selenium Manager are x64-only ELF binaries that fail with `exec format error`
  on ARM64 hosts, making the gate CI-only for ARM64 contributors.
- Negative: its exception mechanism relies on coarse whole-rule disables or selector excludes that can mask genuine
  violations.

## More information

- The `wcag22aa` tag selects only rules introduced in WCAG 2.2 such as `target-size` and omits `color-contrast`; the
  gate requires the full Level A and Level AA tag set (`wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa`, `wcag22aa`).
- Related: `frontend/DESIGN.md`, `frontend/PRODUCT.md` Accessibility section, and the pull request accessibility
  checklist.
- Revisit trigger: if authenticated scan targets add a second Playwright accessibility spec, factor the `AxeBuilder`
  setup into a shared Playwright fixture.
