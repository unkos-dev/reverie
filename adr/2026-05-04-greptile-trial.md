---
status: accepted
date: 2026-05-04
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Greptile AI code review: 4-week trial

## Context and Problem Statement

Reverie has one human reviewer (the maintainer). Reviewer attention
is the bottleneck — every PR sits until the maintainer can look at
it. The strict-lint policy ratified in
[`adr/2026-05-03-strict-lint-policy.md`](2026-05-03-strict-lint-policy.md)
machine-enforces style and most CLAUDE.md hard rules, but the
reviewer remains the only check on logic-level concerns: subtle data
flow bugs, missed edge cases, security gaps not caught by lint, and
deviations from project conventions that lint can't express.

Greptile is a third-party AI code reviewer (GitHub App) that
auto-comments on PRs with a graph-based view of the codebase. Two
questions to answer in this trial:

1. **Signal**: does Greptile catch enough real issues (logic bugs,
   security gaps, convention violations) to justify the noise?
2. **Workflow fit**: can a single-maintainer project absorb auto
   comments without them becoming review fatigue?

The strict-lint policy intentionally landed first to compress the
style territory Greptile would otherwise claim, isolating the trial
to logic-level signal where AI review is most likely to add value.

## Decision

Run a 4-week trial of Greptile on `unkos-dev/reverie`.

### Trial configuration

Maximally verbose for trial calibration. Better to start with full
visibility into what Greptile catches and tighten later than to
start tight and miss patterns.

`greptile.json` at repo root:

- `strictness: 1` — counter-intuitively, this is the _least_
  filtered setting in Greptile's schema (1 = Low strictness =
  Verbose; 3 = High strictness = Critical-only). Comments on
  everything Greptile flags. Trial purpose is signal calibration
  — needs the full output to evaluate
- `commentTypes: ["logic", "syntax", "style", "info"]` — every
  category enabled. Style overlap with the lint policy is expected
  signal data: any style nit Greptile raises that lint already
  enforces is a data point on Greptile's lint awareness; any nit
  lint doesn't catch is a candidate for a new lint rule
- `triggerOnUpdates: true` — re-review on every push, not just first
- `customContext.files`: pinned references to `CLAUDE.md`,
  `backend/CLAUDE.md`, `frontend/CLAUDE.md`,
  `adr/2026-05-03-strict-lint-policy.md`, and the 13
  `.claude/security/codeguard-*.md` checklists. These are the
  load-bearing convention sources — Greptile should read them
  before flagging style/security
- `customContext.rules`: project-specific rules with file scopes —
  `time` not `chrono` (backend), no raw hex literals (frontend), no
  `enum` (frontend), no inline JSX style (frontend), shadcn/ui
  carve-out, secret-handling stance, TDD requirement, Conventional
  Commits requirement
- No `ignorePatterns` initially — see Alternatives. Amended
  2026-05-04 to exclude lockfiles only; see Amendments below

### App install

Done by the maintainer via GitHub App marketplace
(<https://github.com/apps/greptileai>) on the `unkos-dev/reverie`
repo only (not org-wide).

### Trigger model

Auto on every PR. No label gate. Branch filtering (skip
`release-please--*` and `dependabot/*`) considered but not adopted
in this trial — those PRs are typically small and Greptile silence
on them is itself a useful data point.

### Trial gate

**Duration**: 4 weeks from install date.

**Success metric**: ≥30% of Greptile findings actionable (defined
as: surfaces a real issue and gets addressed in a follow-up commit
on the same PR, or filed as a tracked issue). Findings dismissed
without action count against. Maintainer logs verdict per PR in a
running tally during the trial.

**At the gate**, decide:

- **Adopt**: keep current config, possibly tighten `commentTypes`
  to drop `style` and `info` if they're the noise drivers
- **Reconfigure and re-trial**: change `strictness`, `commentTypes`,
  or add `ignorePatterns`; another 4 weeks
- **Reject**: uninstall App, delete `greptile.json`. No repo
  changes (lint policy stands on its own)

Decision recorded as a follow-up ADR (`accepted` / `superseded`).

## Decision Outcome (2026-05-29)

**Adopt both.** At the parallel-trial gate the verdict — recorded in
[`2026-05-07-coderabbit-parallel-trial.md`](2026-05-07-coderabbit-parallel-trial.md)
and tallies UNK-155 / UNK-187 — is to keep both reviewers in steady
state: Greptile label-gated (the PR #171 model — zero auto-quota,
opt-in per PR via the `greptile-review` label), CodeRabbit auto on
every non-draft PR.

Greptile cleared the gate decisively: n=19 findings across 12 PRs,
84% actionable. Three failure modes observed and dispositioned —
lockfile rename hallucination (mitigated via the lockfile
`ignorePatterns` amendment below); ADR-status convention misread
(tracked, not recurred); stale-snapshot timing skew (process-side,
not a Greptile defect). Its differentiator surface — governance /
cross-document / cross-PR / public-API-surface consistency, which
lint cannot express and which CodeRabbit's concrete-code-level
surface does not cover — earns it a permanent (label-gated) seat
alongside CodeRabbit. Cross-tool overlap with CodeRabbit was zero
on PRs #180 and #188; the first observed overlap (PR #194,
`# Errors` self-consistency) was a single finding among 14, so the
complement holds.

