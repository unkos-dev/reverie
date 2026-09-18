---
severity: low
surfaces: [ci]
adopted: 2026-07-26
adopted-because: kache-action's only tag, v1, predated the `namespace` and `pr-comment` inputs the backend cache step depends on; GitHub discards an undeclared input with a warning rather than an error, so pinning to the tag left both settings silently inert
lift-when-class: upstream
lift-when: kunobi-ninja/kache-action publishes a vX.Y.Z tag containing commit d71ab254, at which point the pin moves to that tag and Renovate tracks it by version
---

# kache-action digest pinned past its only tag

## Constraint

`.github/workflows/backend.yml` and `.github/workflows/codeql.yml` pin `kunobi-ninja/kache-action` to commit
`d71ab254aa8c20bdfa2dc38856611f5e78678cc6` on the upstream default branch, and the trailing `# main` comment makes
Renovate track that branch.

The action publishes one tag, `v1`, and moves it. It points at `78ff455`, which declares the `namespace` and
`pr-comment` inputs the backend cache step passes. The pin is four commits past it, including the fix that skips the
GitHub cache save after an exact-key restore.

GitHub does not fail a workflow that passes an input the action does not declare. It emits
`##[warning]Unexpected input(s)` and drops the value, so an input added to the step must be checked against `action.yml`
at the pinned revision.

## Workaround

The pin names a default-branch commit. Renovate proposes a digest update whenever upstream `main` moves, and those
updates stay manual: `renovate.json` automerges digest updates only for `actions/**` and `github/**`.

## Why this isn't the right shape

An unreleased commit carries no changelog and no release testing. Pinning the moving `v1` tag instead would not help:
`helpers:pinGitHubActionDigestsToSemver` tracks only tags with a full version, so every other action is followed by
version while this one is followed by commit.

## Lift conditions

Upstream publishes a `vX.Y.Z` tag containing `d71ab254`. The pin then moves to that tag's digest with an exact-version
comment, matching every other action in the workflows.

## Related

- `docs/adr/0043-remote-rust-build-cache-on-object-storage.md`
- `debt/2026-07-26-kache-shard-upload-step.md`
