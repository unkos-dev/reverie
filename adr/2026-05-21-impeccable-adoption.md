---
status: accepted
date: 2026-05-21
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Adopt `impeccable` as the frontend design anti-pattern detector

## Context and Problem Statement

Reverie's UI/UX agent toolset accreted three skills with overlapping
remits: `ui-ux-pro-max` (catalog + shadcn MCP bridge),
`design-system` (audit + consistency review), and
`frontend-patterns` (React patterns). The design system itself is
locked: Reverie has a published brand identity, D2 design tokens,
shadcn primitives aliased onto the canonical palette, and a tiered
comment policy that codifies threat-aware docstrings on UI
surfaces. Catalog-style skills produce diminishing returns once the
visual direction is settled, as their value front-loads on a
greenfield project, then drifts toward soft-everything scorecards
("7/10 on color, 7/10 on spacing") that don't move work forward.

On 2026-05-21 the maintainer ran a side-by-side empirical
comparison (`plans/2026-05-21-ui-tooling-comparison.md`,
gitignored) on `frontend/src/pages/design/system.tsx` between:

- `design-system` skill (existing), 10-dimension scored audit
- `impeccable detect` deterministic CLI (`npx impeccable@2.1.9 detect`)
- `impeccable critique` LLM critique framework (manual application)
- `taste-skill` family (Leonxlnx/taste-skill, web/code anti-slop skills)

Outcomes:

- `design-system` produced a 10-line scorecard with vague refs.
  Soft-everything; no file:line, no actionable diff.
- `impeccable detect` (deterministic) caught 3× `bg-black` in stock
  shadcn overlays (`alert-dialog`, `dialog`, `sheet`), direct
  violations of Reverie's tinted-neutral brand spec, file:line +
  fix recommendation per finding.
- `impeccable critique` (LLM) produced a 14-finding punch list with
  file:line, framework-grounded fixes (em-dash ban violation,
  nested cards, identical card grids, missing alarm color token in
  showcase, copy that leaks implementation detail).
- `taste-skill` overlaps `impeccable`'s territory; outputs prompts
  for _external_ image generators (ChatGPT, Codex image mode) which
  don't fit the Claude-Code-first workflow. Deferred 30 days.

The detector is the load-bearing finding: a deterministic,
CI-runnable, no-LLM gate catches anti-patterns no LLM skill
surfaces reliably. That capability doesn't exist anywhere else in
the toolset.

## Decision

Adopt `impeccable` (v2.1.9, MIT/Apache-2.0) as a frontend dev
dependency. Wire its deterministic detector into the existing
pre-commit/lint-staged chain plus the frontend CI job. Static-scan
mode only (URL-scan deferred; see Future Plan).

This ADR governs the dependency itself. The skill side (full
impeccable command surface) lands in a separate PR with its own
ADR-or-amendment after the detector earns its keep.

### Adoption posture

- **Static scan only.** Command form: `impeccable detect src`.
  Operates on file content, never reaches the puppeteer code path
  (which is dynamically imported inside the URL-only `detectUrl()`
  function; see Future Plan for the runtime evidence).
- **Pre-commit (lint-staged)** triggers full-scan when any staged
  path matches `frontend/src/**/*.{ts,tsx,html,css}`. Function form
  used so the same `vp run --filter frontend detect` runs locally
  and in CI. Wall-time delta vs single-file scan ~400ms.
- **CI** runs the same `vp run --filter frontend detect` as a new step in the
  frontend job between Stylelint and font-integrity.
- **Advisory both sides** (lint-staged `|| true` + CI
  `continue-on-error: true`) until the 3 deferred `bg-black`
  findings are addressed. Strip both flags in the same follow-up.
- **Renovate auto-tracks** the package via the existing
  `config:recommended` extension. Per the existing
  `matchCurrentVersion: "!/^0\\./"` rule in
  `.github/renovate.json`, pre-v1.0 bumps stay manual-review (no
  auto-merge); `impeccable` is currently at v2.1.9, so patch
  bumps auto-merge would apply once any rules drift to require it.
- **No Chromium download.** The root `package.json` denies puppeteer's
  install script under
  [2026-08-03-package-ingress-default-deny.md](2026-08-03-package-ingress-default-deny.md),
  so the postinstall Chromium fetch never runs. Our usage never
  invokes the puppeteer code path (see Future Plan for the
  dynamic-import evidence).

### Supply-chain stance

`impeccable` ships these runtime deps (lockfile, 2026-05-21):

- `jsdom@29.0.0`: HTML parsing for static scan. Required.
- `marked@^16.4.2`: markdown rendering for impeccable's skill
  surface (not used by the detector). Required transitively.
- `puppeteer@^24.42.0`: **optional**. Used only by
  `detectUrl()` (URL-scan). Dynamically imported. Top-level
  imports do not reach puppeteer.

