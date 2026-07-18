---
severity: low
surfaces: [developer, ci]
adopted: 2026-07-19
adopted-because: the typescript ~7.0.0 bump; vite-plus 0.2.4 exact-pins oxlint-tsgolint =0.24.0 and declares a typescript ^5||^6 peer, so the bump cannot carry the full toolchain with it
lift-when-class: dep-unblocks
lift-when: a vite-plus release ships oxlint-tsgolint >=0.25.0 re-pairing lint type-semantics with the 7.x compiler, and the docs i18next chain (via @astrojs/starlight) accepts a typescript ^7 peer so `npm ls typescript` resolves a single 7.x for the whole workspace
---

# Lint type-semantics and the hoisted typescript trail the 7.0 compiler

Frontend `typescript` is `~7.0.0` (the native compiler), but the rest of the
tree could not move in the same PR, and the two halves lift independently:

- `oxlint-tsgolint`, the `vp lint` type-aware engine and the repo's sole
  typechecker, is exact-pinned at `=0.24.0` by vite-plus (still so in
  vite-plus 0.2.5, which bumps oxlint and oxfmt but not tsgolint). That build
  rides a typescript-go snapshot cut 13 days before the 7.0 GA; 0.25.0 rides
  a GA-day snapshot. Lint type-semantics can therefore lag the installed
  compiler at the margins. Overriding the pin would fork vite-plus's tested
  pairing, so this half waits for the vite-plus release that moves it.
- The root-hoisted `node_modules/typescript` stays at 6.0.3, so the tree
  resolves two typescript versions and `docs/` (which has no typescript
  devDependency of its own) plus any root-resolved tooling, editor tsserver
  included, still see 6.0.3. vite-plus 0.2.5 widens its core typescript peer
  to `^5 || ^6 || ^7`, but a local combination of this bump with the 0.2.5
  upgrade still hoists 6.0.3 even after `npm dedupe`: `i18next` (pulled in by
  `@astrojs/starlight` in `docs/`) peers `typescript: "^5 || ^6"` and is the
  remaining constraint. This half waits for the starlight/i18next chain to
  accept `^7`.

The native `tsc --noEmit` and the type-aware pass agree on the current tree,
so the lag is latent, not observed. On full lift, confirm the single-version
resolution and drop this entry.
