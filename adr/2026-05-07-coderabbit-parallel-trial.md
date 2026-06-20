---
status: accepted
date: 2026-05-07
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# CodeRabbit AI code review: parallel trial alongside Greptile

## Context and Problem Statement

[`adr/2026-05-04-greptile-trial.md`](2026-05-04-greptile-trial.md)
ratified a 4-week trial of Greptile, structured around a per-PR
actionable-rate metric. By 2026-05-07 (3 days into the trial)
Greptile's signal has been strong: 16 findings across 9 PRs at
81% actionable, with multiple differentiator-class catches
(cross-document, cross-PR, cross-file consistency) that lint cannot
express.

Operational issues have surfaced in the same window that the signal
has not:

1. The trial-tier 50-review per-author cap was hit on 2026-05-07,
   exhausting review capacity for the maintainer (`junkovich`)
   despite a confirmed OSS discount applied 2026-05-04. Support
   ticket filed; resolution pending. Whether the cap is a
   billing-tier flag or a separate per-account counter is unclear
   from the dashboard
2. Confidence-score updates on fix commits burn an additional
   review credit, and the original config (`triggerOnUpdates: true`)
   spent one review per push on iteration-heavy PRs. PR #171
   amended the config to label-gated to mitigate, but for PRs
   already reviewed under the old model, the burn is sunk
3. Greptile's trial tier does not post a formal GitHub Pull Request
   Review (status check). Branch protection has nothing to gate on;
   the PR shows green-to-merge while a Greptile review is in
   progress

CodeRabbit is the closest competitor with a public-repos-free
posture. Two observations distinguish it operationally:

- **No fixed lifetime cap.** Free OSS tier appears rate-limited
  (per-hour windows) rather than capped at a fixed lifetime
  number. The exact rate-limit values warrant verification at
  install time, but the structural difference matters: a
  per-hour rate limit caps the _rate_ of iteration on
  push-heavy PRs, not the _total volume_ of review activity over
  weeks
- **Formal PR Review with status.** CodeRabbit posts a GitHub Pull
  Request Review (with state) by default, not just inline comments.
  Branch protection can gate on it. Greptile's trial-tier
  comment-only posture is documented as a gating gap in
  `2026-05-04-greptile-trial.md`; CodeRabbit closes it natively

The original Greptile ADR explicitly anticipated this:

> CodeRabbit / Diamond / Codium / other AI reviewers. Deferred:
> Greptile's graph-based codebase context is the most differentiated
> angle, and 4 weeks of one tool generates cleaner signal than 1
> week of four. If the Greptile trial fails, the next ADR can pick
> up an alternative with a clean comparison baseline

Greptile has not failed on signal. It has run into operational
constraints. The framing for this followup is therefore not
"Greptile rejected, CodeRabbit adopted" but **"Greptile signal
strong but operationally constrained, CodeRabbit trialled in
parallel as an operationally-equivalent alternative"**.

## Decision

Run a parallel trial of CodeRabbit alongside Greptile starting
2026-05-07. Both reviewers active concurrently. Greptile remains
under the PR #171 label-gate (no auto-burn) so it draws zero quota
while inactive; the maintainer can opt in by applying the
`greptile-review` label per-PR. CodeRabbit runs auto on every PR
under its default configuration until the rate-limit shape is
empirically known.

### Trial configuration

`.coderabbit.yaml` at repo root, modelled on the Greptile config
shape:

- `reviews.profile: chill`: comparable to Greptile `strictness: 1`
  in spirit (verbose-by-design for trial calibration)
- `reviews.path_instructions`: pin per-path conventions matching
  Greptile's `customRules` (time-not-chrono, no-raw-hex, no-enum,
  no-inline-style, shadcn carve-out, secret-handling stance,
  TDD requirement, Conventional Commits)
- `reviews.path_filters`: exclude `**/package-lock.json`,
  `**/Cargo.lock` (matches Greptile's `ignorePatterns` carve-out
  for the lockfile hallucination class)
- `reviews.auto_review.enabled: true`, `reviews.auto_review.drafts: false`
  : auto on every non-draft PR. No per-push-burn concern under
  rate-limit pricing the way it was a fixed-cap concern on Greptile
- `reviews.tools.markdownlint.enabled: false`: markdown lint
  already enforced via repo `.markdownlint-cli2.jsonc`. Don't
  duplicate
- `reviews.review_status: true`: formal PR Review (status check)
  enabled. The gating-gap closer relative to Greptile

### App install

