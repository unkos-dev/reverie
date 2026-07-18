---
severity: low
surfaces: [developer, ci]
adopted: 2026-07-19
adopted-because: the typescript ~7.0.0 bump; vite-plus 0.2.4 exact-pins oxlint-tsgolint =0.24.0 and declares a typescript ^5||^6 peer, so the bump cannot carry the full toolchain with it
lift-when-class: dep-unblocks
lift-when: a vite-plus release ships oxlint-tsgolint >=0.25.0 and a typescript ^7 peer range, the grouped vite-plus Renovate bump lands, and `npm ls typescript` resolves a single 7.x for the whole workspace
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
  vite-plus-core `typescript: "^5.0.0 || ^6.0.0"` optional peer, so the tree
  resolves two typescript versions. `docs/` (which has no typescript
  devDependency of its own) and any root-resolved tooling, editor tsserver
  included, still see 6.0.3.

Overriding either pin would fork vite-plus's tested pairing (vite-plus 0.2.5
still keeps `=0.24.0` while bumping oxlint and oxfmt), so both moves ride the
grouped vite-plus Renovate bump instead. The native `tsc --noEmit` and the
type-aware pass agree on the current tree, so the lag is latent, not observed.
On lift, confirm the single-version resolution and drop this entry.
