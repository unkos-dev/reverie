---
type: ADR
profile-version: 1
id: "REV-ADR-0008"
title: "Adopt impeccable as the frontend design anti-pattern detector"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-21"
decision-makers:
  - "John Unkovich"
---

# Adopt impeccable as the frontend design anti-pattern detector

## Context and problem statement

Reverie's UI/UX agent toolset accreted three skills with overlapping remits: `ui-ux-pro-max` (catalog and shadcn MCP
bridge), `design-system` (audit and consistency review), and `frontend-patterns` (React patterns). The design system
itself is locked: Reverie has a published brand identity, D2 design tokens, shadcn primitives aliased onto the canonical
palette, and a tiered comment policy that codifies threat-aware docstrings on UI surfaces. Catalog-style skills produce
diminishing returns once the visual direction is settled, since their value front-loads on a greenfield project and then
drifts toward soft-everything scorecards ("7/10 on colour, 7/10 on spacing") that don't move work forward.

The maintainer ran a side-by-side empirical comparison on `frontend/src/pages/design/system.tsx` between:

- the `design-system` skill (existing), a 10-dimension scored audit
- `impeccable detect`, a deterministic CLI (`npx impeccable@2.1.9 detect`)
- `impeccable critique`, an LLM critique framework (applied manually)
- the `taste-skill` family (Leonxlnx/taste-skill, web and code anti-slop skills)

Outcomes:

- `design-system` produced a 10-line scorecard with vague references, no file:line, and no actionable diff.
- `impeccable detect` (deterministic) caught three `bg-black` instances in stock shadcn overlays (`alert-dialog`,
  `dialog`, `sheet`), direct violations of Reverie's tinted-neutral brand spec, with file:line and a fix recommendation
  per finding.
- `impeccable critique` (LLM) produced a 14-finding punch list with file:line and framework-grounded fixes (an em dash
  ban violation, nested cards, identical card grids, a missing alarm colour token in the showcase, and copy that leaks
  implementation detail).
- `taste-skill` overlaps `impeccable`'s territory and outputs prompts for external image generators (ChatGPT, Codex
  image mode), which don't fit the Claude-Code-first workflow.

The detector is the load-bearing finding: a deterministic, CI-runnable, no-LLM gate catches anti-patterns no LLM skill
surfaces reliably. That capability doesn't exist anywhere else in the toolset. Which tool, if any, should fill it?

## Decision drivers

- Empirical comparison on a real Reverie surface, not vendor claims.
- Deterministic, CI-runnable, and free of API cost: an axis orthogonal to the LLM skills already in use.
- Catches what the existing skills miss (the three `bg-black` findings).
- Compatible with the locked brand: the tool flags violations of Reverie's own tokens rather than imposing its own taste
  catalog.
- Husky, lint-staged, and Renovate are already wired, so the incremental cost is one devDependency, one lint-staged
  entry, and one CI step.

## Considered options

- Adopt `impeccable`.
- Keep the existing skills (`design-system`, `ui-ux-pro-max`, `frontend-patterns`).
- Adopt `taste-skill` (Leonxlnx).
- Roll a custom anti-pattern lint.

## Decision outcome

Chosen option: **adopt `impeccable`**, because the empirical comparison showed it is the only tool that catches concrete
anti-patterns the existing skills miss, at negligible wall-time cost, while respecting Reverie's locked brand rather
than imposing its own taste.

This decision governs the dependency itself; the skill side (the full impeccable command surface) is a separate
decision, to be made once the detector has earned its keep.

Only static source scanning is in scope; URL scanning and the skill command surface are separate choices.

### Consequences

- Positive: frontend anti-patterns are surfaced deterministically on every commit and every pull request.
- Positive: the CI signal is independent of LLM availability, running on `ubuntu-latest` in under two seconds.
- Positive: the install-script default-deny keeps the Chromium fetch out of a clean install and preserves install time.
- Negative: one more devDependency plus its transitive packages.
- Negative: detector rules are upstream-controlled, so rule churn could surface false positives mid-development.
- Negative: the LLM critique surface is opinionated and could conflict with the locked brand if invoked
  indiscriminately; mitigated by deferring the skill-side install to a separate decision.

## Pros and cons of the options

### Adopt `impeccable`

- Positive: a deterministic 27-rule detector catches anti-patterns the LLM skills miss (the three `bg-black` findings).
- Positive: a full scan runs in about one second and a single-file scan in about 400ms, well inside the pre-commit
  budget.
- Positive: ships as a standalone npm package, installable as a devDependency and Renovate-trackable.
- Neutral: the skill side (the full command surface) remains available later, as incremental adoption.
- Negative: one more devDependency plus its transitive packages.
- Negative: the optional puppeteer dependency pulls a Chromium download by default, mitigated by the install-script
  default-deny.
- Negative: the LLM critique surface is opinionated and could conflict with the locked brand if invoked
  indiscriminately; out of scope for this decision, which covers the detector only.

### Keep the existing skills (`design-system`, `ui-ux-pro-max`, `frontend-patterns`)

- Positive: no new dependency.
- Positive: already wired.
- Negative: the empirical comparison showed they produce soft scorecards rather than punch lists, missing all three
  `bg-black` findings, the em dash ban violation, and the nested-card pattern that `impeccable` surfaced.
- Negative: `ui-ux-pro-max`'s catalog is pre-brand-lock value; the locked brand renders it noise.
- Negative: `design-system` is subsumed by `impeccable critique` per the comparison.
- Negative: `frontend-patterns` covers generic React patterns, less specific than `frontend/CLAUDE.md`.

### Adopt `taste-skill` (Leonxlnx)

- Positive: image-generation reference boards (`brandkit`, `imagegen-frontend-web`).
- Positive: style variants for a sanity check (soft, minimalist).
- Negative: outputs prompts for external image generators (ChatGPT, Codex image mode), which don't fit the
  Claude-Code-first workflow.
- Negative: its anti-slop frontend skills overlap `impeccable`'s territory.
- Negative: twelve skills and dials add a high cognitive surface area.
- Negative: no CI-runnable component.

### Roll a custom anti-pattern lint

- Positive: total control over the rule set.
- Positive: could integrate directly with ESLint or Stylelint.
- Positive: no upstream-maintenance risk.
- Negative: re-invents `impeccable`'s 27 deterministic rules and its 12-rule LLM critique pass.
- Negative: maintenance cost without funding.
- Negative: greenfield rule-authoring is slower than consuming an off-the-shelf catalog and tightening from there.

## More information

Unlike paid-tool trials run under a fixed evaluation window, `impeccable` is a static development tool with no recurring
cost, so no trial period applies. Open a superseding record if any of the following happen:

- The detector becomes unreliable (a false-positive rate above 15% across a representative pull request sample); revisit
  rule selection or drop the tool.
- The tool stops being maintained upstream (no commits for 90 days while bugs accumulate); fork or drop it.
- An equivalent first-party tool ships (shadcn, Tailwind, or an ESLint plugin); consolidate onto it.
- Related decision:
  [Strict lint policy: pedantic Clippy and strict frontend lint](./0002-strict-lint-policy-pedantic-clippy-and-strict-frontend-lint.md),
  a sibling enforcement layer.
- Related decision:
  [Package ingress: global controls with named exceptions, merged on required checks](./0050-package-ingress-global-controls-with-named-exceptions-merged-on-required-checks.md),
  which governs the install-script denial this record relies on.
- Upstream: <https://github.com/pbakaus/impeccable> (Apache-2.0, forked from Anthropic's `frontend-design` skill).
