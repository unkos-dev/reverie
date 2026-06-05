# Feature: UNK-268 — axe-core accessibility gate + a11y review ADR + PR-template checklist

## Summary

Wire an automated WCAG 2.2 Level AA accessibility gate into CI for the frontend,
driven by `agent-browser` (CDP) running `axe-core` against the dev server, with a
documented, unit-tested allowlist for the one accepted brand carve-out (Reverie
Gold on large CTAs). Add an MADR ADR codifying the manual audit cadence + the
automated/manual tooling boundary, and a PR template carrying an a11y checklist.
This makes the "WCAG 2.2 AA as a design invariant" promise in `frontend/PRODUCT.md`
load-bearing instead of aspirational.

## User Story

As a Reverie maintainer / contributor
I want CI to fail on any new WCAG 2.2 AA violation outside a documented allowlist
So that the accessibility invariant in PRODUCT.md is enforced, not just stated.

## Problem Statement

`frontend/PRODUCT.md` § Accessibility and `frontend/DESIGN.md` §2 declare WCAG 2.2
AA a design invariant and name specific rules (1.4.3, 1.4.11) plus an accepted
carve-out (gold on light surfaces restricted to focus rings / large CTAs /
recovery actions). Nothing enforces this. There is no axe run, no audit cadence,
and no PR-time checklist. The spec can drift from reality undetected.

## Solution Statement

A dedicated, path-conditional CI job (`a11y`) starts the Vite dev server (the
design-system showcase routes are dev-only), uses `agent-browser` over CDP to
inject `axe-core` and run it with the full WCAG 2.2 AA tag set, then pipes the
results through a pure, unit-tested allowlist/verdict module that drops the one
documented brand carve-out and exits non-zero on anything else. An ADR records
the surrounding human process; a PR template surfaces the checklist on UI PRs.

## Metadata

| Field            | Value                                                                         |
| ---------------- | ----------------------------------------------------------------------------- |
| Type             | NEW_CAPABILITY                                                                |
| Complexity       | MEDIUM                                                                        |
| Systems Affected | CI (`.github/workflows/ci.yml`), frontend tooling, ADRs, PR template, docs    |
| Dependencies     | `axe-core` ^4.11.4 (already present), `agent-browser` 0.27.0 (image-provided) |
| Estimated Tasks  | 12                                                                            |

---

## Locked Decisions (from this session — do NOT re-litigate)

1. **Driver = `agent-browser` over CDP.** Drives Brave locally on ARM64
   (`/usr/bin/brave-browser`, via `AGENT_BROWSER_EXECUTABLE`), Chromium in CI on
   x86. NO Playwright, NO selenium, NO chromedriver. Proven working this session:
   `agent-browser open <url>` then `agent-browser eval --stdin` with
   `node_modules/axe-core/axe.min.js` concatenated to an `axe.run(...)` call
   returns violations JSON.
   - **Why not `@axe-core/cli`:** its bundled `chromedriver` npm pkg AND
     `selenium-webdriver`'s `selenium-manager` are x64-only ELF binaries →
     `exec format error` on this aarch64 box. Chrome-for-Testing has no
     linux-arm64 build (Chrome ARM64 Linux GA only Q2 2026). Ubuntu 24.04 Noble
     has no clean apt chromium (snap-only). Dead end on ARM dev.
2. **Tag set = full WCAG 2.2 AA:** `["wcag2a","wcag2aa","wcag21a","wcag21aa","wcag22aa"]`.
   - **Why not `wcag22aa` alone (the ticket's literal text):** in axe that tag
     selects only rules _new_ in WCAG 2.2 (e.g. `target-size`). It returns ZERO
     `color-contrast` findings (`color-contrast` is tagged `wcag2aa`/`wcag143`).
     A `--tags wcag22aa` gate would pass trivially and enforce nothing. Verified
     empirically this session (0 vs 1 violation).
3. **Allowlist scope = the gold large-CTA carve-out only.** lg primary buttons +
   their loading state. The default Badge contrast is a **bug**, NOT allowlisted
   (see Real Axe Data). File a separate Linear issue; do not fix the badge here.
4. **Dedicated `a11y` CI job**, gated `if: needs.changes.outputs.frontend == 'true'`
   and added to the `ci-gate` aggregator's `needs:` (skipped == pass, so
   docs/backend-only PRs do not block).
