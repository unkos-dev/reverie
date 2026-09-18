---
type: ADR
profile-version: 1
id: "REV-ADR-0050"
title: "Package ingress: global controls with named exceptions, merged on required checks"
status: "proposed"
recorded-on: "2026-09-19"
decision-makers:
  - "John Unkovich"
---

# Package ingress: global controls with named exceptions, merged on required checks

## Context and problem statement

Third-party code enters the repository through the JavaScript and Rust package registries, pinned developer tools,
GitHub Actions, and container images. A control keyed to a version the repository does not own stops applying the moment
that version moves, silently rather than loudly. Merging every routine update by hand spends maintainer attention
without adding judgement, since most updates carry nothing for a human to weigh. What should govern how third-party
artifacts enter a build, and which of those entries still need a person to look at them?

## Decision drivers

- A lapsed control fails loudly rather than decaying into no control.
- No control rests on per-package bookkeeping keyed to a version the repository does not pin.
- Enforcement is mechanical, not reviewer memory.
- One policy applies across ecosystems.
- A dependency that cannot work without its install script stays installable.
- Human review is reserved for updates automation cannot judge.

## Considered options

- Global controls with named exceptions, merged on required checks and the release-age hold
- Per-package allowlists pinned to reviewed versions
- The package managers' defaults, unmodified
- Global controls with a human merge of every update

## Decision outcome

Chosen option: **global controls with named exceptions, merged on required checks and the release-age hold**, because a
default is the only form no version can void by moving underneath it, and required checks plus the hold do the work a
routine human merge did.

The controls are pinned and enforced managers, frozen installs, downloads verified against a committed lockfile, no
install-time code execution by default, a release-age hold, and audits of every ingested tree. An exception names its
package and records its reason. Updates merge automatically when required checks pass and the hold has elapsed. A human
reviews major versions, a released action tag that has been rewritten, pre-1.0 libraries linked into the product,
updates that need a manual step, and any third-party action update in a workflow whose job can publish, sign, or write
to the repository, because the review boundary is the privilege of the job a workflow runs, not the kind of update.

### Consequences

- Positive: no entry is keyed to a version the repository does not own.
- Positive: a dependency that starts shipping an install script fails the install rather than running it unreviewed.
- Positive: review attention concentrates on rewritten tags, majors, and privileged workflows, rather than spreading
  across every routine update.
- Negative: an automerged tool update reaches developer machines without a look.
- Negative: the release-age hold on action tags ages by commit date, which a publisher can backdate.
- Negative: a tool checksum is recorded on first download and trusted from then on.
- Negative: a transitive advisory absent from the relevant advisory database gets no automated fix pull request.
- Negative: cargo has no install-script control, so that control remains ecosystem-specific rather than spanning both
  stacks.
- Negative: an ad-hoc package fetch outside the workspace install is its own decision, uncovered by these controls.
- Negative: one CI tool resolves its own dependencies fresh on every run because its lock sidecar is not tracked.

## More information

Related decisions: REV-ADR-0033, REV-ADR-0035.

Reconsider when the JavaScript package managers gain per-workspace script policy, a dependency arrives that cannot work
without its install script, the dependency-update automation can raise fixes for transitive dependencies in the Rust or
JavaScript ecosystems, or action tag publish times become available to the release-age hold.
