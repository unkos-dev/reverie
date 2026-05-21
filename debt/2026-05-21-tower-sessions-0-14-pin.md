---
status: active
severity: low
surfaces: [developer]
adopted: 2026-05-21
adopted-because: axum-login 0.18.0 (latest crates.io release) peer-pins tower-sessions 0.14; upgrade to 0.15 requires either axum-login release containing the bump or a git dep override
lift-when-class: dep-unblocks
lift-when: axum-login crates.io release > 0.18.0 containing maxcountryman/axum-login#315 (merged 2026-05-07, commit 151c72d, "Upgrade tower-sessions to 0.15.0")
lifted: ~
superseded-by: ~
---

# tower-sessions pinned to 0.14 (axum-login peer pin)

## Constraint

`axum-login` 0.18.0 — the latest crates.io release (published
2025-07-20) — peer-pins `tower-sessions = "0.14"`. Bumping reverie to
`tower-sessions = "0.15.0"` (Renovate PR
[#128](https://github.com/unkos-dev/reverie/pull/128)) fails to
resolve unless `axum-login` is also bumped to a release that accepts
`tower-sessions 0.15`.

Upstream merged the bump on 2026-05-07
([axum-login#315](https://github.com/maxcountryman/axum-login/pull/315),
commit `151c72d` "Upgrade tower-sessions to 0.15.0"). Dependabot's
duplicate-bump PR
[axum-login#320](https://github.com/maxcountryman/axum-login/pull/320)
closed shortly after as superseded.

**The fix is in `main` but unreleased.** No new crates.io publish
since v0.18.0 (~10 months). PR #128 stays blocked until upstream
cuts v0.19.0 (or higher).

## Workaround

`backend/Cargo.toml:51` keeps `tower-sessions = "0.14"` pinned, with
inline note at L52-58 explaining the `tower-sessions-sqlx-store` 0.15
↔ tower-sessions 0.14 pairing (ADR
[`2026-05-08-tower-sessions-sqlx-store.md`](../adr/2026-05-08-tower-sessions-sqlx-store.md)).
The inline note references axum-login#320 but the load-bearing PR
is #315; refresh on next touch.

## Why this isn't the right shape

- tower-sessions 0.15 carries a memory-ordering race fix
  ([tower-sessions#254](https://github.com/maxcountryman/tower-sessions/pull/254))
  and `rand` 0.9 update — both worth picking up.
- Holding 0.14 keeps reverie on a release that the upstream maintainer
  has already moved past on the 0.15 line.
- Renovate keeps re-surfacing PR #128 every sweep; each cycle costs
  human attention deciding "still blocked, still wait".

Severity is `low` because tower-sessions 0.14 is functionally correct
for reverie's session usage (single-process, no high-contention
ID-cycling path). The fix in 0.15 is not load-bearing for current
threat model.

## Lift conditions

Lift trigger: axum-login crates.io release > 0.18.0 that includes
commit `151c72d`.

When that release ships:

1. Bump `axum-login` in `backend/Cargo.toml` to the new release.
2. Land Renovate PR #128 (`tower-sessions = "0.15"`).
3. Refresh the inline note at `Cargo.toml:52-58` to reflect the new
   pairing.
4. Flip this entry to `status: lifted`, set `lifted`,
   `superseded-by`.
5. Update README "Active" → "Lifted" listing.

Do **not** lift via a git-dep override on `axum-login`. ADR rationale
for staying on crates.io releases applies (auditable supply chain,
no unreviewed-HEAD risk).

## Related

- [UNK-101](https://linear.app/unkos/issue/UNK-101) — tower-sessions
  version bump tracking ticket (the lift work)
- [PR #128](https://github.com/unkos-dev/reverie/pull/128) — Renovate
  bump PR held open until lift
- [axum-login#315](https://github.com/maxcountryman/axum-login/pull/315)
  — upstream fix merged but unreleased
- [axum-login#320](https://github.com/maxcountryman/axum-login/pull/320)
  — dependabot duplicate, closed
- [`adr/2026-05-08-tower-sessions-sqlx-store.md`](../adr/2026-05-08-tower-sessions-sqlx-store.md)
  — pairing rationale
- `backend/Cargo.toml:26,51-59` — pin sites