5. **ADR status flips proposed→accepted in this same PR** (repo convention:
   adr/2026-05-08 ... ADR-status-flip-at-merge). `supersedes: []`.

---

## Real Axe Data (captured this session, /design/system, full-AA tags)

Exactly ONE violation: `color-contrast` (serious), 4 nodes, all cream foreground
on Reverie Gold background:

| #   | Node                                 | fg / bg           | ratio | size      | Permitted gold surface?        | Disposition              |
| --- | ------------------------------------ | ----------------- | ----- | --------- | ------------------------------ | ------------------------ |
| 1   | `bg-primary` button `data-size="lg"` | #e8dcc2 / #8e6f38 | 3.44  | 14px norm | YES — "large CTA"              | ALLOWLIST                |
| 2   | `bg-primary` button `data-size="lg"` | #e8dcc2 / #8e6f38 | 3.44  | 14px norm | YES — "large CTA"              | ALLOWLIST                |
| 3   | primary button loading state (pulse) | #e9ddc4 / #9c804e | 2.78  | 14px norm | YES — large CTA state          | ALLOWLIST                |
| 4   | default Badge `.group/badge`         | #e8dcc2 / #8e6f38 | 3.44  | 12px      | **NO** (not CTA/ring/recovery) | **BUG → separate issue** |

> **Discriminator note (load-bearing):** the buttons (ALLOW) and the badge
> (DENY) share the **identical** gold bg `#8e6f38`. Background colour therefore
> CANNOT separate allow from deny — keying the allowlist on `bgColor` would
> swallow the very bug it must catch. The only reliable signal is element ROLE
> from `node.html`: permitted large CTAs carry `data-slot="button"` **and**
> `data-size="lg"`; the badge carries `data-slot="badge"` and no `data-size`.
> All three allowlisted nodes (incl. the loading state, whose `target` is the
> `.animate-[loading-pulse…]` class with NO `data-size`) are `data-slot="button"`
> `data-size="lg"` in their html — so match on html attributes, never on `target`.

DESIGN.md §2 "Light-Gold Restriction Rule": _"axe-core contrast violations on
small-text gold are the right signal — the surface is misusing the accent."_ The
badge is exactly that signal.

---

## Mandatory Reading (implementation agent reads before starting)

| Priority | File                                       | Lines                                        | Why                                                   |
| -------- | ------------------------------------------ | -------------------------------------------- | ----------------------------------------------------- |
| P0       | `.github/workflows/ci.yml`                 | 588–643 (frontend job), 899–935 (ci-gate)    | Mirror frontend-job conventions; wire ci-gate `needs` |
| P0       | `.github/workflows/ci.yml`                 | 25–75 (changes job), 61–64 (frontend filter) | `if: needs.changes.outputs.frontend == 'true'` gating |
| P0       | `frontend/vite.config.ts`                  | 90–119 (test projects + coverage)            | Add 3rd vitest project; extend coverage include       |
| P0       | `frontend/DESIGN.md`                       | §2 Colors / Named Rules                      | Allowlist rationale source-of-truth                   |
| P1       | `frontend/PRODUCT.md`                      | § Accessibility & Inclusion                  | ADR + PR-template checklist source                    |
| P1       | `adr/TEMPLATE.md` + `adr/CLAUDE.md`        | all                                          | MADR shape; NO impl-plan sections                     |
| P1       | `adr/README.md`                            | 22–60 (index)                                | Add index entry in lockstep                           |
| P2       | `frontend/src/App.test.tsx`                | 1–30                                         | vitest import/style pattern                           |
| P2       | `frontend/vite-plugins/` (any `__tests__`) | —                                            | node-env project pattern to mirror for `a11y` project |
| P2       | `frontend/src/components/ui/badge.tsx`     | all                                          | Context for the deferred Badge issue (do NOT edit)    |
| P2       | `frontend/src/main.tsx`                    | 58–61                                        | Confirms design routes are dev-only                   |

