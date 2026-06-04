---
status: lifted
severity: low
surfaces: [developer]
adopted: 2026-05-21
adopted-because: axum-login 0.18.0 (latest crates.io release) peer-pins tower-sessions 0.14; upgrade to 0.15 requires either axum-login release containing the bump or a git dep override
lift-when-class: internal-refactor
lift-when: first-party session layer lands (adr/2026-06-04-first-party-session-layer.md) — removes axum-login + tower-sessions-sqlx-store, unblocking tower-sessions 0.15 (UNK-101)
lifted: 2026-06-04
superseded-by: PR #424 (https://github.com/unkos-dev/reverie/pull/424)
---

# tower-sessions pinned to 0.14 (axum-login + sqlx-store peer pins)

> **Lifted 2026-06-04 (UNK-101).** The first-party session layer
> ([`adr/2026-06-04-first-party-session-layer.md`](../adr/2026-06-04-first-party-session-layer.md))
> removed both `axum-login` and `tower-sessions-sqlx-store` from
> `backend/Cargo.toml` and bumped `tower-sessions` to 0.15 directly (the bump
> was applied in that PR rather than via Renovate PR #128, which #128 supersedes).
> Entry retained for audit.

## Constraint

The `tower-sessions` 0.14 → 0.15 bump (Renovate PR
[#128](https://github.com/unkos-dev/reverie/pull/128)) is
**double-blocked** by two independent peer-pins, both on abandoned
single-maintainer wrappers:

1. **`axum-login` 0.18.0** (latest crates.io, published 2025-07-20)
   peer-pins `tower-sessions = "0.14"`. The bump merged upstream
   2026-05-07
   ([axum-login#315](https://github.com/maxcountryman/axum-login/pull/315),
   commit `151c72d`; dependabot duplicate
   [#320](https://github.com/maxcountryman/axum-login/pull/320) closed
   as superseded) but has **never been released** — no crates.io
   publish since 0.18.0.
2. **`tower-sessions-sqlx-store` 0.15.0** (latest crates.io, published
   2025-01-01) has a normal dependency on `tower-sessions-core ^0.14`.

Removing either alone is insufficient; the other still holds 0.14.
This entry originally recorded only the axum-login pin — the
sqlx-store pin is the second wall (it also blocks sqlx 0.9, see sister
entry [`2026-06-02-sqlx-0-9-blocked.md`](2026-06-02-sqlx-0-9-blocked.md)).

**Resolution (decided 2026-06-04).** Rather than wait on two
unresponsive upstreams, reverie drops both wrappers and reimplements
the thin slice it uses as first-party code on the maintained
`tower-sessions` core — ADR
[`2026-06-04-first-party-session-layer.md`](../adr/2026-06-04-first-party-session-layer.md).
That removes both pins and unblocks the 0.15 bump directly.

## Workaround

`backend/Cargo.toml:51` keeps `tower-sessions = "0.14"` pinned, with
inline note at L52-58 explaining the `tower-sessions-sqlx-store` 0.15
↔ tower-sessions 0.14 pairing (ADR
[`2026-05-08-tower-sessions-sqlx-store.md`](../adr/superseded/2026-05-08-tower-sessions-sqlx-store.md)).
The inline note now references the load-bearing upstream PR,
axum-login#315 (refreshed in the same change that adopted this
debt entry).

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

Lift trigger: the first-party session layer (ADR
[`2026-06-04-first-party-session-layer.md`](../adr/2026-06-04-first-party-session-layer.md))
lands, removing both `axum-login` and `tower-sessions-sqlx-store`.

When that work lands:

1. Confirm no `axum-login` / `tower-sessions-sqlx-store` entries
   remain in `backend/Cargo.toml`.
2. Land Renovate PR #128 (`tower-sessions = "0.15"`).
3. Remove the now-obsolete sqlx-store pairing note at
   `Cargo.toml:52-58`.
4. Flip this entry to `status: lifted`, set `lifted`,
   `superseded-by`.
5. Update README "Active" → "Lifted" listing.

Do **not** lift via a git-dep override on `axum-login` — that path is
rejected by the ADR (unauditable HEAD; cargo-audit/Renovate blind to
git deps) and would still leave the sqlx-store pin in place.

## Related

- [UNK-101](https://linear.app/unkos/issue/UNK-101) — tower-sessions
  version bump tracking ticket (the lift work)
- [PR #128](https://github.com/unkos-dev/reverie/pull/128) — Renovate
  bump PR held open until lift
- [axum-login#315](https://github.com/maxcountryman/axum-login/pull/315)
  — upstream fix merged but unreleased
- [axum-login#320](https://github.com/maxcountryman/axum-login/pull/320)
  — dependabot duplicate, closed
- [`adr/2026-06-04-first-party-session-layer.md`](../adr/2026-06-04-first-party-session-layer.md)
  — the decision that lifts this entry (drops both wrappers)
- [`2026-06-02-sqlx-0-9-blocked.md`](2026-06-02-sqlx-0-9-blocked.md)
  — sister entry; sqlx 0.9 is blocked behind the same sqlx-store wall
- [`adr/superseded/2026-05-08-tower-sessions-sqlx-store.md`](../adr/superseded/2026-05-08-tower-sessions-sqlx-store.md)
  — pairing rationale (superseded by the first-party-layer ADR)
- `backend/Cargo.toml:26,51-59` — pin sites
