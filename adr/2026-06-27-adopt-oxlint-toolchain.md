---
status: accepted
date: 2026-06-27
supersedes:
  [
    "superseded/2026-05-04-replace-eslint-plugin-react.md",
    "superseded/2026-05-22-frontend-docstring-tooling.md",
  ]
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Adopt oxlint, replacing the ESLint toolchain

## Context and Problem Statement

The frontend lint stack ratified in
[strict lint policy](2026-05-03-strict-lint-policy.md) ran on ESLint plus
typescript-eslint (`strictTypeChecked`), `eslint-plugin-react-hooks`,
`eslint-plugin-react-refresh`, `@eslint-react`, and `eslint-plugin-jsdoc`. This
record moves the JS/TS lint engine to oxlint, the Rust linter from the VoidZero
oxc project, as the first step toward a unified oxc toolchain (oxlint for
linting, oxfmt for formatting, a shared runner later).

The strategic force: the oxc family is an order of magnitude faster (native
Rust), converges lint and format on one engine, and is forward-aligned with
TypeScript 7. Its type-aware linter, `oxlint-tsgolint`, is built on
`typescript-go`, the same native engine as the TypeScript 7 compiler.

The constraint: the swap must preserve every enforcement that is a genuine
industry standard while shedding rules that were unexamined house-style. Three
frontend enforcements had no mechanical oxlint port and forced a real decision:
the object-literal `as`-cast ban, docstring-presence linting, and the React
rule layer.

## Decision Drivers

- One oxc toolchain, and lint speed.
- Full ESLint removal. No `eslint-plugin-*` left installed: the JS-plugin bridge
  keeps the ESLint ecosystem alive and is rejected.
- Preserve standards, shed house-style. A rule survives only if it is current
  industry practice or covered by a stronger control, not because it was
  previously enabled.
- Forward-align the type-aware path with `typescript-go` now.
- No silent enforcement loss: each surviving surface is proven to fire on a
  deliberate violation.

## Considered Options

- **oxlint, native rules only.**
- **oxlint with `@eslint-react` and `eslint-plugin-jsdoc` loaded through the
  JS-plugin bridge.** Preserves every prior rule, but the ESLint-ecosystem
  plugins stay installed, so the engine is never fully removed.
- **Stay on ESLint.** Slower, and off the unified-toolchain path.

## Decision Outcome

Chosen option: **oxlint as the sole JS/TS linter, native rules only, with
config-driven type-aware via `oxlint-tsgolint`.** The bridge option is rejected
because it fails the full-removal driver; staying on ESLint is rejected by the
toolchain direction.

The three forcing decisions resolved as follows.

- **Cardinal `as`-cast ban** (`typescript/consistent-type-assertions` with
  `objectLiteralTypeAssertions: never`) is a native oxlint rule and fires
  standalone, without type information. It needs no type-aware support, so the
  migration's main risk did not materialize.
- **Fetch centralisation** (`no-restricted-globals` and
  `no-restricted-properties`) is native.
- **The `strictTypeChecked` class** (`no-floating-promises`, `no-unsafe-*`) is
  retained through type-aware (`oxlint-tsgolint`), enabled now rather than
  deferred, because tsgolint is the `typescript-go` engine and adopting it early
  reduces the later compiler-migration delta.
- **The React baseline** is the native react plugin (`rules-of-hooks`,
  `exhaustive-deps`, `only-export-components`) plus the security and correctness
  rules that have native equivalents (`no-danger`, `jsx-no-script-url`,
  `no-find-dom-node`, `jsx-key`, and the rest of that set). The official
  `react/react-compiler` diagnostic is enabled so new code is written
  compiler-safe ahead of turning on the compiler transform.
- **Docstring-presence enforcement is dropped.** `require-jsdoc` was deprecated
  out of ESLint core in 2018 and appears in no typescript-eslint preset;
  machine-requiring a docblock on every export pressures authors toward
  boilerplate that restates the type signature. Existing docstrings stay as a
  reviewed convention. `eslint-plugin-jsdoc` is removed.
- **The `@eslint-react` opinionated layer is dropped.** Its rules are either
  dead by construction (the codebase is function-component only, so the
  class-component rules cannot fire) or backstopped (the `AbortController`
  cleanup convention, the React runtime, and native `exhaustive-deps`). The
  official React-lint baseline, `rules-of-hooks` and `exhaustive-deps`, is
  preserved natively.
- **The enum ban** moves off the linter to the type checker
  (`erasableSyntaxOnly`, already set). CSS hex stays on stylelint.

### Consequences

- Good, because the linter is faster, on the unified oxc path, and aligned with
  the native TypeScript compiler.
- Good, because ESLint is fully removed: no eslint packages and no bridges
  remain.
- Good, because React Compiler safety is enforced now, so code is compiler-safe
  before the transform is enabled.
- Neutral, because the `strictTypeChecked` class runs through `oxlint-tsgolint`
  and must be version-paired with TypeScript when the native compiler lands.
- Bad, because docstring presence and the inline-style ban become review-level,
  not machine-gated. The inline-style ban also has no Content-Security-Policy
  backstop, since `style-src` permits `unsafe-inline` for Tailwind. A separate
  frontend-standards review revisits both.

### Confirmation

`just js::oxlint` runs `oxlint` and gates the CI `frontend` job; a search for
`eslint` under `frontend/` returns nothing. Each surviving enforcement (cardinal
`as`, fetch, the type-aware class, and `react-compiler`) fires on a deliberate
violation, and docstring-presence does not.

## More Information

Supersedes
[Replace eslint-plugin-react with @eslint-react](superseded/2026-05-04-replace-eslint-plugin-react.md)
and
[Frontend docstring linting via eslint-plugin-jsdoc](superseded/2026-05-22-frontend-docstring-tooling.md).
Amends [strict lint policy](2026-05-03-strict-lint-policy.md), whose frontend
engine is now oxlint, and [tiered comment policy](2026-05-08-tiered-comment-policy.md),
whose frontend docstring floor is now review-level rather than lint-enforced.

Formatting moves to oxfmt in a paired record,
[adopt oxfmt](2026-06-28-adopt-oxfmt-formatter.md). Deferred follow-ups, tracked
outside this record: enabling the React Compiler transform, the migration to the
native TypeScript compiler with `oxlint-tsgolint` version-pairing, and a
rule-by-rule review of the frontend authoring standards against current practice.