**External docs:**
| Source | Why |
| --- | --- |
| axe-core API `axe.run` options + tags (`/dequelabs/axe-core` v4.11) | Confirm tag semantics: `wcag22aa` = 2.2-delta only |
| agent-browser core skill (`agent-browser skills get core --full`) | `open` / `eval --stdin` / `wait --load` / `close` usage |

---

## Patterns to Mirror

**Existing node-env vitest project (mirror for the `a11y` project) — `frontend/vite.config.ts:99-117`:**

```ts
projects: [
  {
    extends: true,
    test: { name: "vite-plugins", environment: "node",
      include: ["vite-plugins/**/__tests__/**/*.test.ts"] },
  },
  { /* frontend jsdom project */ },
],
```

**Existing CI frontend job conventions — `.github/workflows/ci.yml:588-612`:** SHA-pinned
actions, `defaults.run.working-directory: frontend`, `persist-credentials: false`,
`setup-node@…` with `node-version: 24.16.0`, `cache: npm`,
`cache-dependency-path: frontend/package-lock.json`, `npm ci`.

**Direct-curl pinned-binary install pattern (no `command -v` fallback, hard-rule-8) —
`.github/workflows/ci.yml:422-436` (actionlint).** Use the same fail-closed shape
for `npm i -g agent-browser@0.27.0`.

**ci-gate aggregator — `.github/workflows/ci.yml:909-934`:** add `a11y` to `needs`;
`if: always()`; failure/cancelled in `needs.*.result` fails the gate; `skipped` passes.

---

## Files to Change

| File                                                | Action                 | Justification                                                                                    |
| --------------------------------------------------- | ---------------------- | ------------------------------------------------------------------------------------------------ |
| `frontend/scripts/a11y/allowlist.mjs`               | CREATE                 | Pure allowlist+verdict logic (TDD target)                                                        |
| `frontend/scripts/a11y/__tests__/allowlist.test.ts` | CREATE                 | Vitest tests for allowlist (failing first)                                                       |
| `frontend/scripts/a11y/fixtures/violations.json`    | CREATE                 | Real captured axe output as test fixture                                                         |
| `frontend/scripts/a11y/axe-scan.mjs`                | CREATE                 | agent-browser driver + axe injection + liveness check + artifact                                 |
| `frontend/vite.config.ts`                           | UPDATE                 | 3rd vitest project (`a11y`, node) + coverage include `scripts/a11y/**`                           |
| `frontend/tsconfig.node.json`                       | UPDATE                 | Add `scripts/a11y/**/*.ts` to `include` so typed ESLint (`projectService`) accepts the test file |
| `frontend/package.json`                             | UPDATE                 | Add `"a11y"` script; remove dead `@axe-core/cli`, `chromedriver`, `puppeteer` config             |
| `frontend/package-lock.json`                        | UPDATE                 | Regenerated by `npm install` after dep removal                                                   |
| `.github/workflows/ci.yml`                          | UPDATE                 | New `a11y` job + add to `ci-gate` needs                                                          |
| `adr/2026-06-05-accessibility-review-process.md`    | CREATE                 | MADR ADR, status accepted                                                                        |
| `adr/README.md`                                     | UPDATE                 | Index entry for the new ADR                                                                      |
| `.github/pull_request_template.md`                  | CREATE                 | PR template with a11y checklist + `Closes` line                                                  |
| `frontend/README.md` (or `CONTRIBUTING`)            | UPDATE                 | Link to the new ADR                                                                              |
| `frontend/a11y-results.json`                        | (artifact, gitignored) | Add to `frontend/.gitignore`                                                                     |

---

## NOT Building (scope limits)

- **No Badge fix** — separate Linear issue (Task 1). Gate is expected to FAIL on
  the badge until that issue ships; the badge is intentionally NOT allowlisted.
- **No post-login view coverage** (Home/Library/Detail) — needs an authenticated
  session; ADR documents the cadence to add them later. Initial scope: `/design/system`.
