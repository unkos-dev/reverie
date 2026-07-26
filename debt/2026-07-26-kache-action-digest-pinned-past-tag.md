---
severity: low
surfaces: [ci]
adopted: 2026-07-26
adopted-because: kache-action's only tag, v1, predates the `namespace` and `pr-comment` inputs the backend cache step depends on; GitHub discards an undeclared input with a warning rather than an error, so pinning to the tag left both settings silently inert
lift-when-class: upstream
lift-when: kunobi-ninja/kache-action publishes a tag containing commit a257c05, at which point the digest pin returns to that tag
---

# kache-action digest pinned past its only tag

## Constraint

`.github/workflows/ci.yml` pins `kunobi-ninja/kache-action` to commit
`a257c055543c2840700a9bbca8f9c3094a421b1b`, which is the head of the
upstream default branch rather than a released tag.

The action has exactly one tag, `v1`, published 2026-06-15 and never
re-pointed. Two inputs the backend cache step depends on arrived after
it:

- `pr-comment`, added in upstream #8 on 2026-06-15
- `namespace`, added in upstream #12 on 2026-06-19

GitHub does not fail a workflow that passes an input the action does not
declare. It emits `##[warning]Unexpected input(s)` and drops the value.
Pinned at `v1`, both settings were accepted by the workflow, discarded by
the runner, and the job stayed green: prefetch shards were never
uploaded, and no comment was withheld by the setting that appeared to
withhold it.

## Workaround

The pin names the commit that declares both inputs, with a comment on the
step recording that the digest is the contract and that any input added
there must be checked against `action.yml` at that exact revision rather
than at the upstream default branch.

## Why this isn't the right shape

An unreleased commit carries no changelog and no release testing. The
seven commits between `v1` and this one include Windows runner support
and a test refresh, none of which were exercised by a release. Renovate
tracks the pin by the branch named in its trailing comment, so it will
propose digest bumps as upstream moves, and each one silently changes the
contract the step is written against.

Every other action in this repository is pinned to a digest that
corresponds to a published release.

## Lift conditions

Upstream publishes any tag containing `a257c05`. The pin then moves to
that tag's digest and the trailing comment names the version, matching
every other action in the workflow.

## Related

- `adr/2026-07-26-remote-build-cache-on-r2.md`
- `debt/2026-07-26-kache-shard-upload-step.md`
