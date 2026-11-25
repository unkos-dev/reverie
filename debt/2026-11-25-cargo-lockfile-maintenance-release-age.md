---
severity: low
surfaces: [ci, security]
adopted: 2026-11-25
adopted-because: Cargo 1.100 downgrades already-locked crates younger than global-min-publish-age during full cargo update (rust-lang/cargo#17642), reverting cookie 0.18.3 to vulnerable 0.18.2 on PR #2004
lift-when-class: dep-unblocks
lift-when: the pinned Rust toolchain's cargo update preserves already-locked versions younger than global-min-publish-age while still rejecting newly selected versions younger than the threshold
---

# Cargo lockfile maintenance runs without a release-age hold

`.github/renovate.json` sets `lockFileMaintenance.minimumReleaseAge` to `null` and sets
`minimumReleaseAgePackageManager` to `"2 days"` on `npm` only. Cargo's targeted dependency pull requests still pass
`CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE` through top-level `minimumReleaseAgePackageManagerSync`, so new transitives
pulled in by direct crate updates wait two days, while daily `lockFileMaintenance` runs `cargo update` without a
publish-age override.

Cargo 1.100 downgrades crates already recorded in `Cargo.lock` when their publish time falls inside
`global-min-publish-age` during a full `cargo update` (tracked upstream as `rust-lang/cargo#17642`). On PR #2004 that
reverted 13 crates merged hours earlier, including `cookie` 0.18.3 back to 0.18.2 (`RUSTSEC-2026-0594`).

Remove this workaround when Reverie pins a Rust toolchain whose full `cargo update` keeps already-locked young crates
while still rejecting newly selected crates younger than the hold. At that point add
`"cargo": { "minimumReleaseAgePackageManager": "2 days" }` (or drop `lockFileMaintenance.minimumReleaseAge: null`),
update `REV-ADR-0050`, and delete this entry and its README link.
