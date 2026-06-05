---
pr: 430
title: "feat(ci): add WCAG 2.2 AA accessibility gate (axe + agent-browser)"
author: junkovich
reviewed: 2026-06-05
recommendation: request-changes
---

# PR Review: #430 — feat(ci): add WCAG 2.2 AA accessibility gate (axe + agent-browser)

**Branch**: `feat/unk-268-a11y-axe-ci` → `main`
**Files Changed**: 16 (+1444 / -617)

---

## Summary

Solid, well-scoped implementation: the axe gate runs end-to-end and is now
**CI-validated on the x86 runner** (agent-browser installs + drives a browser
there — the one piece that couldn't be checked locally). The pure logic is
thoroughly unit-tested and the adversarial-review fixes (role discriminator,
fail-closed liveness) are all present. **One blocker:** the gate is wired as a
*blocking* check and correctly fails on the known Badge violation (UNK-345), so
this PR — and every subsequent frontend PR — cannot go green until the badge is
fixed. That rollout-sequencing needs a decision before merge.

---

## Implementation Context

| Artifact | Path |
|----------|------|
| Implementation Report | `.claude/PRPs/reports/unk-268-a11y-axe-ci-report.md` |
| Original Plan | `.claude/PRPs/plans/completed/unk-268-a11y-axe-ci.plan.md` |
| Documented Deviations | 3 (all valid: `.mjs`-not-`.ts`, PR-template-already-existed, `@ts-check` scoped) |

Deviations are well-documented and sound. The `.mjs` strategy (sidestepping the
typed-lint/tsconfig edit) is a legitimately simpler resolution of the plan's C1.

---

## Issues Found

### Critical

No critical issues.

### High Priority

**H1 — Blocking `a11y` job fails on the known Badge violation → PR (and all frontend PRs) unmergeable until UNK-345.**
`.github/workflows/ci.yml` (a11y job + `ci-gate` `needs`)

- **Observed**: CI run on this PR — `Accessibility (axe)` = **fail** (badge `.group/badge` 3.44:1), `ci-gate` aggregates it → required check red. The gate is doing exactly what it should; the problem is *sequencing*. As wired, no frontend-touching PR can merge green until the default Badge contrast is fixed (UNK-345), and this PR blocks itself.
- **Why it matters**: the acceptance criterion ("CI fails on violations") is met, but shipping it blocking with a pre-existing known failure on the baseline page bricks the merge queue for frontend work.
- **Fix options** (decision needed — touches the earlier "badge = separate issue" call):
  1. **Advisory-until-baseline-clean** (repo precedent): set `continue-on-error: true` on the gate step with a documented flip-to-blocking comment referencing UNK-345 — mirrors the existing `impeccable detect` step (`ci.yml:636`, "Flip to blocking once the known findings are addressed"). Lets this PR + frontend PRs merge now; flip to blocking when UNK-345 lands.
  2. **Land UNK-345 first**, then this PR is green as blocking from day one.
  3. **Fix the badge in this PR** (reverses the earlier scope decision).
- Recommendation: option 1 (advisory + flip-condition) — it matches an established repo pattern, keeps the gate visible immediately, and avoids a merge deadlock. Whichever is chosen is a maintainer call.

### Medium Priority

No medium issues.

### Suggestions

**S1 — Coverage includes the untestable runner at 0%.**
`frontend/vite.config.ts` — `coverage.include` adds `scripts/a11y/**`, which pulls
`axe-scan.mjs` (an agent-browser-driving entrypoint, e2e-tested not unit-tested)
into the Codecov frontend flag at 0%, lowering the reported number. Coverage is
informational/non-gating here, so it's cosmetic — but scoping the include to
`scripts/a11y/allowlist.mjs` (or excluding the runner) would keep the signal honest.

**S2 — `scripts/a11y/*.mjs` are entirely unlinted.** ESLint's flat config only
lints files matched by a `files` glob, so the `.mjs` tooling is skipped. This is
the documented `.mjs` trade-off (consistent with the repo's other `.js` config
files), and the logic-bearing module is fully unit-tested — noting it for
awareness, not asking for a change.

---

## Validation Results

| Check | Status | Details |
|-------|--------|---------|
| Frontend lint | PASS | `eslint . --max-warnings 0` clean |
| a11y unit tests | PASS | 14 passed |
| Full vitest suite | PASS | 269 passed / 32 files (incl. new `a11y` project) |
| Build | PASS | `tsc -b && vite build` clean; no `design-*` chunk in dist |
| Prettier / markdownlint / actionlint / yamllint | PASS | clean (verified pre-commit + this session) |
| **a11y gate (CI, x86)** | **FAIL (by design)** | agent-browser installed + drove browser; 3 nodes allowlisted; failed on badge `#e8dcc2`/`#8e6f38` 3.44:1 — see H1 |
| Frontend job (CI) | PASS | |

---

## Pattern Compliance

- [x] Follows existing CI job conventions (SHA-pinned actions, `persist-credentials:false`, frontend-conditional, ci-gate wiring)
- [x] Mirrors the `vite-plugins` node-env vitest project pattern
- [x] MADR ADR shape; index + status-flip-in-PR convention honoured
- [x] Tests added (TDD; the pure logic is the test target)
- [x] Docs updated (ADR, CONTRIBUTING, PR template)
- [x] No untrusted `${{ }}` in `run:` blocks (workflow-injection safe)

---

## What's Good

- **Failure-mode discipline**: role-not-bgColor discriminator (the badge and buttons share `#8e6f38`, so colour can't separate them) and the fail-closed `scanOk` liveness check (a crashed/blank scan can't masquerade as "0 violations"). Both are the difference between a real gate and a green-looking no-op.
- **CI-validated cross-arch**: the agent-browser-on-x86 unknown is resolved by the actual run.
- **Honest scope**: the badge bug is filed (UNK-345) and *deliberately not allowlisted*, so the gate proves it catches real misuse rather than masking it.
- **Clean removal** of the dead `@axe-core/cli`/`chromedriver` deps (49 packages).
- Thorough negative/edge tests (anti-bgColor regression, missing-html fail-closed, scan-sentinel).

---

## Recommendation

**REQUEST CHANGES** — code is correct and CI-validated; the single blocker is the
rollout decision in **H1** (the gate blocks its own PR and all frontend PRs until
UNK-345). Resolve H1 (recommend: advisory + documented flip-to-blocking, matching
the `impeccable detect` precedent), then this is ready to merge. S1/S2 optional.

---

*Reviewed by Claude*
*Report: `.claude/PRPs/reviews/pr-430-review.md`*
