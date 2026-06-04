---
severity: low
surfaces: [developer]
adopted: 2026-06-02
adopted-because: sqlx 0.9 bump (Renovate #326 crate, #325 sqlx-cli) cannot compile — tower-sessions-sqlx-store 0.15.0 (latest) pinned sqlx ^0.8, so our 0.9 PgPool mismatched the store's 0.8 PgPool
lift-when-class: internal-refactor
lift-when: the mechanical QueryBuilder migration (drop the `'q` lifetime, ~20 sites) lands sqlx 0.9 via Renovate #326/#325 + `.sqlx` cache regen (UNK-101)
---

# sqlx pinned to 0.8 (mechanical sqlx 0.9 migration pending)

The upstream wall is **gone**. This entry now tracks only the mechanical
`QueryBuilder` migration that remains between reverie and sqlx 0.9. It is
active, not resolved: the code still pins `sqlx = "0.8.6"` and the proper
fix has not shipped.

## Constraint

`backend/Cargo.toml:47` pins `sqlx = "0.8.6"`. Renovate
[#326](https://github.com/unkos-dev/reverie/pull/326) (`sqlx → 0.9.0`)
and its paired [#325](https://github.com/unkos-dev/reverie/pull/325)
(`sqlx-cli → 0.9.0`, the `.sqlx` offline-cache toolchain) do not yet
compile against our code.

The break originally had **two parts**. Only one remains:

1. **Our code — mechanical, ~20 sites (REMAINING).** sqlx 0.9 removed
   the `'q` lifetime from `QueryBuilder`, so every
   `QueryBuilder<'_, Postgres>` (and `Separated<…>`) annotation is an
   `E0107` ("struct takes 0 lifetime arguments but 1 supplied"). 18
   sites across `routes/library/mod.rs`, `routes/opds/library.rs`,
   `routes/opds/scope.rs`, `db.rs`. Plus one `E0599` (query strings are
   now `SqlStr`, a `.as_bytes()` call needs `.as_str()` first) and one
   `E0521` borrow-escape fallout from a builder helper signature. All
   mechanical; ~half a day including regenerating the `.sqlx` cache with
   cli 0.9.

2. **Upstream store wall — LIFTED 2026-06-04 (PR #424).**
   `tower-sessions-sqlx-store` 0.15.0 pinned sqlx `^0.8.0`, so with sqlx
   0.9 in the tree the store built against 0.8 and our calls into it
   (`PostgresStore::new(pool)` and friends) were `E0308` mismatched
   types. The first-party session layer (ADR
   [`2026-06-04-first-party-session-layer.md`](../adr/2026-06-04-first-party-session-layer.md))
   dropped `tower-sessions-sqlx-store` entirely, removing the `sqlx ^0.8`
   pin. This was the same dependency wall that blocked the tower-sessions
   0.14 → 0.15 bump (Renovate PR #128); #424 lifted both.

With part 2 gone, the migration is now **landable** — it was previously
held back because a half-done branch would rot against every `main`
merge that touched a query while the store wall made it un-completable.

## Workaround

Keep `sqlx = "0.8.6"` (`backend/Cargo.toml:47`) and
`SQLX_CLI_VERSION: "0.8.6"` (`.github/workflows/ci.yml`). Renovate PRs
`#326` and `#325` stay open until the mechanical migration lands.

## Why this isn't the right shape

- sqlx 0.9 is a genuine version behind; holding it indefinitely accrues
  the usual upgrade-debt interest (eventual larger jump, missed fixes).
- The remaining work is genuinely mechanical now that the store wall is
  gone — there is no longer an `E0308` trap waiting to waste the
  round-trip. This entry records that it is **landable, not yet
  landed**, so it isn't mistaken for already-done.

Severity is `low`: sqlx 0.8.6 is fully functional for reverie; nothing
in 0.9 is load-bearing for the current threat model.

## Resolve

The PR that lands sqlx 0.9:

1. Lands Renovate PR `#326` (`sqlx → 0.9`): drop the `'q` lifetime from
   every `QueryBuilder<'_, Postgres>` / `Separated<…>`; fix the `SqlStr`
   `.as_bytes()` site; resolve the one borrow-escape.
2. Lands Renovate PR `#325` (`sqlx-cli → 0.9`) in the **same** PR and
   regenerates the `.sqlx` offline cache so cli and crate versions stay
   locked (cli/crate skew corrupts the cache format).
3. Runs the `cargo sqlx prepare --check` round-trip and `cargo test`.
4. **Purges this entry** — deletes this file, removes its README "Active"
   line, and names the resolving PR in the purge commit message.

## Related

- [PR #326](https://github.com/unkos-dev/reverie/pull/326) — Renovate
  `sqlx → 0.9.0`, held open until the migration lands
- [PR #325](https://github.com/unkos-dev/reverie/pull/325) — Renovate
  `sqlx-cli → 0.9.0`, lands paired with #326
- [PR #424](https://github.com/unkos-dev/reverie/pull/424) — first-party
  session layer; dropped `tower-sessions-sqlx-store`, lifting part 2
- [`adr/2026-06-04-first-party-session-layer.md`](../adr/2026-06-04-first-party-session-layer.md)
  — the decision that removed the `sqlx ^0.8` wall
- [UNK-101](https://linear.app/unkos/issue/UNK-101) — tower-sessions
  ecosystem bump (the coordinated work this rides on)
- `backend/Cargo.toml:47` — sqlx pin site
- `.github/workflows/ci.yml` — `SQLX_CLI_VERSION` pin site
