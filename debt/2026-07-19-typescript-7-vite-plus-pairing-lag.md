---
severity: low
surfaces: [developer, ci]
adopted: 2026-07-19
adopted-because: the typescript ~7.0.0 bump; vite-plus 0.2.4 exact-pins oxlint-tsgolint =0.24.0 and declares a typescript ^5||^6 peer, so the bump cannot carry the full toolchain with it
lift-when-class: dep-unblocks
lift-when: the grouped vite-plus bump to >=0.2.5 lands so `npm ls typescript` resolves a single 7.x for the whole workspace (the 0.2.5 core peer already allows ^7), and a vite-plus release ships oxlint-tsgolint >=0.25.0 re-pairing lint type-semantics with the 7.x compiler
---

# Lint type-semantics and the hoisted typescript trail the 7.0 compiler

Frontend `typescript` is `~7.0.0` (the native compiler), but two toolchain
partners could not move in the same PR because vite-plus owns their pins:

- `oxlint-tsgolint`, the `vp lint` type-aware engine and the repo's sole
  typechecker, is exact-pinned at `=0.24.0` by vite-plus. That build rides a
  typescript-go snapshot cut 13 days before the 7.0 GA; 0.25.0 rides a GA-day
  snapshot. Lint type-semantics can therefore lag the installed compiler at
  the margins.
- The root-hoisted `node_modules/typescript` stays at 6.0.3 to satisfy the
  installed vite-plus-core 0.2.4 `typescript: "^5.0.0 || ^6.0.0"` optional
  peer, so the tree resolves two typescript versions. `docs/` (which has no
  typescript devDependency of its own) and any root-resolved tooling, editor
  tsserver included, still see 6.0.3.

Overriding either pin would fork vite-plus's tested pairing, so both moves
ride vite-plus releases instead, and they lift in two stages. vite-plus 0.2.5
already widens the core typescript peer to `^5 || ^6 || ^7` (its release
notes call out TypeScript 7 support for declaration generation), so the
hoisted-6.0.3 half lifts as soon as the ordinary grouped vite-plus bump
lands. The tsgolint half waits longer: 0.2.5 still keeps `=0.24.0` while
bumping oxlint and oxfmt, so re-pairing lint snapshots needs a later
vite-plus release. The native `tsc --noEmit` and the type-aware pass agree on
the current tree, so the lag is latent, not observed. On full lift, confirm
the single-version resolution and drop this entry.
