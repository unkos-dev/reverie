---
type: ADR
profile-version: 1
id: "REV-ADR-0031"
title: "Adopt lefthook, replacing husky and lint-staged"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-28"
decision-makers:
  - "John Unkovich"
---

# Adopt lefthook, replacing husky and lint-staged

## Context and problem statement

The repository ran its git hooks on two Node tools: husky wired the hooks by pointing `core.hooksPath` at its own
dispatch directory, and lint-staged mapped file globs to commands and re-staged formatter output. Which tool should own
`pre-commit`, `pre-push`, and `commit-msg` going forward?

The force is a single polyglot runner with declarative, parallel-capable configuration and no Node-only hook layer. The
constraint is parity: every check that blocked a commit or a push before has to block on the same files, in the same
order, with the same advisory-or-gating posture. A direct port is not automatic, because lefthook installs to a
different hook path and matches globs with a different engine.

## Decision drivers

- One binary for all three hooks, with no separate staged-file layer.
- Declarative configuration in one file.
- Parity: the same checks, on the same files, blocking the same way.
- A faithful backend push gate: format, clippy, and generated-artifact drift, always-run and stop-on-first-failure.
- An exact-pinned devDependency with the lockfile committed, matching the install path the oxc toolchain records already
  established.

## Considered options

- lefthook as the sole hook runner
- Keep husky and lint-staged
- A language-agnostic hook framework with its own runtime

## Decision outcome

Chosen option: **lefthook as the sole hook runner**, because it is a single polyglot binary with declarative,
parallel-capable configuration, and it removes the Node-only staged-file layer that lint-staged added.

Lefthook owns the repository hooks, with matching semantics that cover nested files, formatter changes staged before
commit, and read-only checks run without index races. Frontend linting is included in the local hook surface.

### Consequences

- Positive: one binary owns all three hooks, the configuration is declarative, and the read-only checks run in parallel.
- Positive: husky and lint-staged are fully removed: no dependency, no dispatch directory, no separate config file, no
  comment references.
- Positive: pre-commit now catches the lint and type-aware lint classes locally that previously surfaced only in
  continuous integration.
- Negative: the type-aware lint adds a fixed cost to any commit that stages TypeScript, since the engine loads the whole
  project graph regardless of how many files are staged. It is gated to frontend TypeScript, so a commit that stages
  none of it skips the pass.

## Pros and cons of the options

### lefthook as the sole hook runner

- Positive: one binary owns all three hooks, with declarative configuration and parallel-capable job groups.
- Negative: its default glob engine and hook install path both differ from the prior tools, so the configuration cannot
  be a blind port.

### Keep husky and lint-staged

- Negative: two installers, a Node-only staged-file layer, and husky on a deprecation path.

### A language-agnostic hook framework with its own runtime

- Negative: adds a separate runtime to a repository whose tooling is converging on Rust and Node binaries.

## More information

This record pairs with the oxlint record
([Adopt oxlint, replacing the ESLint toolchain](./0030-adopt-oxlint-replacing-the-eslint-toolchain.md)) and the oxfmt
record ([Adopt oxfmt formatter](./0032-adopt-oxfmt-replacing-prettier.md)), which moved linting and formatting to the
oxc toolchain; it moves the hook runner those passes execute under. The pre-commit commands invoke the tools and scripts
directly against the staged-file list rather than the whole-tree task recipes, so a commit scans only its staged files
while the whole-tree recipes back the continuous-integration gates.
