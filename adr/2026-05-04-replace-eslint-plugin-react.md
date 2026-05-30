---
status: accepted
date: 2026-05-04
decision-makers: john
---

# Replace `eslint-plugin-react` with `@eslint-react/eslint-plugin`

## Context and Problem Statement

The frontend lint stack ratified in
[`adr/2026-05-03-strict-lint-policy.md`](2026-05-03-strict-lint-policy.md)
includes `eslint-plugin-react` as one of the three React-aware
plugins layered on top of `typescript-eslint`. The stack works on
`eslint@9.x` today, but it has hit a hard ceiling against
`eslint@10.x`:

- `eslint-plugin-react@7.37.5` (the current pinned version, last
  released 2025-04 — 13+ months ago at the time of this decision)
  declares its peer dependency range as
  `eslint@"^3 || ^4 || ^5 || ^6 || ^7 || ^8 || ^9.7"`. eslint v10
  sits outside the range
- Renovate raised PRs #135 (`@eslint/js` v10) and #136 (`eslint`
  v10); both failed `npm install` with `ERESOLVE` and were closed
- The upstream v10-compat tracker
  ([jsx-eslint/eslint-plugin-react#3977](https://github.com/jsx-eslint/eslint-plugin-react/issues/3977))
  shows no recent activity. No projected timeline
- Renovate force-pushed PR #136 12+ times retrying the broken
  install before being added to the Greptile bot exclude list,
  burning a slice of the trial review-credit budget along the way

Holding the line is a real cost: PR #147 added a Renovate
`packageRule` pinning `eslint` and `@eslint/js` at `<10`, which
freezes the project's eslint version and blocks future security
patches and feature work in the eslint v10 line. The pin is
documented as temporary and is meant to be removed once the
underlying blocker is resolved.

What `eslint-plugin-react` is currently doing for the project (per
`frontend/eslint.config.js`):

1. `react.configs.flat.recommended` — base ruleset, mostly
   historical rules that target React 16-era patterns
   (`jsx-uses-react`, `no-deprecated`, etc.)
2. `react.configs.flat['jsx-runtime']` — disables
   `react-in-jsx-scope` and `jsx-uses-react`. Redundant for projects
   on the new JSX transform (Reverie has been on it since project
   bootstrap)
3. `'react/jsx-key': 'error'` — explicit, load-bearing. Catches
   missing `key` prop on iterated JSX
4. `'react/no-array-index-key': 'error'` — explicit, load-bearing.
   Catches the `<List>` anti-pattern of using array indices as keys

Two of the four entries are load-bearing; the other two are dead
weight on a modern React/TS stack.

## Decision

Replace `eslint-plugin-react` with `@eslint-react/eslint-plugin`
(formerly published as `eslint-plugin-react-x`).

`@eslint-react/eslint-plugin` is a TypeScript-first reimplementation
of the React eslint rules, actively maintained on a weekly release
cadence, and explicitly supports `eslint@9` and `eslint@10` plus
flat config natively. Used in production by Vercel, Astro, and
TanStack ecosystems.

The two load-bearing rules have direct equivalents:

| `eslint-plugin-react`      | `@eslint-react/eslint-plugin`      |
| -------------------------- | ---------------------------------- |
| `react/jsx-key`            | `@eslint-react/no-missing-key`     |
| `react/no-array-index-key` | `@eslint-react/no-array-index-key` |

The historical/redundant entries (`flat.recommended`,
`flat['jsx-runtime']`) are dropped — `@eslint-react`'s
`recommended-typescript` preset replaces them with rules that
target current React patterns and integrate cleanly with the
existing `tseslint.configs.strictTypeChecked` extends.

`eslint-plugin-react-hooks` and `eslint-plugin-react-refresh` stay
as-is. Both are separately maintained, support the eslint v9 + v10
range, and are not part of this decision's scope.

## Consequences

- Good — unblocks `eslint` and `@eslint/js` v10 bumps. The Renovate
  pin from PR #147 is removed in the migration PR; future eslint
  majors flow through Renovate normally
- Good — replaces a 13-month-stale plugin with one that has shipped
  releases as recently as last week. Reduces supply-chain risk
- Good — `@eslint-react/eslint-plugin` is TypeScript-first; rule
  authors reach into TS type information, catching bugs that
  `eslint-plugin-react`'s untyped AST analysis misses
  (e.g. missing `key` on a typed `Array<T>` returned from a hook
  that needs renderable props)
- Good — fewer rules in the `recommended-typescript` preset are
  React-16-era historical, so post-migration the lint output is
  more relevant to the actual code under review
- Bad — the `recommended-typescript` preset turns on rules that
  `eslint-plugin-react`'s `recommended` did not. Migration PR will
  surface a one-time wave of new lint errors that need
  triage: address, suppress with documented reason, or override
  in the config. Expected scope: under 50 sites across
  `frontend/src/**`, given the strict-lint policy already enforces
  most modern React idioms
- Bad — third-party dependency swap. If
  `@eslint-react/eslint-plugin` itself goes stale in a future
  eslint major, the project hits the same blocker again. Mitigation:
  the `eslint-plugin-react-hooks` and
  `eslint-plugin-react-refresh` deps are independently maintained,
  so the React-specific lint surface is sharded across three
  upstream maintainers — failure of any one is contained
- Bad — rule names change. Any existing
  `// eslint-disable-next-line react/jsx-key` comments need
  rewriting to `@eslint-react/no-missing-key`. Likely zero or
  near-zero in current codebase but worth grepping in the
  migration PR
- Neutral — bundle size impact zero (lint runs in CI + dev only,
  not in production)
- Neutral — `frontend/CLAUDE.md` rules around `as` casts, TS
  `enum`, and raw hex are enforced via `no-restricted-syntax` and
  `@typescript-eslint/consistent-type-assertions` — neither plugin
  involved. No change

## Alternatives Considered

- **Drop `eslint-plugin-react` entirely; rely on `typescript-eslint` and `eslint-plugin-react-hooks` only.** Rejected — loses both
  load-bearing rules. `typescript-eslint` does not catch missing
  `key` props or array-index-as-key; `eslint-plugin-react-hooks`
  scope is limited to hook-rule violations
- **Fork `eslint-plugin-react`.** Rejected — indefinite maintenance
  burden on a single-maintainer project. The whole point of
  external lint plugins is offloading rule-authoring to the
  ecosystem
- **Wait for upstream to ship eslint v10 compat in
  `eslint-plugin-react`.** Rejected — issue
  [#3977](https://github.com/jsx-eslint/eslint-plugin-react/issues/3977)
  has been open without progress and the project has shown no
  release activity for over a year. Waiting indefinitely for a
  fix that may never come keeps the eslint pin in place
  indefinitely
- **Stay on eslint v9 indefinitely.** Rejected — same as the
  "wait for upstream" option in effect, with the additional cost
  of missing eslint v10 features and security fixes. The
  Renovate pin is documented as temporary precisely so this
  doesn't become the default
- **Switch to a different React lint plugin family entirely (e.g.
  Biome, deno-lint).** Rejected for this trial-scoped decision —
  swapping the linter is a different and larger architectural
  change with its own ADR-level scope. `@eslint-react` is the
  smallest swap that resolves the immediate blocker

## Implementation Plan

Single PR scope. All work in `frontend/`:

1. **Dependency swap.** In `frontend/package.json`:
   - Remove `eslint-plugin-react` from `devDependencies`
   - Add `@eslint-react/eslint-plugin` to `devDependencies`
   - Add `eslint@^10` and `@eslint/js@^10` (the bumps held back by
     the pin)
   - Run `npm install` — confirm `package-lock.json` regenerates
     without `ERESOLVE`

2. **Config swap.** In `frontend/eslint.config.js`:
   - Replace `import react from 'eslint-plugin-react'` with
     `import reactX from '@eslint-react/eslint-plugin'`
   - In the main `extends` array, replace
     `react.configs.flat.recommended` and
     `react.configs.flat['jsx-runtime']` with
     `reactX.configs['recommended-typescript']` (one entry, not
     two)
   - Drop the `settings.react` block (no longer required by
     `@eslint-react`'s plugin discovery)
   - Update the explicit rule names:
     `'react/jsx-key': 'error'` → `'@eslint-react/no-missing-key': 'error'`,
     `'react/no-array-index-key': 'error'` → `'@eslint-react/no-array-index-key': 'error'`
   - Re-verify the `Lockup.tsx` and `src/components/ui/**` carve-out
     blocks still target the right rules — both currently disable
     project-local rules (`no-restricted-syntax`,
     `@typescript-eslint/consistent-type-assertions`), neither of
     which is touched by this swap

3. **Migrate any `eslint-disable` comments referencing the old
   rule names** (`react/jsx-key`, `react/no-array-index-key`,
   anything else in the `react/*` namespace). Grep with
   `rg "react/" frontend/` (scope covers `frontend/src/**` AND
   `frontend/eslint.config.js` itself — the config file is the most
   likely place stale rule names hide after Step 2's swap). Likely
   zero matches outside the config given the strict-lint policy

4. **Run `npm run lint` and triage the wave of new findings.** The
   `recommended-typescript` preset is stricter than
   `eslint-plugin-react`'s `recommended`. For each new finding:
   either fix the code, suppress with an inline disable + comment
   reason, or add a `rules:` override in `eslint.config.js` with
   rationale (project-wide overrides should be rare and justified)

5. **Remove the Renovate eslint pin in the same PR.** In
   `.github/renovate.json`, delete the `packageRule` whose
   `description` starts with "Hold eslint and @eslint/js at v9".
   Renovate will subsequently raise eslint v10 + @eslint/js v10
   PRs naturally on the next poll

6. **Verify CI.** Frontend job runs `npm ci`, `npm run lint`,
   `npm test`, `npx stylelint`, and `npm run build`. All five must
   pass. The lint step is the one that surfaces any preset-driven
   regressions

**Affected paths (exhaustive):**

- `frontend/package.json`
- `frontend/package-lock.json`
- `frontend/eslint.config.js`
- `frontend/src/**/*.{ts,tsx}` — only the files where new lint
  errors surface; expected to be a small subset
- `.github/renovate.json` — remove the eslint pin
- `frontend/CLAUDE.md` — no expected change; reverify the React
  conventions section does not name `eslint-plugin-react`

**Not in scope (do not touch):**

- `eslint-plugin-react-hooks` — independently maintained
- `eslint-plugin-react-refresh` — independently maintained
- `typescript-eslint` configuration — separate concern
- `frontend/vite-plugins/csp-hash.ts` — separate concern
- `backend/` — Rust, unrelated
- `docs/` — Astro/Starlight, separate eslint config (if any)

## Verification

Walk through these after the migration PR lands:

- [x] `npm run lint` exits 0 against the migrated config
- [x] CI Frontend job passes end-to-end (lint, test, stylelint,
      font integrity, build)
- [ ] Renovate raises eslint v10 + @eslint/js v10 PRs on next
      poll (proves the pin removal is honoured by Renovate) —
      pending next Renovate poll cycle post-merge
- [x] Manual sanity test: introduce a deliberate `<ul>{items.map(i => <li>{i}</li>)}</ul>` (no `key`), confirm
      `@eslint-react/no-missing-key` flags it, revert
- [x] Manual sanity test: introduce
      `items.map((i, idx) => <li key={idx}>{i}</li>)`, confirm
      `@eslint-react/no-array-index-key` flags it, revert
- [x] `Lockup.tsx` and `Lockup.test.tsx` still lint-clean (carve-out
      block still works)
- [x] `src/components/ui/**` files still lint-clean (shadcn carve-out
      still works)
- [x] `rg "react/jsx-key|react/no-array-index-key|eslint-plugin-react" frontend/` returns zero matches outside this ADR (verified with
      word-boundary regex `rg -P "(?<!-)react/(jsx-key|no-array-index-key)|eslint-plugin-react(?!-)"`
      to exclude substring false positives from the new
      `@eslint-react/...` rule names and the unrelated
      `eslint-plugin-react-{hooks,refresh,dom}` packages)

## Revisit Conditions

Open a superseding ADR if any of the following happen:

- `@eslint-react/eslint-plugin` itself goes stale (no releases for
  6+ months while eslint major versions advance)
- The project decides to swap eslint for a different linter (Biome,
  deno-lint, oxlint), which would moot the React-plugin question
  entirely
- `eslint-plugin-react` upstream ships an eslint v10-compatible
  release AND there is concrete reason to migrate back (unlikely;
  noted for completeness)

## More Information

- MADR 4.0: <https://adr.github.io/madr/>
- `@eslint-react/eslint-plugin` docs: <https://eslint-react.xyz>
- eslint flat config migration:
  <https://eslint.org/docs/latest/use/configure/migration-guide>
- Related: [`adr/2026-05-03-strict-lint-policy.md`](2026-05-03-strict-lint-policy.md)
  — frontend lint stack baseline this ADR amends
- Related: [`adr/2026-05-04-greptile-trial.md`](2026-05-04-greptile-trial.md)
  — trial review tally records the eslint v10 PRs that
  triggered this decision
- Related PRs:
  - #135 (closed) — `@eslint/js` v10 bump, ERESOLVE
  - #136 (closed) — `eslint` v10 bump, ERESOLVE, force-pushed 12×
  - #147 (merged) — Renovate pin holding eslint at `<10`
- Tracker: UNK-155 trial tally has the false-positive credit-cap
  and hallucination context that surrounded the eslint v10 saga
