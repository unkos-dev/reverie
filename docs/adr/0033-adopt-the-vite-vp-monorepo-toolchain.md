---
type: ADR
profile-version: 1
id: "REV-ADR-0033"
title: "Adopt the Vite+ (vp) monorepo toolchain"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-30"
decision-makers:
  - "John Unkovich"
---

# Adopt the Vite+ (vp) monorepo toolchain

## Context and problem statement

[Adopt oxlint](./0030-adopt-oxlint-replacing-the-eslint-toolchain.md) and
[adopt oxfmt](./0032-adopt-oxfmt-replacing-prettier.md) moved the JS/TS lint and format engines to the oxc project,
but each ran standalone with its own config, and the repo stayed three separate npm trees (`frontend`, `website`,
root). Formatting, linting, building, and testing were driven by four loosely related commands over two config
files.

This record completes the pivot: a single Vite+ (`vp`) toolchain drives the repo as one workspace, with lint and
format configuration unified in a root `vite.config.ts`. The forces are convergence (one toolchain, one config
home) and parity (no enforcement, build, or test behaviour may regress in the move).

## Decision drivers

- One toolchain over the JS/TS/CSS/MD plane, configured in one place.
- Monorepo orchestration: `vp run -r` across `frontend` and `website`.
- A pinned, reproducible runtime: a baked global `vp` plus `setup-vp` in CI.
- No silent enforcement loss when the type-aware pass becomes the sole typecheck.

## Considered options

- **Vite+ monorepo.** Root `vite.config.ts` owns fmt and lint; `frontend` and `website` are workspace projects;
  `vp` drives every gate.
- **Keep standalone oxlint and oxfmt over three npm trees** (the post-oxfmt state). Two config files, no workspace
  orchestration, and whole-tree formatting needs a separate standalone binary.
- **Scope vp to `frontend` only, keep `website` standalone.** Rejected: a single whole-tree fmt config requires
  `website` to be in vp's scope, and astro builds cleanly on vp's vite fork, so the split buys nothing.

## Decision outcome

Chosen option: **Vite+ monorepo**, because it gives the workspace one toolchain and one config home while
preserving parity with the prior lint, format, build, and test behaviour.

The forcing details resolved as follows.

- **One root `vite.config.ts` owns fmt and lint.** The `fmt` block is the sole oxfmt config and formats the whole
  tree (root-relative ignores); the `lint` block holds the frontend rules with every override scoped to
  `frontend/**`. The standalone `oxfmt` dependency, `.oxfmtrc.json`, and the per-package fmt block are gone.
  `frontend/vite.config.ts` keeps only build, server, and test config.
- **A pnpm workspace.** `pnpm-workspace.yaml` declares the projects, one lockfile, and a catalog that holds each
  shared pin once. Overrides live beside the catalog and apply across the workspace, so pins consolidate in one
  file; astro runs on vp's `@voidzero-dev/vite-plus-core` fork, so a single `vite` override serves every project.
  The layout is isolated rather than hoisted: each project gets its own `node_modules` linking into a shared
  virtual store.
- **tsgo is the sole typechecker.** `vp lint` runs the type-aware pass over the same app and node scope `tsc -b`
  covered, so the separate `tsc -b` build step is removed. Because a missing type-aware engine exits zero silently,
  a test asserts the pass fires on a known type-aware violation.
- **vp is a global binary plus a `vite-plus` root devDependency.** The config loader resolves `vite-plus` from the
  project `node_modules`, so the root config needs it as a dependency even though the binary is global. vp
  downloads the package manager named by `packageManager` in the root `package.json`, which is what keeps every
  invocation on the declared pnpm.
- **CI bootstraps vp via `voidzero-dev/setup-vp` (SHA-pinned) and one root `vp install`.** The `just` recipes that
  define each gate are unchanged.

### Consequences

- Positive: one toolchain drives lint, format, typecheck, build, and test from one config, and `vp run -r`
  orchestrates both packages.
- Positive: a single lockfile replaces three, and it pins every platform's native binding.
- Positive: the isolated layout means a project can only import what it declares: an undeclared transitive
  dependency fails at resolution rather than working by accident until the graph shifts.
- Negative: vp is pre-release, so the binary is pinned.
- Negative: tooling that walks `node_modules` must account for the virtual store: a scanner pointed at the
  workspace root sees the root's own dependencies, not a project's.

## More information

Pairs with [adopt oxlint](./0030-adopt-oxlint-replacing-the-eslint-toolchain.md),
[adopt oxfmt](./0032-adopt-oxfmt-replacing-prettier.md), and
[adopt lefthook](./0031-adopt-lefthook-replacing-husky-and-lint-staged.md), which moved lint, format, and git hooks
to this toolchain. The workspace-image bake that runs `vp env setup` for fresh workspaces is a follow-up for the
arm64 staging host.
