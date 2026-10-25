---
type: ADR
profile-version: 1
id: "REV-ADR-0065"
title: "Browser Mode for browser-dependent component tests"
status: "proposed"
recorded-on: "2026-10-25"
decision-makers:
  - "John Unkovich"
---

# Browser Mode for browser-dependent component tests

## Context and problem statement

Component assertions about focus, layout, scrolling and native input need browser behaviour that jsdom cannot fully
represent. Geometry and browser API stubs can exercise application branches without establishing that the interaction
works in a rendered page. Pure calculations do not need a browser, while document navigation can destroy the embedded
frame that hosts a Vitest component test.

The choice is which environment owns each kind of assertion without replacing the Vite+ toolchain or conflating
component coverage with application journeys and accessibility acceptance. Its scope is the bounded component-test
foundation, not full suite conversion, new product behaviour or new navigation journeys.

## Decision drivers

- Browser-dependent outcomes need native input, layout, focus and scrolling with application styles.
- Supported test-runner integration avoids maintaining a browser adapter or emulating geometry.
- Local and CI execution need aligned dependencies and reproducible browser provisioning.
- Required assertions need continuous coverage while the existing suite moves to appropriate environments.
- Component isolation, document navigation and application security have different verification boundaries.

## Considered options

- Vitest Browser Mode for browser-dependent components, with Node for pure logic and standalone browser journeys for
  document navigation.
- Retain jsdom for component assertions, with browser API stubs and separate browser coverage where needed.
- Move component assertions to standalone Playwright tests, with Node for pure logic.

## Decision outcome

Chosen option: **Vitest Browser Mode for browser-dependent components**, because it exercises native browser semantics
within the Vite+-owned test runner while keeping pure logic and document navigation in environments suited to them.

The chosen browser provider is Playwright, retaining supported integration with the Vitest version owned by Vite+. Node
remains the choice for pure logic because those assertions gain no browser semantics to justify the extra cost.
Standalone browser journeys remain the choice for actual document navigation because navigating the embedded component
frame can destroy its test runner.

Temporary jsdom coexistence avoids a coverage gap during migration. The chosen strategy retires jsdom coverage as its
required outcomes gain verified replacement homes, with jsdom itself retired once no required outcome depends on it.
This permits incremental adoption without making duplicate suites permanent.

### Consequences

- Positive: browser-dependent assertions observe real layout, focus, scrolling and input with application styles.
- Positive: supported Vite+ and Vitest integration keeps component testing within the existing toolchain.
- Positive: incremental migration preserves required coverage while giving pure logic a Node home.
- Negative: browser binaries and system libraries add provisioning, runtime and memory costs; CI capacity needs its own
  measurement.
- Negative: provider, runner and browser dependency alignment becomes part of test maintenance.
- Negative: temporary coexistence carries duplicate coverage and two environments until replacements are verified.
- Negative: embedded component execution cannot cover document navigation, leaving journey work outside this foundation.
- Negative: isolated component coverage does not establish application bootstrap, CSP enforcement, session invalidation
  or end-to-end navigation, and does not replace separate accessibility and render verification.

## Pros and cons of the options

### Retain jsdom for component assertions

- Positive: retains familiar, lightweight execution and the existing suite without browser provisioning for those tests.
- Positive: remains useful for assertions that do not depend on rendering or native browser behaviour.
- Negative: layout, scrolling and focus assertions can depend on fabricated browser behaviour, requiring separate
  browser coverage to establish the actual interaction.

### Move component assertions to standalone Playwright tests

- Positive: provides native browser automation and supports document navigation beyond an embedded component frame.
- Positive: application journeys can exercise bootstrap and integration boundaries that isolated components cannot.
- Negative: component assertions need their own mounting and fixture approach outside the existing Vitest component
  runner, while still paying browser provisioning and resource costs.
- Negative: using application journeys for every component assertion increases fixture and application setup coupling.

## More information

- [Frontend testing guidance](../../frontend/README.md#testing) covers operational setup and migration details.
- [Vite+ toolchain decision](0033-adopt-the-vite-vp-monorepo-toolchain.md) retains toolchain ownership.
- [Playwright accessibility decision](0040-accessibility-gate-and-render-verification-on-playwright.md) retains the
  separate accessibility and render-verification boundary.
- [Vite+ Vitest integration](https://viteplus.dev/guide/migrate#vitest) documents the bundled runner and provider
  import.
- [Vitest Browser Mode](https://vitest.dev/guide/browser/) documents providers, embedded test execution and headless
  mode.
- [Playwright browser provisioning](https://playwright.dev/docs/browsers) documents browser binaries and system
  dependencies.
