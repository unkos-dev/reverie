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

The WCAG 2.2 AA accessibility gate ran axe-core against the dev-only design showcase and failed on any violation outside
a documented allowlist. It was built on `agent-browser`, a first-party CDP runner, because contributors on ARM64 hosts
could not run `@axe-core/cli`'s x64-only bundled chromedriver. That same ARM64 compatibility requirement also made
`agent-browser` the interactive tool for render verification of UI work.

`agent-browser` is a single-vendor interim runner: the a11y gate carries a bespoke CDP driver plus a hand-rolled
dev-server start/poll/kill block, and the render-verification path depends on the same binary. Standard,
vendor-supported browser automation covers both needs on ARM64 and x86 alike, and the project already standardises on it
elsewhere.

Should browser automation for the accessibility gate and for frontend render verification stay on `agent-browser`, or
move to the Playwright stack?

## Decision drivers

- The gate must be reproducible for contributors on ARM64 and x86 hosts and on x86 CI without a bespoke browser driver.
- One browser-automation stack, not two: a second stack is maintenance overhead and a skew risk.
- The documented gold carve-out must stay expressible precisely enough that the gate still fails on genuine misuse.
- Automated tooling cannot catch every accessibility concern such as keyboard order, screen-reader semantics, focus
  management and motion budget; the process must still name what is left to a human audit and when.

## Considered options

- **`@playwright/test` + `@axe-core/playwright`**: axe-core on Playwright-managed browsers, with the allowlist/verdict
  filter kept as first-party code.
- **Keep `agent-browser` + axe-core**: the prior first-party CDP runner.
- **`@axe-core/cli`**: the off-the-shelf axe CLI using Selenium and chromedriver.

## Decision outcome

Chosen option: **`@playwright/test` + `@axe-core/playwright` for the CI gate, with `playwright-mcp` as the interactive
render-verification tool**, retiring `agent-browser`, because Playwright resolves both original grounds for the CDP
runner.

The automated accessibility gate uses Playwright with axe against a product route and the WCAG 2.2 AA rules. Interactive
render verification uses Playwright, while manual audits retain responsibility for keyboard, screen-reader, focus and
motion behaviour that axe cannot establish.

### Consequences

- Positive: the gate runs on a vendor-supported test runner with a single lifecycle owner, on ARM64 and x86, and the
  bespoke CDP driver plus hand-rolled server block are gone.
- Positive: failures ship a Playwright trace with DOM snapshots, console output and attached axe results instead of a
  bare results JSON.
- Positive: the project converges on one browser-automation stack.
- Positive: the allowlist/verdict layer is unaffected by the driver change; it stays small, pure, unit-tested
  first-party code.
- Negative: the gate covers one route. `/login` and `/setup` each call `fetchSetupStatus` in a mount-time query, so
  against the Vite-only server this gate boots they render an error branch rather than their real markup; scanning them
  needs a backend. Post-login views need that and a stored-session fixture. Both are added to the run targets as that
  becomes possible.

## Pros and cons of the options

### `@playwright/test` + `@axe-core/playwright`

- Positive: Playwright ships ARM64 and x86 browsers, so contributors on ARM64 and x86 hosts can reproduce the gate
  locally without a browser substitution.
- Positive: `webServer` owns the dev-server lifecycle, removing the hand-rolled start/poll/kill block.
- Positive: it is the standard automation stack, retiring the second one.
- Neutral: the axe engine is unchanged (`@axe-core/playwright` tilde-pins the same `axe-core` minor the lockfile already
  resolved), so gate semantics are preserved.

### Keep `agent-browser` + axe-core

- Negative: it keeps a bespoke first-party CDP runner and a duplicated dev-server lifecycle where a vendor-supported
  runner now suffices.
- Negative: it maintains a second browser-automation stack alongside Playwright.

### `@axe-core/cli`

- Negative: its bundled chromedriver and Selenium Manager are x64-only ELF binaries that fail with `exec format error`
  on ARM64 hosts, so the gate would be CI-only and not locally reproducible for ARM64 contributors.
- Negative: its allowlisting is coarse (whole-rule disable or selector exclude), which would mask genuine violations
  alongside the carve-out.

## More information

- The `wcag22aa` tag selects only the rules new in WCAG 2.2 such as `target-size` and does not include `color-contrast`;
  narrowing the gate to it alone would make it pass trivially. The full AA tag set is required and is hard-coded in the
  spec.
- Subsequent changes: interactive browser verification moved from `playwright-mcp` to `playwright-cli`, and automated
  violation suppression was removed. Current gate behaviour, scan targets and exception handling live in
  [Accessibility gate and manual audit](../specs/accessibility/design/0002-accessibility-gate-and-manual-audit.md).
- Related: `frontend/DESIGN.md`, `frontend/PRODUCT.md` Accessibility section, and the PR template accessibility
  checklist that fires on UI-touching PRs.
- Revisit trigger: if a second Playwright spec such as authenticated scan targets is added, factor the `AxeBuilder`
  setup into the official `axe-test.ts` fixture; a single spec does not yet earn it.
