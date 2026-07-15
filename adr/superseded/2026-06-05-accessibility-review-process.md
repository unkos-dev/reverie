---
status: "superseded"
date: 2026-06-05
supersedes: []
superseded-by: ["../2026-07-13-a11y-gate-on-playwright.md"]
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Accessibility review process: automated axe gate + manual audit cadence

## Context and Problem Statement

`frontend/PRODUCT.md` § Accessibility declares **WCAG 2.2 Level AA a design
invariant**, and `frontend/DESIGN.md` §2 names specific rules (1.4.3, 1.4.11)
and one deliberate, accepted carve-out (Reverie Gold on light surfaces,
restricted to focus rings, large CTAs, and recovery actions). Until now nothing
enforced any of this: no automated check, no defined manual cadence, no
PR-time prompt. The invariant could drift from the shipped UI undetected.

How should accessibility be enforced so the invariant is load-bearing rather
than aspirational: what runs automatically, what stays a human audit, when
does the human audit happen, and how is the one accepted carve-out documented
so the gate does not mask it?

## Decision Drivers

- The gate must be reproducible by contributors, not just in CI. The dev
  workspace is ARM64.
- Automated tooling cannot catch everything a11y (keyboard order,
  screen-reader semantics, focus management, motion budget), the process must
  name what is left to humans, and when.
- The accepted gold carve-out must be expressed precisely enough that the gate
  still fails on genuine misuse (e.g. small-text gold on a non-CTA surface).
- Tooling must fit the existing verification-stack model (hard rule 8: stack
  binaries land in the workspace image, pinned, no `command -v` fallback).

## Considered Options

- **agent-browser (CDP) + axe-core, with a documented allowlist**: drive a
  Chromium-family browser over CDP, inject axe-core, gate on the result.
- **`@axe-core/cli`**: the off-the-shelf axe CLI (selenium + chromedriver).
- **`@axe-core/playwright`**: axe on top of Playwright-managed browsers.
- **Manual audits only**: no automated gate.

## Decision Outcome

Chosen option: **agent-browser + axe-core with a documented allowlist, plus a
defined manual-audit cadence**, because it is the only automated option that
runs on both the ARM64 dev workspace and x86 CI without new browser-driver
infrastructure, and it reuses a verification-stack binary already in the
workspace image.

Concretely:

- **Automated gate.** A CI job (`a11y`) runs axe-core against the dev-only
  design showcase under `npm run dev`, using the **full WCAG 2.2 AA tag set**
  (`wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa`, `wcag22aa`). It fails on any
  violation outside the documented allowlist. It is frontend-conditional, so
  docs/backend-only PRs skip it. Locally: `npm run a11y`.
- **Accepted carve-out.** The single allowlisted violation is `color-contrast`
  on **large CTAs** (primary `data-slot="button"` `data-size="lg"`), per
  DESIGN.md §2. The allowlist (`frontend/scripts/a11y/allowlist.mjs`) matches on
  element role read from the node HTML, **not** background colour, because the
  permitted button and the (then non-permitted) default badge rendered the
  identical gold background; bg colour cannot reliably separate allow from deny.
  The badge contrast has since been corrected, but role-keyed matching stays
  the correct shape. Every allowlist entry carries an inline rationale.
- **Manual audit cadence.** A manual a11y pass is run **at every release tag**
  and **before any net-new view ships**. Any team member may perform and sign
  off the audit; a designated a11y reviewer is optional.
- **Tooling boundary.** axe catches contrast, accessible names, roles, and
  structural rules. The manual audit owns what axe cannot: keyboard navigation
  order, screen-reader semantics, focus management in dialogs/overlays, and the
  motion budget under `prefers-reduced-motion`.
- **Mitigation log.** Accepted violations live only in
  `frontend/scripts/a11y/allowlist.mjs`, each with a rationale comment pointing
  at DESIGN.md §2. Adding an entry is an accessibility exception a reviewer must
  approve; it is not a way to silence the gate.

### Consequences

- Good, because the WCAG 2.2 AA invariant is now enforced on every
  frontend-touching PR and reproducible locally on ARM64.
- Good, because the carve-out is explicit and role-keyed, so the gate still
  fails on genuine misuse (the default Badge variant's cream-on-gold contrast
  failed the gate rather than being masked (role-keyed matching is detailed in the accessibility fixes, fixed in PR #434).
- Bad, because the gate currently covers only the design showcase; post-login
  views (Home/Library/Detail) need an authenticated session and are not yet
  scanned. They are added to the run targets as that becomes possible.
- Neutral, because the runner depends on `agent-browser` being present (image
  binary locally; pinned `npm i -g` in CI), consistent with the verification
  stack.

### Confirmation

Enforced by the CI `a11y` job: `npm run a11y` fails (exit non-zero) on any WCAG
2.2 AA violation whose nodes are not covered by
`frontend/scripts/a11y/allowlist.mjs`. The runner also asserts the scan
ran (axe testEngine present, URL matched, non-trivial passes/inapplicable) and
fails closed otherwise, so an empty result from a crashed browser or blank page
cannot be mistaken for "0 violations".

## Pros and Cons of the Options

### agent-browser (CDP) + axe-core

- Good, because one mechanism runs on ARM64 (Brave) and x86 CI (Chromium); no
  chromedriver, no Playwright.
- Good, because agent-browser is already a verification-stack binary.
- Neutral, because the allowlist/verdict layer is first-party code (small, pure,
  unit-tested).

### `@axe-core/cli`

- Bad, because its bundled `chromedriver` and Selenium Manager are x64-only ELF
  binaries: `exec format error` on the ARM64 workspace, and Chrome-for-Testing
  has no linux-arm64 build (Chrome ARM64 Linux is not GA until ~Q2 2026). The
  gate would be CI-only and not locally reproducible.
- Bad, because its allowlisting is coarse (whole-rule disable / selector
  exclude), which would mask genuine violations alongside the carve-out.

### `@axe-core/playwright`

- Good, because Playwright ships ARM64 + x86 browsers.
- Bad, because it introduces a second browser-automation stack alongside the
  agent-browser the project already standardises on.

### Manual audits only

- Bad, because nothing prevents a contrast/role regression from shipping
  between audits; the invariant stays aspirational.

## More Information

- PR template (`.github/pull_request_template.md`) carries the a11y checklist
  that fires on UI-touching PRs (keyboard nav, focus visibility, 1.4.11/1.4.3,
  reduced motion, tokens-not-hex, alarm carve-out).
- The `wcag22aa` tag in axe selects only the rules _new_ in WCAG 2.2 (e.g.
  `target-size`): it does **not** include `color-contrast`. Narrowing the gate
  to `wcag22aa` alone would make it pass trivially; the full AA tag set is
  required and is hard-coded in the runner.
- Rollout: the gate shipped **advisory** (`continue-on-error` on the `a11y`
  job's gate step) while the showcase baseline contained one known, deliberately
  un-allowlisted violation (the default Badge contrast). This violation shipped
  (PR #434) and the gate passes, so `continue-on-error` was removed and the gate
  now **blocks**. The `### Confirmation` invariant (fails on non-allowlisted
  violations) holds at both the runner and CI-gate level.
- Revisit trigger: when Chrome for ARM64 Linux ships (and Chrome-for-Testing
  publishes a linux-arm64 driver), re-evaluate whether `@axe-core/cli` becomes
  viable as a simpler off-the-shelf replacement for the first-party runner.
- Related: `frontend/DESIGN.md` §2 (Light-Gold Restriction Rule),
  `frontend/PRODUCT.md` § Accessibility, the accessibility review process work,
  and the default Badge contrast fix.