- **No WCAG 2.2 AAA** — brand-incompatible (single-accent rule vs 4.5:1 body text).
- **No Storybook / visual-regression** — separate concern.
- **No `@axe-core/cli` / Playwright / chromedriver** — see Locked Decision 1.

---

## Step-by-Step Tasks (execute in order; TDD where logic exists)

### Task 1: File the deferred Badge Linear issue (do FIRST, capture ID)

- **ACTION**: Search Reverie project issues for an existing Badge-contrast issue
  (dedup rule). If none, create one: title ~ "fix(ui): default Badge variant
  fails WCAG 1.4.3 (cream-on-gold 3.44:1)". Body: cite DESIGN.md §2, the 12px
  cream `#e8dcc2` on gold `#8e6f38` finding, that gold is not a permitted badge
  surface, and that UNK-268's a11y gate will fail until fixed. Attach to v0.1.0
  milestone if it's release-gating; else leave milestone unset. Priority Medium.
- **GOTCHA**: cap save_issue bursts; pass UUID for `state` if setting one.
- **VALIDATE**: issue URL returned; record the `UNK-XXX` id for ADR/PR cross-ref.

### Task 2: Capture the real axe fixture

- **ACTION**: CREATE `frontend/scripts/a11y/fixtures/violations.json` from the
  real captured output: the full `color-contrast` violation with all 4 nodes
  (targets + `any[0].data` fg/bg/ratio). This is ground truth for tests.
- **VALIDATE**: valid JSON (`node -e "JSON.parse(require('fs').readFileSync(...))"`).

### Task 3 (TDD-RED): Write failing allowlist tests

- **ACTION**: CREATE `frontend/scripts/a11y/__tests__/allowlist.test.ts`.
- **IMPLEMENT** (import from `../allowlist.mjs`, which does not exist yet → RED):
  - the two lg buttons → allowlisted (filtered out) — match on `data-slot="button"`+`data-size="lg"` in html
  - the loading-state node → allowlisted (its `target` has no `data-size`; its **html** does — proves html-keying, not target-keying)
  - the default badge node → NOT allowlisted (remains) — `data-slot="badge"`, no `data-size`, despite SAME `#8e6f38` bg as the buttons
  - **anti-bgColor regression**: a synthetic non-button node with bg `#8e6f38` (badge-like) → remains (guards against a bgColor-based discriminator slipping in)
  - a synthetic NEW color-contrast node on a non-gold bg → remains
  - a synthetic non-contrast violation (e.g. `image-alt`) → remains
  - `verdict({ violations: [], scanOk: true })` → pass
  - `verdict({ violations: [badge], scanOk: true })` → fail (non-empty remaining)
  - **scan-failure sentinel**: `verdict({ violations: [], scanOk: false })` → **fail** (an empty result from a failed/blank scan must NOT pass — guards Finding S1)
  - edge: empty input, violation with no `nodes`, node with missing `any[].data`, node with missing/empty `html`
- **MIRROR**: `frontend/src/App.test.tsx:1-7` import style (`describe/test/expect` from vitest).
- **VALIDATE**: `npm test -- a11y` fails (module missing) — RED confirmed.

### Task 4: Wire the `a11y` vitest project + tsconfig + coverage

- **ACTION**:
  1. UPDATE `frontend/vite.config.ts`: add a 3rd project mirroring `vite-plugins`
     — `{ name: "a11y", environment: "node",
include: ["scripts/a11y/**/__tests__/**/*.test.ts"] }`; add `"scripts/a11y/**"`
     to `coverage.include`.
  2. UPDATE `frontend/tsconfig.node.json`: add `"scripts/a11y/**/*.ts"` to its
     `include` array (currently `["vite.config.ts","vite-plugins/**/*.ts"]`). REQUIRED
     — `eslint.config.js:50` matches `**/*.{ts,tsx}` with `projectService: true`
     (line 62), so any `.ts` not in a tsconfig project makes `eslint . --max-warnings 0`
     hard-fail ("not found in any of the provided project(s)"). The `vite-plugins`
     tests lint cleanly _only because_ they're in this include — mirror that.
  3. Confirm `.mjs` lint status: no `files` block in `eslint.config.js` targets
     `**/*.mjs` under `scripts/` (lines 50, 156 cover `.ts/.tsx` + `src`/`vite-plugins`
     `.js`). The `.mjs` runner files lint under base rules only — verify `npm run lint`
     is clean on them; if eslint flags them, add a `files: ["scripts/**/*.mjs"]` block
     with node globals + relaxed rules (NOT a blanket ignore).
