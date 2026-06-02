---
status: active
severity: low
surfaces: [developer]
adopted: 2026-06-02
adopted-because: sqlx 0.9 bump (Renovate #326 crate, #325 sqlx-cli) cannot compile — tower-sessions-sqlx-store 0.15.0 (latest) pins sqlx ^0.8, so our 0.9 PgPool mismatches the store's 0.8 PgPool
lift-when-class: dep-unblocks
lift-when: tower-sessions-sqlx-store crates.io release accepting sqlx ^0.9 — part of the same coordinated tower-sessions ecosystem bump that lifts the tower-sessions 0.14 pin (UNK-101)
lifted: ~
superseded-by: ~
---

# sqlx pinned to 0.8 (tower-sessions-sqlx-store peer pin)

## Constraint

`backend/Cargo.toml:47` pins `sqlx = "0.8.6"`. Renovate
[#326](https://github.com/unkos-dev/reverie/pull/326) (`sqlx → 0.9.0`)
and its paired [#325](https://github.com/unkos-dev/reverie/pull/325)
(`sqlx-cli → 0.9.0`, the `.sqlx` offline-cache toolchain) fail to
compile.

The break has **two distinct parts**, and only one is ours to fix:

1. **Our code — mechanical, ~20 sites.** sqlx 0.9 removed the `'q`
   lifetime from `QueryBuilder`, so every `QueryBuilder<'_, Postgres>`
   (and `Separated<…>`) annotation is an `E0107` ("struct takes 0
   lifetime arguments but 1 supplied"). 18 sites across
   `routes/library/mod.rs`, `routes/opds/library.rs`,
   `routes/opds/scope.rs`, `db.rs`. Plus one `E0599` (query strings are
   now `SqlStr`, a `.as_bytes()` call needs `.as_str()` first) and one
   `E0521` borrow-escape fallout from a builder helper signature. All
   mechanical; ~half a day including regenerating the `.sqlx` cache with
   cli 0.9.

2. **Upstream wall — not fixable in our code.**
   `tower-sessions-sqlx-store` latest is **0.15.0** (published
   2025-01-01) and pins **sqlx `^0.8.0`**. With sqlx 0.9 in the tree the
   store still builds against 0.8, so `PgPool` (and the rest of sqlx's
   public types) are two incompatible types. Our calls into the store —
   `PostgresStore::new(pool)` and friends — become `E0308` mismatched
   types (7 sites in `lib.rs`, `services/session_sweep.rs`,
   `services/settings.rs`, all noting
   `tower-sessions-sqlx-store-0.15.0/src/postgres_store.rs:33`). No
   amount of edits on our side resolves this; it needs a
   tower-sessions-sqlx-store release built against sqlx 0.9.

This is the **same dependency wall** that blocks the tower-sessions 0.14
pin (see [`2026-05-21-tower-sessions-0-14-pin.md`](2026-05-21-tower-sessions-0-14-pin.md)
/ PR `#128`). The tower-sessions / tower-sessions-sqlx-store / axum-login
trio moves as a unit; sqlx 0.9 is gated behind the same coordinated bump.

## Workaround

Keep `sqlx = "0.8.6"` (`backend/Cargo.toml:47`) and
`SQLX_CLI_VERSION: "0.8.6"` (`.github/workflows/ci.yml`). Hold Renovate
PRs `#326` and `#325` open, do **not** start the mechanical code
migration — it cannot land until part 2 lifts, and a half-done branch
rots against every `main` merge that touches a query.

## Why this isn't the right shape

- sqlx 0.9 is a genuine version behind; holding it indefinitely accrues
  the usual upgrade-debt interest (eventual larger jump, missed fixes).
- The migration _looks_ mechanical from the CI error summary, which
  invites someone to pick it up as a quick win — then hit the
  unfixable `E0308` store wall and waste the round-trip. This entry
  exists primarily to record that the work is **blocked, not merely
  unstarted**, so it isn't re-scoped as "easy" later.

Severity is `low`: sqlx 0.8.6 is fully functional for reverie; nothing
in 0.9 is load-bearing for the current threat model.

## Lift conditions

Lift trigger: a `tower-sessions-sqlx-store` crates.io release that
accepts `sqlx ^0.9` (expected alongside the broader tower-sessions
ecosystem bump that lifts UNK-101 / PR #128).

When that release ships:

1. Bump `tower-sessions-sqlx-store` (and the paired tower-sessions /
   axum-login bumps per the sister entry) in `backend/Cargo.toml`.
2. Land Renovate PR `#326` (`sqlx → 0.9`): drop the `'q` lifetime from
   every `QueryBuilder<'_, Postgres>` / `Separated<…>`; fix the `SqlStr`
   `.as_bytes()` site; resolve the one borrow-escape.
3. Land Renovate PR `#325` (`sqlx-cli → 0.9`) in the **same** PR and
   regenerate the `.sqlx` offline cache so the cli and crate versions
   stay locked (cli/crate skew corrupts the cache format).
4. Run the `cargo sqlx prepare --check` round-trip; `cargo test`.
5. Flip this entry to `status: lifted`, set `lifted`, `superseded-by`.
6. Update README "Active" → "Lifted" listing.

## Related

- [PR #326](https://github.com/unkos-dev/reverie/pull/326) — Renovate
  `sqlx → 0.9.0`, held open until lift
- [PR #325](https://github.com/unkos-dev/reverie/pull/325) — Renovate
  `sqlx-cli → 0.9.0`, lands paired with #326
- [`2026-05-21-tower-sessions-0-14-pin.md`](2026-05-21-tower-sessions-0-14-pin.md)
  — sister entry; same tower-sessions ecosystem wall
- [UNK-101](https://linear.app/unkos/issue/UNK-101) — tower-sessions
  ecosystem bump (the coordinated lift work this rides on)
- `backend/Cargo.toml:47` — sqlx pin site
- `.github/workflows/ci.yml` — `SQLX_CLI_VERSION` pin site
