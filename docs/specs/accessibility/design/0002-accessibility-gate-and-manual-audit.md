---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0002"
title: "Accessibility gate and manual audit"
satisfies:
  - "REV-REQ-0004"
governed-by:
  - "REV-ADR-0040"
---

# Accessibility gate and manual audit

This Design covers how Reverie checks its web surfaces against the WCAG 2.2 Level AA obligation: the automated
Playwright and axe-core gate and the manual audit that covers what the automated gate cannot.

## Purpose and boundaries

This subject owns verifying WCAG 2.2 Level AA conformance for Reverie's web surfaces: the automated scan that runs on
every UI-touching pull request and the recurring manual audit. It does not own the design tokens, component markup, or
colour system that make a surface conformant in the first place; those live in `frontend/DESIGN.md` and the component
implementations themselves. It does not own the WCAG 2.2 standard's own rule definitions, which `@axe-core/playwright`
evaluates against.

Depends on: `@playwright/test` and `@axe-core/playwright` as the browser-automation and axe-core integration; the
frontend Vite dev server, whose lifecycle Playwright's `webServer` configuration owns for the gate run; the `changes`
job in `.github/workflows/frontend.yml`, which decides whether a pull request touches the frontend at all.

Depended on by: every pull request that touches the frontend, through the CI `a11y` job; every release tag and every
net-new view, through the manual audit; a contributor running the gate locally before pushing.

## Structure

- `frontend/scripts/a11y/a11y.spec.ts` is the Playwright test file. It registers one test per scan target, drives
  `@axe-core/playwright`'s `AxeBuilder` against each, and asserts the gate's verdict.
- `frontend/scripts/a11y/allowlist.mjs` is a pure, side-effect-free module with no Playwright dependency: it holds the
  `DEFAULT_TARGETS` scan-target list, parses targets (`parseTargets`), and checks that a scanned URL matches its
  intended target (`urlMatches`).
- The `just js::a11y` recipe and the CI `a11y` job in `.github/workflows/frontend.yml` are the two run sites. Both
  ultimately execute the Playwright test file; CI additionally installs and caches a Playwright Chromium browser and
  uploads the Playwright trace directory on failure.
- The manual audit has no code component: it is a human accessibility pass performed against the running application.

## Interfaces and dependencies

- `@axe-core/playwright`'s `AxeBuilder` is the scan engine: `a11y.spec.ts` constructs one per test, restricts it to the
  WCAG tag set (`wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa`, `wcag22aa`), and calls `.analyze()` to obtain a
  `violations`/`passes`/`inapplicable`/`testEngine` result.
- The `A11Y_TARGETS` environment variable is the scan-target contract: a comma-separated list of root-relative paths,
  parsed by `parseTargets`. An unset variable falls back to `allowlist.mjs`'s `DEFAULT_TARGETS`: `/forgot-password`,
  `/verify-email`, and `/reset-password`. A setup-status response fixture enables the recovery form against the
  Vite-only server. Fragment-free verification and reset routes render their invalid-link states. Authenticated Home,
  Library, and Detail views need both a backend and a stored-session fixture the gate does not yet supply.
- Playwright's own `webServer` configuration starts and stops the dev server for the run; the gate does not start a
  second server, and CI's `a11y` job step comment records that a second `vp dev` would collide with it on port 5173.
- `just js::a11y` is the local entry point; the CI `a11y` job in `.github/workflows/frontend.yml` is the entry point in
  the pipeline, gated on the `changes` job's `frontend` output so a docs- or backend-only pull request skips it.

## Data and state

- `DEFAULT_TARGETS` in `allowlist.mjs` is the current scan-target list, described above.
- The `A11Y_TARGETS` environment variable, when set, overrides `DEFAULT_TARGETS` for that run; CI does not set it, so CI
  always scans `DEFAULT_TARGETS`.

## Runtime behaviour

For each target path in `parseTargets(process.env.A11Y_TARGETS)`, `a11y.spec.ts` runs one test:

1. Fulfil `/auth/setup/status` with the recovery-enabled configuration and navigate to the target path.
2. Wait for a top-level `main` landmark to become visible. Confirm the recovery form's email field and submit button, or
   the invalid-link heading for verification and reset routes.
3. Run `AxeBuilder` with the full WCAG tag set and collect the result.
4. Check liveness before trusting the result: `testEngine.name` is present, the scanned `pathname` matches the intended
   target (`urlMatches`, tolerant of a trailing-slash mismatch between the two), and the sum of `passes` and
   `inapplicable` is greater than zero. Any of these failing means the page did not render as scanned, and the test
   fails regardless of the violation count.
5. Attach the raw violations and scan counts as an `axe-results.json` test artifact before asserting, so a failing run
   carries the rule id, impact, help text, and per-node HTML and contrast data.
6. Assert the raw `violations` list is empty; any violation fails the test. No findings are filtered or suppressed.

## Failure and recovery

- **An axe violation.** The test fails with the raw violations printed in the assertion message and attached as
  `axe-results.json`; CI additionally uploads the Playwright trace directory (`frontend/test-results/`) as a workflow
  artifact on failure.
- **A verified axe false positive.** The finding and manual assessment evidence for the affected success criterion are
  retained. This is a scanner defect; a genuine WCAG failure is a product defect.
- **A crashed browser or blank/wrong page.** The liveness checks in step 4 fail closed: an empty `violations` array from
  a page that never rendered is indistinguishable from a genuinely clean scan unless the test also confirms axe ran
  (`testEngine` present), against the intended page (`urlMatches`), with a non-trivial rule set applied
  (`passes + inapplicable > 0`). Any of these failing fails the test even when `violations` is empty.
- **`A11Y_TARGETS` resolving to zero targets.** `parseTargets` throws at spec-collection time rather than registering no
  tests, since a suite with no tests would otherwise pass without scanning anything.
- **An off-origin or malformed target.** `parseTargets` throws when an entry does not start with a single `/`, since
  `page.goto` would otherwise leave the configured base URL and scan an unrelated origin.
- **A design change that introduces a contrast failure.** A colour or role change that pushes a previously conformant
  surface into a contrast failure is caught by the gate the same as any other regression: the change fails the gate and
  needs a design fix.

## Security and operations

The gate has no credential or secret surface: it scans a locally booted dev server with no authenticated session. Its
operational surface is the CI job (frontend-conditional, 20-minute timeout, Playwright Chromium cached by runner OS,
architecture, and Playwright version) and the local `just js::a11y` recipe, which a contributor runs on ARM64 or x86
without a browser substitution because Playwright installs a matching Chromium build for either. Troubleshooting entry
point on a red run: the attached `axe-results.json` (rule id, impact, per-node contrast data) locally, or the same
attachment plus the uploaded Playwright trace directory in CI.

The manual audit is the operational control the automated gate does not replace: it runs at every release tag and before
any net-new view ships, and any team member may perform and sign off the pass. It covers every surface, including routes
that the gate scans. On scanned routes, it assesses the applicable criteria and behaviours that automated checks cannot
fully establish; on unscanned surfaces, it assesses the full applicable Level A and Level AA criteria. This includes
keyboard navigation order, screen-reader semantics, focus management in dialogs and overlays, and the motion budget
under `prefers-reduced-motion`. The automated gate supplies evidence for the colour contrast, accessible name, role, and
structural rules it evaluates, rather than proof of complete conformance on those routes.

## More information

- The `wcag22aa` axe tag selects only the rules new in WCAG 2.2 (for example `target-size`) and excludes
  `color-contrast`; the gate hard-codes the full AA tag set rather than that tag alone.