Done by the maintainer via GitHub App marketplace
(<https://github.com/apps/coderabbitai>) on the `unkos-dev/reverie`
repo only (not org-wide). OSS plan applied at signup time per
CodeRabbit's "free for public repos" terms.

### Trigger model

Auto on every non-draft PR. No label gate (rate-limit pricing
removes the lifetime-burn concern that motivated Greptile's
label-gate amendment under PR #171).

If empirical rate-limit values turn out tight enough to recreate
the per-push-burn problem (e.g. < 4 PR reviews/hour on iteration-
heavy days), this ADR is amended with a label-gate identical to
Greptile's, applied to CodeRabbit. Until that's observed,
auto-on-every-PR is the cleaner default.

### Parallel-trial gate

**Duration**: 2 weeks (2026-05-07 → 2026-05-21). Shorter than
Greptile's 4-week gate because:

- Greptile's existing tally provides a baseline. The comparison
  signal is "does CodeRabbit produce comparable actionable-rate"
  not "does AI review work at all"
- Operational shape (rate-limit behaviour, status-check posture,
  rate of UI polish) is observable within the first week of use

**Success metrics:**

- **Signal**: CodeRabbit actionable-rate ≥ 30% (same threshold
  as Greptile)
- **Operations**: no rate-limit-induced iteration blockage on
  any PR during the trial window

Tally tracker: separate Linear ticket parallel to the trial tally, same
shape (per-PR finding rows, false-positive class breakdown,
trial observations).

**At the parallel-trial gate**, decide:

- **Adopt CodeRabbit, retire Greptile**: uninstall Greptile App,
  delete `greptile.json`, supersede `2026-05-04-greptile-trial.md`
  with this ADR (status: accepted)
- **Adopt both**: keep Greptile label-gated under PR #171 for
  cases where graph-based context catches matter; CodeRabbit
  default for every PR. Two reviewers in steady state. ADRs both
  flip to accepted
- **Retire CodeRabbit, retain Greptile**: uninstall CodeRabbit
  App, delete `.coderabbit.yaml`, this ADR flips to rejected
- **Retire both**: both have failed on signal or operations.
  Document and revisit later

Decision recorded by editing this ADR's status (and Greptile
ADR's status) to the matching outcome.

## Decision Outcome (2026-05-21 gate, ratified 2026-05-29)

**Adopt both** (matrix option 2). CodeRabbit cleared both gate
metrics:

- **Signal**: 11 of 12 tally rows actionable (92%), zero
  false-positives across the window, four positive observation
  classes documented (verbatim-move-masks-bug, structural-exposure,
  large-doc-PR exhaustive review, fix-feedback re-review). Well
  above the ≥30% threshold
- **Operations**: no rate-limit-induced iteration blockage observed
  on any PR during the trial window; the hourly-refill model never
  recreated Greptile's fixed-cap problem

CodeRabbit becomes the default reviewer (auto on every non-draft PR)
and closes Greptile's documented gating gap via its formal PR Review
status check.

Greptile is **retained label-gated, not retired**: its signal was
never the problem; the operational cap was, and it was lifted
2026-05-08 (see the Greptile ADR's Amendments). Its governance /
cross-document / public-API-surface surface is complementary, not
redundant: zero cross-tool overlap on PRs #180 and #188, with the
first observed overlap (PR #194, `# Errors` self-consistency) a
single finding among 14.

Both ADRs flip `proposed` → `accepted` per the in-place mechanism
above. Greptile retained as bigger-win on differentiator surface;
CodeRabbit as the operationally-robust default. Tallies for both trials closed at this gate.

**Watch** (re-read when next touching AI-review config; no separate
tracking ticket by design):

- Rate-limit-induced iteration blockage on push-heavy days. If
  observed, amend with a Greptile-style label gate applied to
  CodeRabbit (anticipated in the Trigger Model section above)
- CLI surface diluting the PR-side actionable-rate metric: if the
  CLI catches the easy wins pre-push, the PR-side number understates
  full reviewer signal (see the 2026-05-08 CLI amendment)
- Cross-tool overlap-rate with Greptile on docstring-heavy PRs
  (mirror of the Greptile ADR's watch-item)

## Consequences

- Good: direct head-to-head signal comparison on the same PRs
  during the parallel window. Cleaner data than sequential trials
  because the codebase, conventions, and PR shapes are constant
- Good: CodeRabbit's formal PR Review closes Greptile's
  documented gating gap. If branch protection gating becomes a
  requirement, CodeRabbit can satisfy it natively
- Good: operational hedge. If Greptile's quota issue persists
  past support resolution, CodeRabbit is already running and can
  pick up sole-reviewer duty without ramp-up delay
- Bad: review noise during parallel window. Two AI reviewers per
  PR plus the local multi-agent review (`prp-core:prp-review-agents`)
  plus the maintainer pass. PR comment volume goes up; triage cost
  goes up
- Bad: operational complexity. Two `*.json` / `*.yaml` configs to
  maintain, two ADR + tally pairs to track, two security
  disclosures in `CONTRIBUTING.md`. If both are retired, cleanup
  cost is correspondingly higher
- Bad: third-party data exposure widens. Reverie is AGPL-3.0 so
  the public-code surface is unchanged, but the org-internal
  `.claude/`, `adr/`, and `docs/superpowers/specs/` paths are now
  read by two providers instead of one. Acceptable for a public
  repo; not transferable to private repos without re-review
- Neutral: CodeRabbit's per-finding signal-to-noise ratio
  unknown until the first ~5 PRs run through it. The parallel
  window measures it directly

## Alternatives Considered

- **Switch to CodeRabbit, uninstall Greptile.** Rejected: bails
  on Greptile's strong signal data on operational grounds alone.
  Greptile's quota issue may resolve via support response, in
  which case the parallel-trial framing keeps the option open;
  outright switch loses it
- **Defer CodeRabbit until Greptile gate (2026-06-01 or
  early-success consideration trigger).** Rejected: Greptile is
  currently quota-walled, so the maintainer has zero functional
  AI review for ~10 days. Gap-filling via CodeRabbit is the
  pragmatic move regardless of which tool eventually wins
- **Trial both Diamond / Codium / other reviewers in parallel
  (3+ reviewers).** Rejected: review-comment fatigue scales
  with reviewer count, and the comparison signal degrades with
  more concurrent variables. Two is enough. If neither Greptile
  nor CodeRabbit clears the bar, a future ADR picks up a third
- **Run CodeRabbit on a separate branch / fork to isolate
  comparison.** Rejected: installation is per-repo. Forking
  for the trial loses the actual contribution shape (Renovate,
  Dependabot, real PR variety) that the comparison needs to be
  honest

## Amendments

### 2026-05-08: CLI wire-up for pre-push review

The original ADR scoped the trial to the **PR-side surface** (GitHub
App, auto-on-every-non-draft-PR). CodeRabbit also ships a CLI
(`~/.local/bin/coderabbit`, alias `cr`) that reviews uncommitted /
committed local changes against a base branch and emits structured
JSON via `--agent`, suitable for Claude Code ingestion.

Wire-up done in the coder workspace 2026-05-08:

- `curl -fsSL https://cli.coderabbit.ai/install.sh | sh` →
  `~/.local/bin/coderabbit` v0.4.5 (Linux ARM64)
- `coderabbit auth login` (browser OAuth)
- Claude Code plugin: `/plugin install coderabbit` →
  `/coderabbit:review` slash command available

#### Why add CLI to scope mid-trial

Two operational wins, both observed during the Greptile trial as
gaps that no PR-side reviewer can close:

- **Pre-push elimination**: findings raised and resolved in the
  local diff never produce review-comment churn on the PR. Reduces
  the PR-history noise the parallel trial otherwise widens (two AI
  reviewers per PR was already an explicit `Bad` consequence)
- **No round-trip**: addressing a finding pre-push removes the
  push → wait-for-review → re-push cycle. Direct token-cost saving
  for any agent-driven workflow that would otherwise paste PR
  comments back into a session

#### Rate-limit interaction

CLI reviews count against the OSS-tier hourly cap of 3 CLI
reviews/hour/dev (per <https://docs.coderabbit.ai/management/plans>).
This is shared with the same OSS quota the PR-side trial runs
against. A push-heavy iteration day could plausibly hit the cap.
If observed, document in the CLI pre-push review notes and consider
amending again with a session-level pacing discipline.

#### What stays unchanged

- PR-side trial framing, gate (2026-05-21), and decision matrix:
  unchanged
- `actionable-rate ≥ 30%` metric: measured on PR-side findings only.
  CLI findings are _not_ counted into the metric (they reduce the
  PR-side surface rather than add to it; they are tracked
  separately under the CLI pre-push review notes)
- `.coderabbit.yaml` config: unchanged. CLI inherits the same
  `path_instructions`, `path_filters`, `profile`, etc. from the
  in-repo config
- CONTRIBUTING.md § "Third-party AI code review": unchanged.
  Contributors do not invoke the CLI; only the maintainer does

#### What to watch

- CLI rate-limit hits during iteration-heavy days. If observed,
  this ADR is amended further (or trial gate brought forward)
- Whether CLI surface diverts review-quality findings away from
  the PR-side metric. If actionable-rate on PR-side falls because
  CLI catches the easy wins pre-push, the trial verdict needs
  to weight that explicitly, as the PR-side metric is no longer
  measuring the full reviewer signal
- Plugin / Claude Code integration friction. The CLI is rated for
  `cr --agent` JSON output; mismatches between what the slash
  command surfaces and the underlying `cr` output are integration
  signal, not reviewer signal

## More Information

- [`adr/2026-05-04-greptile-trial.md`](2026-05-04-greptile-trial.md)
  : the parent trial that this ADR runs alongside
- [`adr/2026-05-03-strict-lint-policy.md`](2026-05-03-strict-lint-policy.md)
  : the strict-lint baseline that compresses style territory
  for both AI reviewers
- CodeRabbit docs: <https://docs.coderabbit.ai>
- CodeRabbit pricing: <https://www.coderabbit.ai/pricing>
- CodeRabbit security and trust: <https://www.coderabbit.ai/trust-center>
- CodeRabbit CLI docs: <https://docs.coderabbit.ai/cli>
- CodeRabbit CLI + Claude Code integration: <https://docs.coderabbit.ai/cli/claude-code-integration>
- `CONTRIBUTING.md` § "Third-party AI code review", contributor
  disclosure (updated to product-agnostic framing in the same PR
  that lands this ADR)