Denying the install script drops the ~180MB postinstall fetch
without removing the puppeteer JS itself. The dynamic import inside
`detectUrl()` still resolves, so a URL-scan would fail at
`launch()` for want of a browser rather than at the import;
`npx puppeteer browsers install chrome` fetches one on demand if
URL-scan is ever wanted. Static path unaffected.

Corrected 2026-08-03: this record first named
`"puppeteer": { "skipDownload": true }` in `frontend/package.json`
as the mechanism. No manifest carried that field, so the fetch kept
running until the install-script denial landed. The alternatives
below were weighed against that field and stand as written.

Alternatives weighed at the time were rejected:

- `npm ci --omit=optional`: breaks `@tailwindcss/oxide`'s 12
  platform-binary `optionalDependencies` (standard npm
  platform-binary pattern). Tailwind build would fail.
- `PUPPETEER_SKIP_DOWNLOAD=true` env var on CI only, leaves the
  Chromium download firing on every developer's local `npm install`.
- Bake Chromium into the Coder workspace image: doesn't solve CI
  (GitHub-hosted runners), introduces puppeteer-vs-system-Chromium
  version drift, and pays the cost for a feature we don't run.

### Trial / revisit gate

Unlike the Greptile and CodeRabbit trials (4-week paid-tool
windows), `impeccable` is a static dev tool with no recurring cost.
No trial gate. Revisit conditions:

- Detector becomes unreliable (false-positive rate >15% across a
  representative PR sample) → revisit rule selection or drop
- Tool stops being maintained upstream (no commits for 90 days
  while bugs accumulate) → fork or drop
- An equivalent first-party tool ships (shadcn, Tailwind, ESLint
  plugin) → consolidate
- URL-scan trigger fires (see Future Plan) → amend this ADR or
  supersede with a new one

## Decision Drivers

- Empirical comparison on a real Reverie surface, not vendor claims
- Deterministic + CI-runnable + free (no API cost): orthogonal
  axis to LLM skills already in use
- Catches what existing skills miss (3× `bg-black` proof)
- Compatible with locked brand: tool flags violations of _our_
  tokens, doesn't impose its own taste catalog
- Husky + lint-staged + Renovate already wired: incremental cost
  of one devDep + one lint-staged entry + one CI step

## Considered Options

### Option 1: Adopt `impeccable` (chosen)

**Pros**:

- Deterministic 27-rule detector catches anti-patterns the LLM
  skills miss (3× `bg-black` proof)
- ~1s wall-time full scan; ~400ms single-file: fits inside
  lint-staged budget
- CLI ships as a standalone npm package: installable as devDep,
  Renovate-trackable
- Skill side available later (separate PR): incremental adoption
- URL-scan path exists upstream for future enable

**Cons**:

- One more devDep + ~53 transitive packages on `npm ci`
- Optional puppeteer pulls Chromium download by default (mitigated
  by `skipDownload: true`)
- LLM critique surface is opinionated: may conflict with locked
  brand if invoked indiscriminately (out of scope for this ADR;
  detector-only)

### Option 2: Keep existing skills (`design-system` + `ui-ux-pro-max` + `frontend-patterns`)

**Pros**:

- Zero new dependency
- Already wired

**Cons**:

- Empirical comparison showed they produce soft scorecards, not
  punch lists. Missed all 3 `bg-black` findings + the em-dash + the
  nested-card pattern that `impeccable` surfaced
- `ui-ux-pro-max` catalog is pre-brand-lock value; locked brand
  renders it noise
- `design-system` is subsumed by `impeccable critique` per
  comparison
- `frontend-patterns` is generic React patterns; less specific than
  `frontend/CLAUDE.md`

Rejected. Skill drops tracked separately (PR5 of the tooling
roadmap, dotfiles + local cleanup).

### Option 3: Adopt `taste-skill` (Leonxlnx)

**Pros**:

- Image-gen reference boards (`brandkit`, `imagegen-frontend-web`)
- Style variants for sanity-check (soft / minimalist)

**Cons**:

- Outputs prompts for external image generators (ChatGPT, Codex
  image mode); doesn't fit Claude-Code-first workflow
- Anti-slop frontend skills overlap impeccable's territory
- 12 skills + dials; high cognitive surface area
- No CI-runnable component

Deferred 30 days. Revisit only if specific gap surfaces post
`impeccable` adoption.

### Option 4: Roll custom anti-pattern lint

**Pros**:

- Total control over rule set
- Could integrate directly with ESLint / stylelint
- No upstream-maintenance risk

**Cons**:

- Re-inventing `impeccable`'s 27 deterministic rules + 12-rule LLM
  critique pass
- Maintenance cost without funding
- Greenfield rule-authoring is slow compared to consuming an
  off-the-shelf catalog and tightening from there

Rejected. Revisit if upstream `impeccable` becomes unreliable.