Status flips `proposed` → `accepted` per the in-place mechanism
defined in the CodeRabbit parallel-trial ADR's gate section — no
superseding ADR; both trial ADRs are the durable record.

**Watch** (re-read when next touching AI-review config; no separate
tracking ticket by design):

- Convention-misread recurrence (UNK-155 row #9 class). If it
  recurs, encode the ADR-status convention in `greptile.json`
  `customRules` rather than relying on per-PR inference
- Lockfile `ignorePatterns` sufficiency — confirm the carve-out
  still suppresses the rename-hallucination class as the lockfile
  format evolves
- Cross-tool overlap-rate creep with CodeRabbit on docstring-heavy
  PRs. If overlap dominates, the "adopt both" justification needs
  re-examination
- Per-author review-cap re-appearance on the Greptile dashboard
  after the 2026-05-08 cap-lift (see Amendments). Recurrence ⇒ the
  lift was scoped, escalate to support

## Consequences

- Good — second reviewer on every PR. Single-maintainer projects
  rely on lint + manual review; an AI reviewer adds a third pass
  that catches what either misses
- Good — graph-based context (the Greptile differentiator) means
  comments can reference cross-file patterns, not just the diff
  in isolation. Stronger than per-file static analysis
- Good — pinned `customContext.files` align Greptile with project
  conventions from day one. Reduces the cold-start noise common in
  AI reviewers that don't read the codebase's documented rules
- Good — `strictness: 1` + all `commentTypes` produces maximum
  signal data for the trial verdict. Easier to dial up (raise the
  filter to 2 or 3) at the gate with evidence than to dial down
  from a quiet starting point and miss the patterns Greptile only
  flags at the verbose tier
- Bad — auto-review on every PR means review noise during the
  trial. Maintainer must triage every comment, even false
  positives, to populate the actionable-rate metric
- Bad — third-party data exposure. Greptile reads PR diffs and
  full repo context. Reverie is open-source so the surface is
  public, but the org-internal `.claude/`, `adr/`, and
  `docs/superpowers/specs/` paths are also read. Acceptable for
  this repo; not transferable to private repos without re-review
- Bad — third-party SaaS dependency. If Greptile changes pricing,
  policy, or API, the workflow regresses. Mitigated by trial
  framing — no automation depends on Greptile's existence
- Neutral — `customContext` may inflate per-PR token spend on
  Greptile's side. Visible in their billing dashboard (if relevant
  for the trial tier) but not on our infra
- Neutral — Greptile's findings are advisory. Maintainer remains
  the sole approver for merge

## Alternatives Considered

- **No AI reviewer (status quo).** Rejected — the open question
  ("does AI review add value here?") doesn't get answered without
  a trial. Lint + manual review is the baseline; the trial measures
  the marginal value of an AI layer on top
- **CodeRabbit / Diamond / Codium / other AI reviewers.** Deferred
  — Greptile's graph-based codebase context is the most
  differentiated angle, and 4 weeks of one tool generates cleaner
  signal than 1 week of four. If the Greptile trial fails, the next
  ADR can pick up an alternative with a clean comparison baseline
- **Self-hosted AI reviewer (Claude Code in CI, custom bot).**
  Rejected for now — trial framing values "does AI review work for
  this repo at all" over "which implementation is best." A
  managed service with one config file is the lowest-effort
  hypothesis test
- **`strictness: 2` (balanced default) or `strictness: 3`
  (critical-only) for trial.** Rejected — both pre-filter the
  output before the trial has measured what the unfiltered output
  looks like. `strictness: 1` exposes the full noise floor; the
  trial verdict can then justify raising the filter (to 2 or 3) at
  the gate with evidence. Starting filtered and trying to
  reconstruct what was hidden is harder than starting verbose and
  pruning
- **`commentTypes: ["logic"]` only.** Rejected — pre-filters out
  the data needed to evaluate Greptile's full output. Trial first,
  filter later
