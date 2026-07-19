---
status: accepted
date: 2026-06-30
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Adopt the Vite+ (vp) monorepo toolchain

## Context and Problem Statement

[Adopt oxlint](2026-06-27-adopt-oxlint-toolchain.md) and
[adopt oxfmt](2026-06-28-adopt-oxfmt-formatter.md) moved the JS/TS lint and format
engines to the oxc project, but each ran standalone with its own config, and the
repo stayed three separate npm trees (`frontend`, `docs`, root). Formatting,
linting, building, and testing were driven by four loosely related commands over
two config files.

This record completes the pivot: a single Vite+ (`vp`) toolchain drives the repo
as one npm workspace, with lint and format configuration unified in a root
`vite.config.ts`. The forces are convergence (one toolchain, one config home) and
parity (no enforcement, build, or test behavior may regress in the move).

## Decision Drivers

- One toolchain over the JS/TS/CSS/MD plane, configured in one place.
- Monorepo orchestration: `vp run -r` across `frontend` and `docs`.
- A pinned, reproducible runtime: a baked global `vp` plus `setup-vp` in CI.
- No silent enforcement loss when the type-aware pass becomes the sole typecheck.

## Considered Options

- **Vite+ monorepo.** Root `vite.config.ts` owns fmt + lint; `frontend` and `docs`
  are npm workspaces; `vp` drives every gate.
- **Keep standalone oxlint + oxfmt over three npm trees** (the post-oxfmt state).
  Two config files, no workspace orchestration, and whole-tree formatting needs a
  separate standalone binary.
- **Scope vp to `frontend` only, keep `docs` standalone.** Rejected: a single
  whole-tree fmt config requires `docs` to be in vp's scope, and astro builds
  cleanly on vp's vite fork, so the split buys nothing.

## Decision Outcome

Chosen option: **Vite+ monorepo.** The forcing details resolved as follows.

- **One root `vite.config.ts` owns fmt and lint.** The `fmt` block is the sole
  oxfmt config and formats the whole tree (root-relative ignores); the `lint` block
  holds the frontend rules with every override scoped to `frontend/**`. The
  standalone `oxfmt` dependency, `.oxfmtrc.json`, and the per-package fmt block are
  gone. `frontend/vite.config.ts` keeps only build, server, and test config.
- **npm workspaces.** The root declares `workspaces: ["frontend", "docs"]` for one
  lockfile and one hoisted `node_modules`. npm honors `overrides` only in the
  workspace root, so all pins consolidate there; astro runs on vp's
  `@voidzero-dev/vite-plus-core` fork, so a single global `vite` override serves
  both packages.
- **tsgo is the sole typechecker.** `vp lint` runs the type-aware pass over the
  same app and node scope `tsc -b` covered, so the separate `tsc -b` build step is
  removed. Because a missing type-aware engine exits zero silently, a test asserts
  the pass fires on a known type-aware violation.
- **vp is a baked global binary plus a `vite-plus` root devDependency.** The config
  loader resolves `vite-plus` from the project `node_modules`, so the root config
  needs it as a dependency even though the binary is global. `npm` and `npx`
  resolve through vp shims (`vp env setup`) so the `devEngines.packageManager`
  version is honored.
- **CI bootstraps vp via `voidzero-dev/setup-vp` (SHA-pinned) and one root
  `vp install`.** The `just` recipes that define each gate are unchanged.

### Consequences

- Good, because one toolchain drives lint, format, typecheck, build, and test from
  one config, and `vp run -r` orchestrates both packages.
- Good, because a single lockfile and hoisted `node_modules` replace three trees,
  and the lockfile pins every platform's native binding.
- Neutral, because vp is pre-release: the binary is pinned, and the workspace image
  must run `vp env setup` so the npm shims exist, or bare `npm` falls through to a
  system version that rejects the package-manager pin.
- Bad, because hoisting moves dependencies to the root `node_modules`: scripts or
  tests that assumed `frontend/node_modules` must resolve through Node module
  resolution instead of a fixed relative path.

### Confirmation

`vp check` gates format, lint, and typecheck; `vp run -r build` builds both
packages with the design-chunk gate and CSP sidecar intact; `npm ci` reproduces the
workspace from the root lockfile; CI runs the gates through `setup-vp` plus one root
`vp install`. The drift-gated generated files and `CHANGELOG.md` stay byte
identical through a format pass.

## More Information

Pairs with [adopt oxlint](2026-06-27-adopt-oxlint-toolchain.md),
[adopt oxfmt](2026-06-28-adopt-oxfmt-formatter.md), and
[adopt lefthook](2026-06-28-adopt-lefthook-git-hooks.md), which moved lint, format,
and git hooks to this toolchain. The workspace-image bake that runs `vp env setup`
for fresh workspaces is a homelab follow-up.