- **GOTCHA**: keep `extends: true`; node env (no jsdom needed for pure logic).
- **VALIDATE**: `npm test` discovers the new project (test still RED); `npm run lint`
  does NOT error on the new `.ts`/`.mjs` files.

### Task 5 (TDD-GREEN): Implement the allowlist + verdict module

- **ACTION**: CREATE `frontend/scripts/a11y/allowlist.mjs` (plain ESM, node-runnable
  — NOT `.ts`, so `axe-scan.mjs` imports it without a transpile step).
- **IMPLEMENT**:
  - `ALLOWLIST`: array of entries, each `{ ruleId: "color-contrast",
htmlIncludesAll: ['data-slot="button"', 'data-size="lg"'],
rationale: "DESIGN.md §2 Light-Gold Restriction: gold permitted on large CTAs",
issue: null }`. Match a node iff `violation.id === entry.ruleId` AND
    **every** string in `htmlIncludesAll` is a substring of `node.html`.
    - **Discriminator = element ROLE from `node.html`, NOT bgColor, NOT target.**
      The badge shares the identical `#8e6f38` bg (see Real Axe Data note), so
      bgColor cannot separate allow from deny; and the loading node's `target`
      lacks `data-size` while its html has it. bgColor MAY be kept only as a
      defensive secondary assertion (warn/log if an allowlisted node's bg is not
      a known gold shade), never as the match key.
  - `filterAllowed(violations) -> remainingViolations` (drops allowlisted nodes;
    drops a violation entirely if all its nodes are allowlisted; a node with
    missing/empty `html` is NEVER allowlisted — fail closed).
  - `verdict({ violations, scanOk }) -> { pass: boolean, remaining }`:
    `pass === (scanOk === true && filterAllowed(violations).length === 0)`.
    **`scanOk: false` always fails** regardless of violations (Finding S1: an
    empty result from a crashed/blank scan must not pass).
  - Each allowlist entry carries an inline rationale comment → DESIGN.md §2.
  - **Deliberately do NOT match the badge** (`data-slot="badge"`, no `data-size`)
    → it stays a failure even though its bg equals the buttons'.
- **VALIDATE**: `npm test -- a11y` GREEN; coverage of `allowlist.mjs` ≥ 80%;
  the anti-bgColor + scan-failure-sentinel tests pass.

### Task 6: Implement the scan runner

