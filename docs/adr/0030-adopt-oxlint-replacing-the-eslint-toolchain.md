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

The frontend lint stack ratified in
[strict lint policy](./0002-strict-lint-policy-pedantic-clippy-and-strict-frontend-lint.md) ran on ESLint plus
typescript-eslint (`strictTypeChecked`), `eslint-plugin-react-hooks`, `eslint-plugin-react-refresh`, `@eslint-react`,
and `eslint-plugin-jsdoc`. This decision moves the JS/TS lint engine to oxlint, the Rust linter from the VoidZero oxc
project, as the first step toward a unified oxc toolchain (oxlint for linting, oxfmt for formatting, a shared runner
later).

The strategic force: the oxc family is an order of magnitude faster (native Rust), converges lint and format on one
engine, and is forward-aligned with TypeScript 7. Its type-aware linter, `oxlint-tsgolint`, is built on `typescript-go`,
the same native engine as the TypeScript 7 compiler.

The constraint: the swap must preserve every enforcement that is a genuine industry standard while shedding rules that
were unexamined house-style. Three frontend enforcements had no mechanical oxlint port and forced a real decision: the
object-literal `as`-cast ban, docstring-presence linting, and the React rule layer.

## Decision drivers

- One oxc toolchain, and lint speed.
- Full ESLint removal. No `eslint-plugin-*` left installed: the JS-plugin bridge keeps the ESLint ecosystem alive and is
  rejected.
- Preserve standards, shed house-style. A rule survives only if it is current industry practice or covered by a stronger
  control, not because it was previously enabled.
- Forward-align the type-aware path with `typescript-go` now.
- No silent enforcement loss: each surviving surface is proven to fire on a deliberate violation.

## Considered options

- oxlint, native rules only, with config-driven type-aware via `oxlint-tsgolint`
- oxlint with `@eslint-react` and `eslint-plugin-jsdoc` loaded through the JS-plugin bridge
- Stay on ESLint

## Decision outcome

Chosen option: **oxlint, native rules only, with config-driven type-aware via `oxlint-tsgolint`**, because it completes
the full removal of ESLint while preserving every enforcement that is a genuine industry standard. The JS-plugin bridge
option is rejected because it fails the full-removal driver; staying on ESLint is rejected by the toolchain direction.

The native rules retain the type-aware safety checks, fetch centralisation, and React correctness baseline.
Docstring-presence enforcement and the opinionated React plugin layer are dropped; the enum restriction remains with the
type checker. These are deliberate changes to the lint policy, not replacements hidden behind a compatibility bridge.

### Consequences

- Positive: the linter is faster, on the unified oxc path, and aligned with the native TypeScript compiler.
- Positive: ESLint is fully removed: no eslint packages and no bridges remain.
- Positive: React Compiler safety is enforced now, so code is compiler-safe before the transform is enabled.
- Negative: the `strictTypeChecked` class runs through `oxlint-tsgolint` and must be version-paired with TypeScript when
  the native compiler lands.
- Negative: docstring presence and the inline-style ban become review-level, not machine-gated. The inline-style ban
  also has no Content-Security-Policy backstop, since `style-src` permits `unsafe-inline` for Tailwind. A separate
  frontend-standards review revisits both.

## More information

This decision superseded two earlier decisions, both retired: a decision to replace eslint-plugin-react with
@eslint-react (retired; history holds the record), and a decision to lint frontend docstrings via eslint-plugin-jsdoc,
whose docstring-presence rule this decision drops (see Decision outcome above).

This decision amends [strict lint policy](./0002-strict-lint-policy-pedantic-clippy-and-strict-frontend-lint.md), whose
frontend engine is now oxlint, and [tiered comment policy](./0004-tiered-comment-policy-for-an-open-source-codebase.md),
whose frontend docstring floor is now review-level rather than lint-enforced.

Formatting moved to oxfmt in a paired decision, [adopt oxfmt](./0032-adopt-oxfmt-replacing-prettier.md). Further
follow-ups: enabling the React Compiler transform, the migration to the native TypeScript compiler with
`oxlint-tsgolint` version-pairing, and a rule-by-rule review of the frontend authoring standards against current
practice.
