---
status: accepted
date: 2026-05-04
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
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
