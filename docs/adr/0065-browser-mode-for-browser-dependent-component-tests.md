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

The foundation uses the supported Playwright provider and its directly declared Playwright peer, aligned with the Vitest
version owned by Vite+. The durable Chromium component project has an isolated React and Tailwind host that loads
application CSS, with browser matcher types separated from app and Node declarations. Native browser behaviour replaces
geometry, focus and browser API emulation in these component tests.

Jsdom coexistence is temporary transition coverage. Superseded jsdom cases are removed as equivalent required outcomes
are verified in their appropriate environment: browser-dependent components in Browser Mode, pure logic in Node and
actual document navigation in standalone journeys. Jsdom and its global stubs are removed when no required assertion
depends on them. This does not require an all-at-once conversion or permanent duplicate suites.

Actual navigation remains a separate journey boundary. The existing three jsdom logout intent checks remain until
appropriate replacements are verified; standalone logout journeys are not implemented by this foundation. Component
tests do not certify application bootstrap, CSP enforcement, session invalidation or end-to-end navigation. Application
CSP plugins, headers, proxy and HTML processing retain their separate verification, as does the accessibility gate.

Local and CI execution share the test recipe. CI provisions the matching Chromium binary and operating-system
dependencies, caches browsers by operating system, architecture and Playwright version, and retains available failure
screenshots. Operational commands and host limitations live in the
[frontend testing guidance](../../frontend/README.md#testing).

### Consequences

- Positive: browser-dependent assertions observe real layout, focus, scrolling and input with application styles.
- Positive: supported Vite+ and Vitest integration keeps component testing within the existing toolchain.
- Positive: incremental migration preserves required coverage while giving pure logic a Node home.
- Negative: browser binaries and system libraries add provisioning, runtime and memory costs; CI capacity needs its own
  measurement.
- Negative: provider, runner and browser dependency alignment becomes part of test maintenance.
- Negative: temporary coexistence carries duplicate coverage and two environments until replacements are verified.
- Negative: embedded component execution cannot cover document navigation, leaving journey work outside this foundation.

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

- [Vite+ toolchain decision](0033-adopt-the-vite-vp-monorepo-toolchain.md) retains toolchain ownership.
- [Playwright accessibility decision](0040-accessibility-gate-and-render-verification-on-playwright.md) retains the
  separate accessibility and render-verification boundary.
- [Vite+ Vitest integration](https://viteplus.dev/guide/migrate#vitest) documents the bundled runner and provider
  import.
- [Vitest Browser Mode](https://vitest.dev/guide/browser/) documents providers, embedded test execution and headless
  mode.
- [Playwright browser provisioning](https://playwright.dev/docs/browsers) documents browser binaries and system
  dependencies.