## Consequences

### Positive

- Frontend anti-patterns surfaced deterministically every commit +
  every PR
- 3× `bg-black` findings now visible in CI logs every frontend PR
  until the deferred fix lands, creating natural pressure toward addressing
  it on the first modal/dialog/sheet PR
- CI signal independent of LLM availability; runs on `ubuntu-latest`
  in <2s
- Lockfile prunes Chromium fetch via `puppeteer.skipDownload`,
  preserving CI install time
- Pattern set up for URL-scan enable (Future Plan) without rework

### Negative

- One more devDep + ~53 transitive packages on `npm ci`
- Detector rules are upstream-controlled; rule churn could surface
  false positives mid-development
- LLM critique surface tempting; risk of indiscriminate invocation
  conflicting with locked brand (mitigated by deferring skill-side
  install to a separate PR)

### Neutral

- Renovate handles version tracking; pre-v1.0 manual-review
  posture inherited (impeccable is currently v2.1.9, so this is
  moot; bumps would auto-merge under the stable-deps rule)

## Future Plan: URL-scan enablement

Static scan catches source-text violations (`bg-black` literals,
em-dash bans, regex-detectable anti-patterns). URL-scan catches
rendered-DOM-state violations no static path can see:

- **Computed colors after CSS-var resolution.** Tailwind v4 `@theme
inline` resolves OKLCH tokens at render time; static scan sees
  the token name, URL-scan sees the actual hex. Catches token
  mis-aliasing that renders as `#000` despite source looking clean.
- **Dual-theme variance.** Reverie ships Light + Dark themes via
  the existing `ThemeSwitcher`. URL-scan can toggle and verify both
  satisfy contrast / brand rules; static scan can't.
- **OKLCH chroma at extreme lightness.** Brand rule per impeccable's
  shared design laws and Reverie's own design philosophy spec
  (`plans/2026-04-25-design-system-philosophy-design.md`,
  `plans/2026-04-26-philosophy-spec-revision.md`).
- **Touch target dimensions post-fluid-sizing.**
- **Line length after responsive resolution** (the 65–75ch cap).
- **Anti-patterns only visible after JS interaction**: open
  dialog, hovered state, focused field, mounted Suspense boundary.
- **State-after-React-render checks.** Static scan sees JSX source,
  URL-scan sees what the user sees.

### Mechanism (when enabled)

`impeccable detect <URL>` launches headless Chromium via puppeteer,
navigates, waits for `networkidle0`, sets viewport 1280×800,
injects `detect-antipatterns-browser.js`, runs `window.impeccableScan()`,
collects findings, closes the browser. CI already passes
`--no-sandbox --disable-setuid-sandbox` automatically when `CI=true`
(observed in upstream source, 2026-05-21).

### Trigger conditions for URL-scan enable

Adopt URL-scan when **all** of the following are true:

1. **≥5 rendered Reverie surfaces shipped** (library hub, reader,
   settings, auth, ingestion status, etc.): the design system
   showcase doesn't count
2. **Design system stabilised**: no token churn for 30 consecutive
   days
3. **A target host exists in CI**: either ephemeral dev server
   boot inside the frontend job, or PR-preview deployment lane
   (currently neither; staging exists but isn't PR-scoped)

### When triggered

When all three conditions hold, amend this ADR or supersede with a
new one. The amendment/supersession will:

1. Remove `"puppeteer": { "skipDownload": true }` from
   `frontend/package.json`
2. Accept the ~180MB Chromium download on CI (~30s per run; or
   cache via GitHub Actions cache keyed on puppeteer version)
3. Add a new CI step that:
   - Boots `vp dev` in background OR pulls from PR-preview URL
   - Waits for server health
   - Runs `impeccable detect http://localhost:5173/<path>` per
     critical surface
   - Tears down dev server (if booted)
4. Decide local-dev posture (developers running URL-scan ad-hoc vs
   pre-push hook)

Documenting this future plan in the same ADR keeps the
adoption-vs-future-enable narrative in one place. It also gives
the next agent (human or otherwise) a clear "when to revisit"
signal rather than letting URL-scan enablement become an
unscheduled "we should do that sometime" backlog item.

## More Information

- Comparison artifact: `plans/2026-05-21-ui-tooling-comparison.md`
  (gitignored, local scratch per CLAUDE.md "Planning Artifact
  Locations" convention)
- Memory entries: `project_ui_tooling_comparison_decisions`,
  `project_bg_black_overlays_deferred`,
  `project_backend_local_validation_gap`
- Related: the backend pre-push hook task (surfaced during
  the same audit, separate scope)
- Related ADRs: `2026-05-03-strict-lint-policy.md` (sibling
  enforcement layer)
- Upstream: <https://github.com/pbakaus/impeccable> (Apache-2.0,
  forked from Anthropic's `frontend-design` skill)
