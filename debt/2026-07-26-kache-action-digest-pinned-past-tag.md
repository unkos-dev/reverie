---
severity: low
surfaces: [ci]
adopted: 2026-07-26
adopted-because: kache-action's only tag, v1, predated the `namespace` and `pr-comment` inputs the backend cache step depends on; GitHub discards an undeclared input with a warning rather than an error, so pinning to the tag left both settings silently inert
lift-when-class: upstream
lift-when: kunobi-ninja/kache-action publishes a vX.Y.Z tag containing commit 1a33fb2f, at which point the pin moves to that tag and Renovate tracks it by version
---

# kache-action digest pinned past its only tag

## Constraint

`.github/workflows/backend.yml` and `.github/workflows/codeql.yml` pin `kunobi-ninja/kache-action` to commit
`1a33fb2ff51be23eb9e87abeae6edb65be78f71c` on the upstream default branch, and the trailing `# main` comment makes
Renovate track that branch.

The action publishes one tag, `v1`, and moves it. It points at `ad73175`, which declares every input both cache steps
pass except `strict`. The pin is two commits past it: `strict`, which both steps set, and a fix that reads the daemon
remote from the current kache stats layout.

GitHub does not fail a workflow that passes an input the action does not declare. It emits
`##[warning]Unexpected input(s)` and drops the value, so an input added to the step must be checked against `action.yml`
at the pinned revision.

## Workaround

The pin names a default-branch commit. Renovate proposes a digest update whenever upstream `main` moves, and those
updates stay manual, because `renovate.json` limits automerged digest updates to `actions/**` and `github/**`.

## Why this isn't the right shape

An unreleased commit carries no changelog and no release testing. Pinning the moving `v1` tag instead would not help:
`helpers:pinGitHubActionDigestsToSemver` tracks only tags with a full version, so every other action is followed by
version while this one is followed by commit.

## Lift conditions

Upstream publishes a `vX.Y.Z` tag containing `1a33fb2f`. The pin then moves to that tag's digest with an exact-version
comment, matching every other action in the workflows.

## Related

- `docs/adr/0043-remote-rust-build-cache-on-object-storage.md`
- `debt/2026-07-26-kache-shard-upload-step.md`
