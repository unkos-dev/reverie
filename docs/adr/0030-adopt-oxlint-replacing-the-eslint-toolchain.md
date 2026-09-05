---
type: ADR
profile-version: 1
id: "REV-ADR-0030"
title: "Adopt oxlint, replacing the ESLint toolchain"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-27"
decision-makers:
  - "John Unkovich"
---

# Adopt oxlint, replacing the ESLint toolchain

## Context and problem statement

The frontend lint stack ratified in [strict lint policy](./0002-strict-lint-policy-pedantic-clippy-and-strict-frontend-lint.md)
ran on ESLint plus typescript-eslint (`strictTypeChecked`), `eslint-plugin-react-hooks`, `eslint-plugin-react-refresh`,
`@eslint-react`, and `eslint-plugin-jsdoc`. This decision moves the JS/TS lint engine to oxlint, the Rust linter from
the VoidZero oxc project, as the first step toward a unified oxc toolchain (oxlint for linting, oxfmt for formatting,
a shared runner later).

The strategic force: the oxc family is an order of magnitude faster (native Rust), converges lint and format on one
engine, and is forward-aligned with TypeScript 7. Its type-aware linter, `oxlint-tsgolint`, is built on
`typescript-go`, the same native engine as the TypeScript 7 compiler.

The constraint: the swap must preserve every enforcement that is a genuine industry standard while shedding rules
that were unexamined house-style. Three frontend enforcements had no mechanical oxlint port and forced a real
decision: the object-literal `as`-cast ban, docstring-presence linting, and the React rule layer.

## Decision drivers

- One oxc toolchain, and lint speed.
- Full ESLint removal. No `eslint-plugin-*` left installed: the JS-plugin bridge keeps the ESLint ecosystem alive and
  is rejected.
- Preserve standards, shed house-style. A rule survives only if it is current industry practice or covered by a
  stronger control, not because it was previously enabled.
- Forward-align the type-aware path with `typescript-go` now.
- No silent enforcement loss: each surviving surface is proven to fire on a deliberate violation.

## Considered options

- oxlint, native rules only, with config-driven type-aware via `oxlint-tsgolint`
- oxlint with `@eslint-react` and `eslint-plugin-jsdoc` loaded through the JS-plugin bridge
- Stay on ESLint

## Decision outcome

Chosen option: **oxlint, native rules only, with config-driven type-aware via `oxlint-tsgolint`**, because it
completes the full removal of ESLint while preserving every enforcement that is a genuine industry standard. The
JS-plugin bridge option is rejected because it fails the full-removal driver; staying on ESLint is rejected by the
toolchain direction.

The three forcing decisions resolved as follows.

- **Cardinal `as`-cast ban** (`typescript/consistent-type-assertions` with `objectLiteralTypeAssertions: never`) is a
  native oxlint rule and fires standalone, without type information. It needs no type-aware support, so the
  migration's main risk did not materialise.
- **Fetch centralisation** (`no-restricted-globals` and `no-restricted-properties`) is native.
- **The `strictTypeChecked` class** (`no-floating-promises`, `no-unsafe-*`) is retained through type-aware
  (`oxlint-tsgolint`), enabled now rather than deferred, because tsgolint is the `typescript-go` engine and adopting
  it early reduces the later compiler-migration delta.
- **The React baseline** is the native react plugin (`rules-of-hooks`, `exhaustive-deps`, `only-export-components`)
  plus the security and correctness rules that have native equivalents (`no-danger`, `jsx-no-script-url`,
  `no-find-dom-node`, `jsx-key`, and the rest of that set). The official `react/react-compiler` diagnostic is enabled
  so new code is written compiler-safe ahead of turning on the compiler transform.
- **Docstring-presence enforcement is dropped.** `require-jsdoc` was deprecated out of ESLint core in 2018 and
  appears in no typescript-eslint preset; machine-requiring a docblock on every export pressures authors toward
  boilerplate that restates the type signature. Existing docstrings stay as a reviewed convention, and
  `eslint-plugin-jsdoc` is removed. An earlier, now-retired decision had enforced docstring presence on frontend
  exports through `eslint-plugin-jsdoc`; that enforcement is what this decision drops.
- **The `@eslint-react` opinionated layer is dropped.** Its rules are either dead by construction (the codebase is
  function-component only, so the class-component rules cannot fire) or backstopped (the `AbortController` cleanup
  convention, the React runtime, and native `exhaustive-deps`). The official React-lint baseline, `rules-of-hooks` and
  `exhaustive-deps`, is preserved natively.
- **The enum ban** moves off the linter to the type checker (`erasableSyntaxOnly`, already set). CSS hex stays on
  stylelint.

### Consequences

- Positive: the linter is faster, on the unified oxc path, and aligned with the native TypeScript compiler.
- Positive: ESLint is fully removed: no eslint packages and no bridges remain.
- Positive: React Compiler safety is enforced now, so code is compiler-safe before the transform is enabled.
- Negative: the `strictTypeChecked` class runs through `oxlint-tsgolint` and must be version-paired with TypeScript
  when the native compiler lands.
- Negative: docstring presence and the inline-style ban become review-level, not machine-gated. The inline-style ban
  also has no Content-Security-Policy backstop, since `style-src` permits `unsafe-inline` for Tailwind. A separate
  frontend-standards review revisits both.

## More information

This decision superseded two earlier decisions, both retired: a decision to replace eslint-plugin-react with
@eslint-react (retired; history holds the record), and a decision to lint frontend docstrings via
eslint-plugin-jsdoc, whose docstring-presence rule this decision drops (see Decision outcome above).

This decision amends [strict lint policy](./0002-strict-lint-policy-pedantic-clippy-and-strict-frontend-lint.md), whose
frontend engine is now oxlint, and [tiered comment policy](./0004-tiered-comment-policy-for-an-open-source-codebase.md),
whose frontend docstring floor is now review-level rather than lint-enforced.

Formatting moved to oxfmt in a paired decision, [adopt oxfmt](../../adr/2026-06-28-adopt-oxfmt-formatter.md). Further
follow-ups: enabling the React Compiler transform, the migration to the native TypeScript compiler with
`oxlint-tsgolint` version-pairing, and a rule-by-rule review of the frontend authoring standards against current
practice.
