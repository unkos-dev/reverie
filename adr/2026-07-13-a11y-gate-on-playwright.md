---
status: "accepted"
date: 2026-07-13
supersedes: ["superseded/2026-06-05-accessibility-review-process.md"]
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Accessibility gate and render-verification on the Playwright stack

## Context and Problem Statement

The WCAG 2.2 AA accessibility gate runs axe-core against the dev-only design
showcase and fails on any violation outside a documented allowlist. It was built
on `agent-browser`, a first-party CDP runner, because the dev workspace is ARM64
and the off-the-shelf `@axe-core/cli` bundles an x64-only chromedriver. That same
ARM64 constraint also made `agent-browser` the interactive tool for
render-verification of UI work.

`agent-browser` is a single-vendor interim runner: the a11y gate carries a
bespoke CDP driver plus a hand-rolled dev-server start/poll/kill block, and the
render-verification path depends on the same binary. Standard, vendor-supported
browser automation now covers both needs on ARM64 and x86 alike, and the project
already standardises on it elsewhere.

Should browser automation for the accessibility gate and for frontend
render-verification stay on `agent-browser`, or move to the Playwright stack?

## Decision Drivers

- The gate must be reproducible on both the ARM64 dev box and x86 CI without a
  bespoke browser-driver.
- One browser-automation stack, not two: a second stack is maintenance surface
  and a skew risk.
- The documented gold carve-out must stay expressible precisely enough that the
  gate still fails on genuine misuse.
- Automated tooling cannot catch every accessibility concern (keyboard order,
  screen-reader semantics, focus management, motion budget); the process must
  still name what is left to a human audit and when.

## Considered Options

- **`@playwright/test` + `@axe-core/playwright`**: axe-core on Playwright-managed
  browsers, with the allowlist/verdict filter kept as first-party code.
- **Keep `agent-browser` + axe-core**: the prior first-party CDP runner.
- **`@axe-core/cli`**: the off-the-shelf axe CLI (selenium + chromedriver).

## Decision Outcome

Chosen option: **`@playwright/test` + `@axe-core/playwright` for the CI gate, with
`playwright-mcp` as the interactive render-verification tool**, retiring
`agent-browser`. Playwright resolves both original grounds for the CDP runner:

- **ARM64.** Playwright officially supports linux-arm64 and installs a
  linux-arm64 Chromium build (chrome-headless-shell, falling back to full
  Chromium on arm64), so the chromedriver / Chrome-for-Testing gap that blocked
  `@axe-core/cli` never applied to Playwright. The gate reproduces locally on
  ARM64 with `vp exec playwright install chromium` then `just js::a11y`, no
  browser substitution required.
- **Stack consolidation.** The consolidation argument now points the other way.
  When the prior decision was made, `agent-browser` was the standardised
  verification binary and Playwright would have been the second stack. Playwright
  is now the standard automation stack; keeping `agent-browser` is what maintains
  a second one.

Concretely:

- **Automated gate.** The CI `a11y` job runs `@axe-core/playwright` against
  `/forgot-password`, using the full WCAG 2.2 AA tag set (`wcag2a`, `wcag2aa`,
  `wcag21a`, `wcag21aa`, `wcag22aa`). Playwright's `webServer` owns the
  dev-server lifecycle. It fails on any violation outside the documented
  allowlist and is frontend-conditional. Locally: `just js::a11y`. The scan
  originally covered a dev-only design showcase; it was moved onto a real route
  so the gate judges what users receive.
- **Allowlist unchanged.** The accepted carve-outs and the role-keyed matching
  that expresses them are unchanged: violations from axe are filtered through the
  same first-party `allowlist.mjs` (matching on element role read from node HTML,
  not background colour), and every entry carries an inline rationale.
- **Render-verification.** Interactive "verify by render" checks move to
  `playwright-mcp` on the dev box. This is a tool choice for local verification,
  not a gate.
- **Manual audit cadence.** Unchanged: a manual accessibility pass runs at every
  release tag and before any net-new view ships, covering what axe cannot
  (keyboard order, screen-reader semantics, focus management, motion budget).

### Consequences

- Good, because the gate runs on a vendor-supported test runner with a single
  lifecycle owner, on ARM64 and x86, and the bespoke CDP driver plus hand-rolled
  server block are gone.
- Good, because failures ship a Playwright trace (DOM snapshots, console, and the
  attached axe results) instead of a bare results JSON.
- Good, because the project converges on one browser-automation stack.
- Neutral, because the allowlist/verdict layer stays first-party code (small,
  pure, unit-tested); only the driver changed.
- Bad, because the gate covers one route. `/login` and `/setup` each call
  `fetchSetupStatus` in a mount-time query, so against the Vite-only server this
  gate boots they render an error branch rather than their real markup; scanning
  them needs a backend. Post-login views need that and a stored-session fixture.
  Both are added to the run targets as that becomes possible.

### Confirmation

Enforced by the CI `a11y` job: `vp run a11y` (Playwright) fails on any WCAG 2.2
AA violation whose nodes are not covered by
`frontend/scripts/a11y/allowlist.mjs`. The spec also asserts the scan ran (axe
testEngine present, the scanned path matches the target, non-trivial
passes plus inapplicable) and fails closed otherwise, so an empty result from a
blank or crashed page cannot be mistaken for "0 violations".

## Pros and Cons of the Options

### `@playwright/test` + `@axe-core/playwright`

- Good, because Playwright ships ARM64 and x86 browsers, so the gate is locally
  reproducible on the dev box without a browser substitution.
- Good, because `webServer` owns the dev-server lifecycle, removing the
  hand-rolled start/poll/kill block.
- Good, because it is the standard automation stack, retiring the second one.
- Neutral, because the axe engine is unchanged (`@axe-core/playwright` tilde-pins
  the same `axe-core` minor the lockfile already resolved), so gate semantics are
  preserved.

### Keep `agent-browser` + axe-core

- Bad, because it keeps a bespoke first-party CDP runner and a duplicated
  dev-server lifecycle where a vendor-supported runner now suffices.
- Bad, because it maintains a second browser-automation stack alongside
  Playwright.

### `@axe-core/cli`

- Bad, because its bundled chromedriver and Selenium Manager are x64-only ELF
  binaries: `exec format error` on the ARM64 dev box, so the gate would be
  CI-only and not locally reproducible.
- Bad, because its allowlisting is coarse (whole-rule disable or selector
  exclude), which would mask genuine violations alongside the carve-out.

## More Information

- The `wcag22aa` tag selects only the rules new in WCAG 2.2 (for example
  `target-size`) and does not include `color-contrast`; narrowing the gate to it
  alone would make it pass trivially. The full AA tag set is required and is
  hard-coded in the spec.
- Related: `frontend/DESIGN.md` §2 (Light-Gold Restriction Rule),
  `frontend/PRODUCT.md` § Accessibility, and the PR template accessibility
  checklist that fires on UI-touching PRs.
- Revisit trigger: if a second Playwright spec (for example authenticated scan
  targets) is added, factor the `AxeBuilder` setup into the official
  `axe-test.ts` fixture; a single spec does not yet earn it.
