---
type: ADR
profile-version: 1
id: "REV-ADR-0032"
title: "Adopt oxfmt, replacing Prettier"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-28"
decision-makers:
  - "John Unkovich"
---

# Adopt oxfmt, replacing Prettier

## Context and problem statement

[Adopt oxlint](./0030-adopt-oxlint-replacing-the-eslint-toolchain.md) moved the JS/TS lint engine to the oxc project
but left formatting on Prettier (`prettier --check .` in the `repo-lint` CI job, `prettier --write` in lint-staged).
This record moves formatting to oxfmt, the oxc formatter, so lint and format share one engine.

The strategic force matches the oxlint record: the oxc family is native Rust and an order of magnitude faster than
the Node tooling it replaces, and converging on one toolchain removes a dependency. The constraint is parity. The
swap must not reflow the existing tree, must keep every path the ignore set protected, and must cover at least the
file types Prettier gated.

## Decision drivers

- One oxc toolchain for both lint and format.
- Full Prettier removal. No `prettier` dependency and no `.prettierrc` or `.prettierignore` left behind.
- No reflow churn. The committed tree is Prettier-formatted, so oxfmt's output has to match it on the types Prettier
  already handled.
- No silent coverage loss. A type the old gate checked must stay checked.
- No workspace-image rebuild. oxfmt ships as an npm devDependency, the same path oxlint took.

## Considered options

- oxfmt as the sole formatter
- Keep Prettier
- Split the tree between oxfmt and a retained Prettier

## Decision outcome

Chosen option: **oxfmt as the sole formatter**, because a dry run over the whole tree settled the split question
empirically. With the Prettier options mapped, oxfmt reformats one file, `backend/Cargo.toml`, and leaves every other
file byte identical. Markdown, MDX, YAML, CSS, JSON, and HTML all match Prettier's output, so no type needs a
retained-Prettier fallback. The split option is rejected. Keeping Prettier is rejected by the toolchain direction.

The forcing details resolved as follows.

- Options come from the Prettier config, mapped by `oxfmt --migrate prettier` and now held in the root
  `vite.config.ts` fmt block: `semi`, double quotes, trailing commas everywhere, a 100-column print width, two-space
  indent, preserved prose wrapping, and Unix line endings. `proseWrap` and `endOfLine` map directly in this oxfmt
  version, so neither relies on `.editorconfig`.
- The ignore set lives in that same fmt block as `ignorePatterns`, root-relative, including `CHANGELOG.md` and the
  drift-gated generated files that a reflow would corrupt.
- TOML formatting is new coverage. Prettier has no TOML parser, so the four `.toml` files never passed through the
  old gate. oxfmt formats TOML, and only `Cargo.toml` drifts: it wraps one over-long dependency array and drops the
  hand-aligned columns on three lines. Keeping the four config files on one formatter beats excluding them or adding
  a second TOML formatter for so small a surface.
- Package-key sorting stays off. oxfmt's `sortPackageJson` defaults on; Prettier never reordered keys, so the
  migrated config disables it to hold parity.
- The local glob widens to match the CI gate. The old lint-staged glob skipped `.mjs`, `.html`, `.mdx`, `.jsonc`, and
  `.toml`, which the whole-tree `--check` still gates, so an edit to one of those could pass pre-commit and fail CI.
  The oxfmt glob covers every type oxfmt formats.

### Consequences

- Positive: lint and format run on one engine, the format pass is faster, and the output matches the prior Prettier
  formatting.
- Positive: Prettier is fully removed, with no dependency, no config files, and no CI or hook reference.
- Positive: TOML now has a format gate and the local pre-commit pass covers the same types as CI.
- Neutral: oxfmt is pre-1.0. Its output can shift between releases, so the dependency is pinned to an exact version
  and any upgrade surfaces as a reviewable diff on the bump rather than as silent drift.

## More information

Pairs with [Adopt oxlint](./0030-adopt-oxlint-replacing-the-eslint-toolchain.md), which moved linting to the same
oxc toolchain. Together they complete the lint-and-format swap to oxc for the JS/TS surface.