- **ACTION**: CREATE `frontend/scripts/a11y/axe-scan.mjs`.
- **IMPLEMENT**:
  - Config: `BASE_URL` env (default `http://localhost:5173`); `TARGETS` array
    (initially `["/design/system"]`; comment noting `/design/hero/*` + post-login
    views are future per ADR).
  - For each target: shell out to `agent-browser open <url>`,
    `agent-browser wait --load networkidle`, then `agent-browser eval --stdin`
    feeding `node_modules/axe-core/axe.min.js` + an `axe.run(document,{runOnly:{
type:"tag",values:["wcag2a","wcag2aa","wcag21a","wcag21aa","wcag22aa"]}})`
    trailer. The trailer must return enough to (a) discriminate roles and (b)
    prove liveness — return `JSON.stringify({ url: result.url,
testEngine: result.testEngine, counts: { violations: result.violations.length,
passes: result.passes.length, inapplicable: result.inapplicable.length },
violations: result.violations })`. **Capture each node's full `html`
    untruncated** (the allowlist keys on `data-slot`/`data-size` in html — do NOT
    `.slice()` it like the manual probe did). Parse stdout JSON.
  - **Liveness assertion (Finding S1) — compute `scanOk` per target, fail loud:**
    `scanOk` is true only if the agent-browser commands all exited 0, stdout
    parsed as the expected object, `testEngine.name` is present, `result.url`
    matches the intended target, and `counts.passes + counts.inapplicable > 0`
    (a real axe run on the showcase always yields a non-trivial passes/inapplicable
    set; `0/0` means a blank/error page or a crashed browser). On any agent-browser
    non-zero exit, parse error, or failed assertion → set `scanOk=false` and record
    the reason; NEVER treat it as "0 violations".
  - `agent-browser close` in a `finally`.
  - Aggregate all targets' violations + an overall `scanOk` (AND across targets) →
    write `frontend/a11y-results.json` (machine artifact incl. `scanOk` + per-target
    meta). Call `verdict({ violations, scanOk })`. Print a human-readable summary
    (per-rule, per-node target + ratio, allowlisted vs remaining; and a loud line
    if `scanOk=false` with the failure reason). `process.exit(verdict.pass ? 0 : 1)`
    — so a failed scan exits non-zero (red), not green.
  - Header comment: the `wcag22aa`-alone pitfall; the role-not-bgColor discriminator
    (badge shares the buttons' gold bg); the silent-pass guard; the
    agent-browser/Brave-on-ARM rationale (link debt entry if one is filed).
- **GOTCHA**: `agent-browser eval` runs as a script (no top-level `return`) — the
  injected trailer must be a bare promise expression. Use `child_process` with the
  axe source piped via stdin (avoid heredocs). Respect `AGENT_BROWSER_EXECUTABLE`.
- **VALIDATE (local, end-to-end)**: with the supervised dev server up,
  `AGENT_BROWSER_EXECUTABLE=/usr/bin/brave-browser npm run a11y` → exits 1, summary
  shows 2 buttons + loading allowlisted, badge remaining; `a11y-results.json` written.

### Task 7: Wire `package.json`

- **ACTION**: UPDATE `frontend/package.json`: add `"a11y": "node scripts/a11y/axe-scan.mjs"`.
  Remove devDeps `@axe-core/cli` and `chromedriver`; remove top-level
  `"puppeteer": { "skipDownload": true }`. Keep `axe-core`.
- **VALIDATE**: `npm install` (regenerates lockfile, drops removed trees);
  `npm test` + `npm run lint` still green.

### Task 8: gitignore the artifact

- **ACTION**: UPDATE `frontend/.gitignore` (create if absent) to ignore
  `a11y-results.json`.
- **VALIDATE**: `git status` does not show the artifact after a run.

### Task 9: Add the CI `a11y` job

- **ACTION**: UPDATE `.github/workflows/ci.yml`. New job `a11y`:
  - `needs: changes`; `if: needs.changes.outputs.frontend == 'true'`;
    `defaults.run.working-directory: frontend`; `timeout-minutes: 20`.
  - Steps: checkout (`persist-credentials: false`); setup-node 24.16.0 + npm cache
    (`cache-dependency-path: frontend/package-lock.json`); `npm ci`; install
    `agent-browser@0.27.0` pinned + `agent-browser install --with-deps` (fail-closed,
    no `command -v`); start `npm run dev` in background + poll `:5173` until ready
    (bounded loop); `npm run a11y`; upload `frontend/a11y-results.json` artifact
    (`if: always()`).
  - Add `a11y` to the `ci-gate` job `needs:` list.
- **GOTCHA**: dev server is backgrounded — capture PID / use `&` + a readiness
  curl loop with timeout; the design routes only exist under `npm run dev` (NOT
  `vite preview`). Ensure `AGENT_BROWSER_EXECUTABLE`/browser path is correct for the
  CI runner (use the agent-browser-installed Chromium or the runner's Chrome).
- **VALIDATE**: `actionlint` clean; `zizmor` clean; `yamllint` clean. (Real CI run
  validates on PR.)

### Task 10: Write the ADR

- **ACTION**: CREATE `adr/2026-06-05-accessibility-review-process.md` from
  `adr/TEMPLATE.md`. `status: accepted`, `date: 2026-06-05`, `supersedes: []`.
  Sections: Context (PRODUCT.md invariant, no enforcement); Decision Drivers;
  Considered Options (agent-browser+axe vs @axe-core/cli vs Playwright — record why
  ARM killed the CLI path, why no Playwright); Decision Outcome (the gate +
  cadence + ownership); Consequences; Confirmation (the load-bearing invariant:
  "CI `a11y` job fails on any WCAG 2.2 AA violation outside the documented
  allowlist in `scripts/a11y/allowlist.mjs`"). Cover: manual audit cadence (every
  release tag + before any net-new view ships), reviewer ownership (any team
  member; designated a11y reviewer optional), automated-vs-manual boundary
  (axe catches contrast/names/roles; manual only: keyboard nav order, SR
  semantics, dialog focus management, `prefers-reduced-motion` budget), the
  allowlist/mitigation-log convention, and the `wcag22aa`-tag pitfall. NO
  implementation-plan/checklist sections.
- **VALIDATE**: markdownlint clean; matches TEMPLATE section order.

### Task 11: Update ADR index + docs link

- **ACTION**: UPDATE `adr/README.md` — add index line:
  `- [Accessibility review process ...](2026-06-05-accessibility-review-process.md) (accepted, 2026-06-05)`.
  UPDATE `frontend/README.md` (or root `CONTRIBUTING` if it's the contributor
  surface) with a short "Accessibility" pointer to the ADR.
- **VALIDATE**: markdownlint clean; lychee link-check passes (relative paths resolve).

### Task 12: Create the PR template

- **ACTION**: CREATE `.github/pull_request_template.md`. Include a brief summary
  scaffold, a `Closes UNK-XXX` line (hard rule 9), and the a11y checklist (fires
  on UI PRs): keyboard nav reaches every interactive element; focus visible (gold
  3px ring); 1.4.11 (3:1) on non-text controls; 1.4.3 (4.5:1) body text except the
  documented gold carve-out; `prefers-reduced-motion` respected; new colour is a
  token (no arbitrary hex); alarm only in its two carve-out contexts. Note the
  checklist is N/A for non-UI PRs.
- **VALIDATE**: markdownlint clean; renders as the default PR body on `gh pr create`.

---

## Testing Strategy

### Unit tests (the only logic-bearing surface)

| Test File                                  | Cases                                                                                                                                                                                                                                                                             | Validates                   |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- |
| `scripts/a11y/__tests__/allowlist.test.ts` | buttons allowlisted (html role); loading allowlisted (html, not target); badge remains (same bg as buttons); anti-bgColor regression; new non-gold contrast remains; non-contrast rule remains; verdict pass/fail; **scanOk:false ⇒ fail**; empty/missing-data/missing-html edges | `filterAllowed` + `verdict` |

### Edge Cases Checklist

- [ ] `verdict({violations:[], scanOk:true})` → pass
- [ ] `verdict({violations:[], scanOk:false})` → **fail** (silent-scan guard, S1)
- [ ] `verdict({violations:[badge], scanOk:true})` → fail
- [ ] Violation with `nodes: []`
- [ ] Node missing `any[].data`
- [ ] Node with missing/empty `html` → never allowlisted (fail closed)
- [ ] Anti-bgColor: a non-button node on `#8e6f38` → remains (badge has the buttons' bg)
- [ ] New contrast violation on a non-gold surface → not allowlisted
- [ ] Allowlist must NOT swallow the badge (regression guard for scope)

### End-to-end (manual, local, before "done")

- [ ] `AGENT_BROWSER_EXECUTABLE=/usr/bin/brave-browser npm run a11y` exits 1,
      badge remaining, buttons allowlisted, `a11y-results.json` written.

---

## Validation Commands

### Level 1 — static analysis (frontend)

```bash
cd frontend && npm run lint && npx stylelint 'src/**/*.css' --max-warnings 0
```

EXPECT: exit 0.

### Level 2 — unit tests + coverage

```bash
cd frontend && npm run test:coverage
```

EXPECT: all projects pass incl. `a11y`; `allowlist.mjs` coverage ≥ 80%.

### Level 3 — a11y gate end-to-end (local)

```bash
cd frontend && AGENT_BROWSER_EXECUTABLE=/usr/bin/brave-browser npm run a11y; echo "exit=$?"
```

EXPECT: exit 1 (badge), summary shows 3 allowlisted nodes.

### Level 4 — repo-wide gates (touched files)

```bash
cd /home/coder/reverie
npx --no-install prettier --check .
git ls-files -z '*.md' ':!:.claude/**' | xargs -0 -r npx --no-install markdownlint-cli2
actionlint -color
git ls-files -z '*.yml' '*.yaml' | xargs -0 -r yamllint
```

EXPECT: exit 0. (zizmor runs in CI on the workflow change.)

NOTE: lint-staged `prettier --write`/`--check` forces full-file normalization on
any touched non-conformant file — reformatting it in the same commit is required
to land, not scope creep.

---

## Pre-commit hygiene (C2)

Before committing, run `prettier --write` on every created/modified file
(`.mjs`, `.ts`, `fixtures/*.json`, the ADR, the PR template, README). `.prettierignore`
does NOT exclude `scripts/`, so all new files are in scope of the `prettier --check .`
CI gate; lint-staged also forces full-file normalization on any touched file.

## Acceptance Criteria (ticket)

- [ ] CI `a11y` job fails on any new WCAG 2.2 AA violation outside the allowlist
- [ ] ADR merged with manual audit cadence + reviewer ownership
- [ ] PR template lists the a11y checklist
- [ ] README/contributor docs link the ADR
- [ ] Separate Linear issue filed for the Badge contrast bug

---

## Risks and Mitigations

| Risk                                                        | Likelihood | Impact | Mitigation                                                                                                                                        |
| ----------------------------------------------------------- | ---------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent-browser` browser resolution differs CI vs local      | MED        | MED    | Pin 0.27.0; `agent-browser install --with-deps` in CI; explicit `AGENT_BROWSER_EXECUTABLE`; fail-closed step                                      |
| Dev-server readiness race in CI                             | MED        | MED    | Bounded curl poll on `:5173` before scan; `timeout-minutes`                                                                                       |
| Allowlist swallows the badge (shares buttons' `#8e6f38` bg) | MED→LOW    | HIGH   | **Key on element ROLE (`data-slot="button"`+`data-size="lg"` in html), never bgColor**; anti-bgColor regression test; badge-remains test          |
| Silent pass on scan/page failure (empty == 0 violations)    | MED→LOW    | HIGH   | Liveness assertion (`scanOk`): require testEngine + url match + non-trivial passes/inapplicable; `scanOk:false` ⇒ verdict fail; loud summary line |
| axe-core flakiness on lazy-loaded design chunk              | LOW        | MED    | `wait --load networkidle` before `axe.run`; design route confirmed rendering this session                                                         |
| `wcag22aa`-only misconfig silently enforces nothing         | LOW        | HIGH   | Full-AA tag set hard-coded; documented in runner + ADR; fixture test covers presence of color-contrast                                            |
| New `.ts` test outside any tsconfig breaks typed ESLint     | MED→LOW    | MED    | Task 4 adds `scripts/a11y/**/*.ts` to `tsconfig.node.json` include (mirrors vite-plugins)                                                         |
| Dead-dep removal breaks lockfile/install                    | LOW        | MED    | `npm install` regenerates; Level 1/2 re-run after                                                                                                 |

---

## Security Review (hard rule 6)

Touches CI config + dev-only tooling; no user input, auth, secrets, file I/O of
untrusted data, or response headers. The CI job runs read-only against a local
dev server with `persist-credentials: false`, no new secrets, fail-closed tool
install (hard rule 8). Artifact is non-sensitive axe JSON. **Stands up to review.**

## Notes

- agent-browser+Brave-on-ARM is a workspace-image capability, not a reverie
  workaround — but if the CLI-on-ARM gap is worth tracking, consider a `debt/`
  entry referencing the Q2-2026 Chrome-ARM64-Linux GA as the lift condition.
- `frontend/scripts/` is new; tooling lives there (not `src/`, which is app code).
- Confidence: 8/10 for one-pass — the only runtime unknown is agent-browser's CI
  browser story, validated on the first PR CI run.
