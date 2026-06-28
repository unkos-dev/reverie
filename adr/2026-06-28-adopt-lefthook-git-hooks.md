---
status: accepted
date: 2026-06-28
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Adopt lefthook, replacing husky and lint-staged

## Context and Problem Statement

The repository ran its git hooks on two Node tools: husky wired the hooks by
pointing `core.hooksPath` at its own dispatch directory, and lint-staged mapped
file globs to commands and re-staged formatter output. This record replaces both
with lefthook, a single Go binary that owns `pre-commit`, `pre-push`, and
`commit-msg` from one declarative `lefthook.yml`.

The force is a single polyglot runner with declarative, parallel-capable
configuration and no Node-only hook layer. The constraint is parity: every check
that blocked a commit or a push before has to block on the same files, in the
same order, with the same advisory-or-gating posture. A direct port is not
automatic, because lefthook installs to a different hook path and matches globs
with a different engine.

## Decision Drivers

- One binary for all three hooks, with no separate staged-file layer.
- Declarative configuration in one file.
- Parity: the same checks, on the same files, blocking the same way.
- A faithful backend push gate: format, clippy, and generated-artifact drift,
  always-run and stop-on-first-failure.
- An exact-pinned devDependency with the lockfile committed, matching the install
  path the oxc toolchain records already established.

## Considered Options

- **lefthook as the sole hook runner.**
- **Keep husky and lint-staged.** Two installers, a Node-only staged-file layer,
  and husky on a deprecation path.
- **A language-agnostic hook framework with its own runtime.** Rejected for
  adding a separate runtime to a repository whose tooling is converging on Rust
  and Node binaries.

## Decision Outcome

Chosen option: **lefthook as the sole hook runner.** The pre-commit surface
reproduces the prior glob-to-command table; the push gate reproduces the backend
verification verbatim; commit-msg runs commitlint. The forcing details resolved
as follows.

- **The glob engine is pinned to doublestar.** The prior matcher treated a
  slashless pattern as a basename match at any depth and treated `**` as zero or
  more directories, so `backend/src/**/*.rs` covered `backend/src/main.rs`.
  lefthook defaults to an engine that matches slashless patterns at the root only
  and treats `**` as one or more directories, which would skip a file sitting
  directly under a base directory. The config sets `glob_matcher: doublestar` and
  writes every slashless pattern as `**/*.ext`, restoring basename-anywhere
  matching. A self-test stages a file directly under a base directory alongside a
  nested one and asserts both match.
- **Formatter output is re-staged through `stage_fixed`.** lint-staged re-staged
  rewritten files; lefthook does so only when a command opts in. Both oxfmt
  commands set `stage_fixed`, and a self-test confirms a staged unformatted file
  commits as its formatted bytes.
- **Partial staging stays faithful.** lefthook hides unstaged changes for the
  duration of the hook, so a file with one hunk staged and another left dirty
  commits only the staged hunk while the dirty hunk is preserved in the working
  tree. This matches the prior behaviour, so there is no divergence to accept.
- **The two formatter commands run sequentially, apart from the readers.**
  `stage_fixed` re-stages by calling `git add` after the command, and lefthook
  does not coordinate the git index across commands. Running both staging
  commands at once would race the index lock and abort otherwise-valid commits.
  The two formatter commands therefore form a sequential group, kept separate
  from a parallel read-only group. The same split keeps a formatter from
  rewriting a file while a reader validates the pre-format bytes. A self-test
  commits a YAML file and another formatted type together across repeated runs
  and asserts no index-lock failure.
- **The secret scan runs in the parallel read-only group.** It runs alongside the
  formatters rather than strictly ahead of them. The formatters only rewrite
  files on disk and create no commit, so any secret-scan finding still aborts the
  commit before it exists. The scan boundary is unchanged.
- **pre-commit gains the frontend linters.** The prior pre-commit ran neither
  oxlint nor stylelint; both ran in continuous integration. The hook now runs
  both on staged frontend files. oxlint is type-aware and loads the whole project
  graph, so the hook reports type-aware findings on staged files only: a
  pre-existing type-aware error in an untouched file does not block the commit,
  and the whole-project lint in continuous integration remains the full backstop.
  The type-aware engine is pinned to an exact version.
- **Install detects the stale hook path.** husky pointed `core.hooksPath` at its
  dispatch directory; lefthook installs shims into `.git/hooks` and the `prepare`
  script runs the install on every dependency install. An existing clone still
  carries the stale local `core.hooksPath`. lefthook detects the conflict and
  refuses to install with explicit guidance, so a contributor clears the setting
  once rather than discovering silently dead hooks. A fresh clone carries no such
  setting and installs cleanly.

### Consequences

- Good, because one binary owns all three hooks, the configuration is
  declarative, the read-only checks run in parallel, and the checks, files, and
  blocking behaviour match the prior stack.
- Good, because husky and lint-staged are fully removed: no dependency, no
  dispatch directory, no separate config file, no comment references.
- Good, because pre-commit now catches the lint and type-aware lint classes
  locally that previously surfaced only in continuous integration.
- Neutral, because the type-aware lint adds a fixed cost to any commit that
  stages TypeScript, since the engine loads the whole project graph regardless of
  how many files are staged. It is gated to frontend TypeScript, so a commit that
  stages none of it skips the pass.

### Confirmation

`lefthook validate` gates configuration validity and runs in the infra lint
aggregate. A behavioural self-test drives real commits through a throwaway
repository and asserts hook firing, the doublestar glob matrix, `stage_fixed`
re-staging, partial-stage fidelity, and an index-lock-free formatter group. The
push gate runs the backend format check, clippy, and the generated-artifact
drift tests, always-run and stop-on-first-failure.

## More Information

This record pairs with the oxlint and oxfmt records, which moved linting and
formatting to the oxc toolchain; it moves the hook runner those passes execute
under. The pre-commit commands invoke the tools and scripts directly against the
staged-file list rather than the whole-tree task recipes, so a commit scans only
its staged files while the whole-tree recipes back the continuous-integration
gates.
