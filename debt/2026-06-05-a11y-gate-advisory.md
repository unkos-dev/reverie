---
severity: low
surfaces: [developer]
adopted: 2026-06-05
adopted-because: the a11y gate (UNK-268) ships against a showcase baseline that already contains one known WCAG violation — the default Badge variant's cream-on-gold contrast (3.44:1, UNK-345) — which is deliberately NOT allowlisted; a blocking gate would red-X its own PR and every subsequent frontend PR until the badge is fixed
lift-when-class: tracked-issue
lift-when: UNK-345 ships (default Badge contrast fixed) and `npm run a11y` passes against the showcase, at which point `continue-on-error: true` is removed from the `a11y` job's gate step so the gate blocks
---

# a11y gate is advisory (continue-on-error) until the showcase baseline is clean

The accessibility gate added in UNK-268 (`.github/workflows/ci.yml`, `a11y`
job) is wired with `continue-on-error: true` on its "Run accessibility gate"
step, so a failing scan does not red the job or the `ci-gate` aggregator. The
gate is therefore **advisory**, not blocking — the violation signal is visible
in the job log and the uploaded `a11y-results.json` artifact, but it does not
block merge.

## Constraint

The gate's first (and currently only) scan target, the dev-only design showcase
`/design/system`, contains one known WCAG 2.2 AA violation that is intentionally
not allowlisted: the default `Badge` variant renders small cream text `#e8dcc2`
on Reverie Gold `#8e6f38` at 3.44:1 (below the 4.5:1 normal-text floor). Per
`frontend/DESIGN.md` §2 a badge is not a permitted gold surface, so this is a
real bug, tracked in UNK-345 — the gate correctly catches it.

Shipping the gate as blocking with that baseline failure present would make the
`a11y` job red, fail `ci-gate` (a required check), and leave UNK-268's own PR —
and every later frontend-touching PR — unmergeable until UNK-345 lands.

## Workaround

`continue-on-error: true` on the `a11y` gate step. This mirrors the established
`impeccable detect` advisory step in the same workflow
(`.github/workflows/ci.yml`), which is likewise advisory until its known
findings are addressed.

## Why this isn't the right shape

- The whole point of the gate (UNK-268 acceptance criterion) is to **fail CI on
  any new WCAG 2.2 AA violation outside the allowlist**. While advisory, a
  genuine new regression would surface only in logs/artifact, not block the
  merge — the gate is not yet load-bearing.
- This is a deliberate, time-boxed rollout concession, not the end state.

Severity is `low`: the gate runs and reports on every frontend PR, and the one
known failure is already tracked (UNK-345); the only thing missing is the
enforcement teeth, which flip on cleanly.

## Resolve

The PR that fixes UNK-345 (or a follow-up once it merges):

1. Confirms `npm run a11y` passes against the showcase (badge fixed; no
   non-allowlisted violations remain).
2. Removes `continue-on-error: true` from the `a11y` job's "Run accessibility
   gate" step in `.github/workflows/ci.yml` so the gate blocks.
3. **Purges this entry** — deletes this file, removes its README "Active" line,
   and names the resolving PR in the purge commit message.

## Related

- [UNK-268](https://linear.app/unkos/issue/UNK-268) — the a11y gate this defers enforcement for
- [UNK-345](https://linear.app/unkos/issue/UNK-345) — the default Badge contrast fix (the lift condition)
- `adr/2026-06-05-accessibility-review-process.md` — the gate's decision record
- `.github/workflows/ci.yml` — the `a11y` job + the `continue-on-error` flip site
- `frontend/scripts/a11y/allowlist.mjs` — the documented allowlist (badge intentionally excluded)
