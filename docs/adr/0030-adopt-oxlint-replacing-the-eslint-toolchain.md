---
type: ADR
profile-version: 1
id: "REV-ADR-0030"
title: "Adopt oxlint, replacing the ESLint toolchain"
status: "superseded"
recorded-on: "2026-09-05"
decided-on: "2026-06-27"
decision-makers:
  - "John Unkovich"
superseded-by:
  - "REV-ADR-0079"
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
were unexamined house-style. Three frontend enforcements needed explicit evaluation during the port: the object-literal
`as`-cast ban, docstring-presence linting, and the React rule layer.

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
the full removal of ESLint while preserving industry-standard safety checks and shedding bespoke house-style rules. The
JS-plugin bridge option is rejected because it fails the full-removal driver; staying on ESLint is rejected by the
toolchain direction.

### Consequences

- Positive: the linter is faster, on the unified oxc path, and aligned with the native TypeScript compiler.
- Positive: ESLint is fully removed: no eslint packages and no bridges remain.
- Positive: React Compiler safety checks run ahead of enabling the transform.
- Negative: the `strictTypeChecked` class runs through `oxlint-tsgolint` and must stay version-paired with TypeScript
  when the native compiler lands.
- Negative: docstring presence and the inline-style ban become review-level conventions rather than lint-enforced gates,
  and `style-src` permits `unsafe-inline` for Tailwind so CSP does not backstop inline styles.