- **Add `ignorePatterns` for `package-lock.json`, `Cargo.lock`,
  `dist/`, `graphify-out/`, generated SQL.** Considered. Skipped
  in initial config because no per-PR cost data exists yet to
  justify the speedup. Add at the trial gate if review latency or
  cost is the binding constraint. **Amended 2026-05-04**: the
  lockfile subset (`package-lock.json`, `Cargo.lock`) was added
  early in response to a different binding constraint than the one
  this bullet anticipated — see Amendments below. The remaining
  patterns (`dist/`, `graphify-out/`, generated SQL) stay deferred
- **Branch filter to skip `release-please--*` and `dependabot/*`.**
  Rejected for the trial — silence on these branches is itself
  useful signal. If Greptile adds noise on dependency bumps, that's
  a config issue worth catching during the trial, not pre-empting
- **Opt-in via `greptile-review` label instead of auto-on-every-PR.**
  Rejected — opt-in produces selection bias (only complex PRs get
  reviewed), undermining the actionable-rate metric. Auto-on-all
  exposes Greptile to the full PR mix and gives the trial verdict
  honest data
- **Org-wide install instead of per-repo.** Rejected — Reverie is
  the only repo in scope. Per-repo install matches the trial
  scope; org-wide install pre-commits to other repos before the
  verdict is in

## Amendments

### 2026-05-08 — Label gate dropped after cap-lift

Greptile support (Muzz Khan) confirmed via email 2026-05-08 06:41
AWST that the per-author 50-review cap has been lifted on the
`junkovich` account. The dashboard "11 days left in your free
trial!" countdown remains visible but is decorative — the OSS
discount applied 2026-05-04 is the live entitlement; the trial
countdown UI is residue, not state.

The 2026-05-07 amendment adopted `labels: ["greptile-review"]` as
the fix-now lever for the cap. With the cap lifted, the label
gate becomes friction without a quota justification — every PR
now needs a manual click before Greptile sees it, including the
logic-heavy PRs that were the trial's actual signal target. The
gate is dropped.

`triggerOnUpdates: false` stays. The 2026-05-07 quota math
(conservative variant: -62% vs. original auto-on-every-push)
identified push-iteration as the dominant burn driver, not
PR-open. Even on a lifted cap, re-review on every push still
inflates the per-PR review count without per-push signal gain
worth the inflation — the maintainer's iteration commits
typically address known feedback rather than introduce new
diff-shape that benefits from a fresh AI pass. The `@greptileai`
manual-mention flow described in the 2026-05-07 amendment remains
available for the rare case a confidence-score update is wanted
mid-PR.

#### `greptile.json` change

Removed:

```json
"labels": ["greptile-review"]
```

Unchanged:

```json
"triggerOnUpdates": false
```

#### Quota math under the dropped-label model

Replayed against the same 10 reverie PRs from the 2026-05-07
table:

| Variant                                            | Reviews | vs original (26) | vs label-gate (10) |
| -------------------------------------------------- | ------- | ---------------- | ------------------ |
| Auto-on-PR-open, no push re-review (now)           | 10      | -62%             | 0                  |
| Auto-on-PR-open + `@greptileai` re-review (per PR) | 20      | -23%             | +100%              |
| Original auto-on-every-push (rejected)             | 26      | 0                | +160%              |

Dropping the label gate alone does not change the conservative
per-PR review count vs. the label-gate model — both are 10
reviews / 10 PRs. What changes is _which_ PRs Greptile sees: the
label-gate model excluded the doc-only / chore class entirely
(8/10 in the skip-doc-only variant); the new model includes them.
On a lifted cap, including the doc-only class is the right call
— Greptile silence on a doc PR is itself trial-relevant signal
(does the AI reviewer correctly stay quiet on no-logic diffs?),
which the label gate suppressed.

#### What stays unchanged

- Strictness, commentTypes, customContext, ignorePatterns, all
  customRules — unchanged
- Author exclusion for `renovate(bot)` and `dependabot[bot]` —
  unchanged (separate per-bot quotas; never depleted maintainer
  quota)
- Trial gate metric (≥30% actionable) and gate decision matrix —
  unchanged. The CodeRabbit parallel trial (2026-05-21 gate)
  is also unchanged
- `triggerOnUpdates: false` — retained as a noise control, not a
  quota control

#### What to watch

- Per-author review counter on the Greptile dashboard. If a
  cap-style notice reappears post-lift, the cap-lift was
  scoped (e.g. one-time reset) rather than a permanent OSS
  entitlement — escalate back to support
- Actionable-rate metric. Including doc-only PRs in the trial
  pool may dilute the rate if Greptile flags non-issues on them.
  Tally each doc-PR review explicitly against the gate metric

### 2026-05-07 — Label-gated trigger + manual confidence-update flow (PR #171)

The original Trigger Model section in this ADR specified "Auto on
every PR. No label gate." That decision burned through the trial
account's 50-review cap inside the first 4 days. Empirical
quota-burn rate at the cap, computed from the per-PR review counts
shown in the Greptile dashboard:

| PR class                                                   | PRs    | Reviews | Avg/PR  |
| ---------------------------------------------------------- | ------ | ------- | ------- |
| `feat/unk-167-sqlx-macros-*` series (push-iteration heavy) | 5      | 17      | 3.4     |
| Other reverie PRs (#168, #170, #169, #166, #165)           | 5      | 9       | 1.8     |
| **All listed reverie PRs**                                 | **10** | **26**  | **2.6** |

50-review cap ÷ 2.6 avg ≈ ~19 PRs runway. Burned through in 9 PRs
because the UNK-167 series clustered three to five reviews per PR.
Renovate and Dependabot are already excluded via per-author Greptile
config (separate per-bot quota; bot-PR reviews do not deplete the
maintainer's `junkovich` author quota — confirmed empirically when
the cap-hit notice referenced `junkovich`-author reviews only).

The dominant burn driver is `triggerOnUpdates: true`: every push to
an open PR triggers a fresh review. For PRs that go through several
fix-iteration rounds, the per-push burn dwarfs the per-PR-open
burn. A secondary driver is auto-review on doc-only / chore /
config PRs that never carried logic-level signal worth Greptile's
attention.

This amendment switches the trigger model to **label-gated** — the
fix-now lever. A possible later layer (path-based auto-label via
GitHub Actions, or `/greptile` slash-command label add) is not
adopted here; the manual-label flow is simpler and aligns with the
maintainer's intent to internal-review first, then ask Greptile.

#### `greptile.json` changes

```json
{
  "triggerOnUpdates": false,
  "labels": ["greptile-review"]
}
```

- `triggerOnUpdates: false` — pushes to an open PR do not trigger a
  fresh review. Eliminates the per-push-burst burn.
- `labels: ["greptile-review"]` — Greptile only reviews PRs that
  carry the `greptile-review` label. Doc-only / chore / config PRs
  that never get the label never burn a review.

#### Workflow

1. **Open PR** — no label, Greptile silent.
2. **Internal review** — adversarial-review skill or
   `prp-core:prp-review-agents` runs against the diff. Findings
   addressed in follow-up commits on the same branch. Greptile is
   still silent because the label is not yet applied.
3. **Internal review passes** — maintainer manually applies the
   `greptile-review` label. Greptile reviews once, against the
   post-internal-review state.
4. **Greptile findings addressed** — fix commits pushed. **Greptile
   does not auto-update its review** because `triggerOnUpdates` is
   false. The original Greptile review remains visible on the PR
   but its confidence score reflects the pre-fix state.
5. **(Optional) request a confidence-score update** — comment
   `@greptileai` on the PR. This burns a second review-credit but
   produces an updated confidence score that reflects the post-fix
   state. Use sparingly — high-confidence Greptile reviews where the
   fixes are clearly responsive to the comments do not need a
   re-review pass.

#### Quota math under the new model

Same 10 reverie PRs replayed under the label-gate model, conservative
(no confidence-score update) and generous (confidence-score update
on every PR) variants:

| Variant                                                        | Reviews | vs old (26) |
| -------------------------------------------------------------- | ------- | ----------- |
| Conservative (label once, no `@greptileai` re-review)          | 10      | -62%        |
| Generous (label once + `@greptileai` re-review on every PR)    | 20      | -23%        |
| Skip-doc-only (label not applied to PR #169 doc archive, etc.) | 8       | -69%        |

The label-not-applied case dominates the savings on PR classes that
do not carry logic-level signal — exactly the class where the prior
"auto on every PR" trigger model spent quota for negligible return.

#### Trade-offs accepted

- **No automatic confidence-score update on fix commits.** The
  Greptile inline-comment "edit" behaviour described in the
  original Trigger Model section still happens, but only after
  `@greptileai` mention; without the mention, the original review
  remains visible at the pre-fix confidence. If the maintainer
  wants a closing "addressed/not-addressed" Greptile assessment on
  every PR, that is one mention per PR.
- **Manual labelling is one extra click per PR** that does need
  Greptile review. Acceptable cost for ~62% quota reduction on
  conservative use and full skip on doc-only PRs.
- **Drafts already skipped** by Greptile default. Combining with
  the label gate keeps the gate consistent: Greptile reviews when
  _both_ the PR is non-draft _and_ the label is present.

<!-- markdownlint-disable MD024 -- pre-existing duplicate amendment heading, unrelated to this status flip -->

#### What stays unchanged

<!-- markdownlint-enable MD024 -->

- Strictness, commentTypes, customContext, ignorePatterns, all
  customRules — unchanged.
- Author exclusion for `renovate(bot)` and `dependabot[bot]` —
  unchanged. Their per-bot review counters are separate from
  the maintainer's author quota.
- Trial gate metric (≥30% actionable) and gate decision matrix —
  unchanged. The label gate adjusts the volume of Greptile
  invocations, not the per-finding signal-to-noise ratio.

Tally tracking these decisions: UNK-155 (early-success consideration
section).

### 2026-05-07 — CodeRabbit parallel trial added (PR #172)

A separate AI reviewer (CodeRabbit) is being trialled alongside
Greptile for 2 weeks (2026-05-07 → 2026-05-21). See
[`adr/2026-05-07-coderabbit-parallel-trial.md`](2026-05-07-coderabbit-parallel-trial.md)
for the parallel-trial framing, success metrics, and gate
decision matrix.

This does not change the Greptile trial framing, gate metric, or
the Trial Configuration documented above. Greptile remains
label-gated per the amendment above (drawing zero auto-quota
until a PR is explicitly labelled `greptile-review`) throughout
the parallel trial. The maintainer can opt in per-PR if the
graph-based context catches Greptile is differentiated on are
relevant for that PR's diff.

The two ADRs (Greptile + CodeRabbit) close together at their
respective gates. At the parallel-trial gate (2026-05-21), four
outcomes are possible:

- Adopt CodeRabbit, retire Greptile — this ADR superseded
- Adopt both — both ADRs flip to accepted, two reviewers in
  steady state
- Retire CodeRabbit, retain Greptile — CodeRabbit ADR rejected,
  this ADR continues to its original 2026-06-01 gate
- Retire both — both ADRs rejected

`CONTRIBUTING.md` § "Third-party AI code review" was rewritten
in the same PR that landed this parallel trial: product-agnostic
framing, both Greptile and CodeRabbit named in passing with their
security disclosures, AI-training opt-in/out clauses split
per-reviewer.

### 2026-05-04 — `ignorePatterns` added for lockfiles (PR #148)

The initial config in this ADR deferred `ignorePatterns` to the
trial gate, with the explicit trigger: _"Add at the trial gate if
review latency or cost is the binding constraint."_ That trigger
described a future quantitative ceiling — too slow or too expensive
once data accumulated.

A different binding constraint emerged ~5 hours into the trial,
not anticipated by the original ADR: a **confirmed hallucination
pattern** specific to lockfiles. Two consecutive Renovate npm-bump
PRs (#71 `@commitlint/config-conventional` and #74
`markdownlint-cli2`) produced identical false-positive findings —
Greptile narrated the existing `name: reverie-dev` string in
`package-lock.json` context as a brand-new `tome-dev → reverie-dev`
"silent rename" introduced by the PR. Verified against actual
diffs: zero `name` field changes in either PR. The pattern is
consistent (twice in two consecutive lockfile PRs), specific
(lockfile only), and predictable (will repeat on every Renovate
npm bump touching `package-lock.json` until mitigated).

PR #148 adds `**/package-lock.json` and `**/Cargo.lock` to
`ignorePatterns` to suppress this false-positive class. Lockfile
changes are mechanical regenerations of dep trees plus integrity
hashes; line-by-line review has zero security signal beyond what
npm/cargo already verify cryptographically. The mitigation also
saves Greptile review-credit budget on every Renovate PR (50/account
trial cap, already partially burned by the eslint v10 retry storm
documented separately in PR #147).

The remaining deferred patterns (`dist/`, `graphify-out/`,
generated SQL) stay deferred per the original ADR — they have not
exhibited the false-positive class that justified the lockfile
amendment.

Trial tally tracking these findings: UNK-155 (rows #4 and #5).

## More Information

- MADR 4.0: <https://adr.github.io/madr/>
- Greptile docs: <https://docs.greptile.com>
- `greptile.json` schema: <https://docs.greptile.com/code-review/configuration>
- Related: [`adr/2026-05-03-strict-lint-policy.md`](2026-05-03-strict-lint-policy.md) — strict lint
  landed first to compress style territory before this trial
- Related: `CLAUDE.md` "Hard Rules" §5 (TDD), §6 (security
  scrutiny), §7 (secret handling) — Greptile is configured to
  read these via `customContext.files`
