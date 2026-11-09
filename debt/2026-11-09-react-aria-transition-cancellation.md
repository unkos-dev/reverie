---
severity: medium
surfaces: [end-user, ci]
adopted: 2026-11-09
adopted-because: React Aria 3.53.1 can leave initial dialog focus waiting on cancelled CSS transitions
lift-when-class: dep-unblocks
lift-when: a released react-aria version contains upstream commit 390f18208720ff528d814b9b0de7a55340d5137f and the Settings browser tests pass without the local patch
---

# React Aria transition cancellation

React Aria 3.53.1 registers its transition cancellation listener with `once: true`. When several properties cancel, only
the first cancellation reaches its transition tracker. The tracker can then delay initial dialog focus indefinitely,
leaving focus on the document body. The existing Settings browser test reproduces this failure.

`frontend/patches/react-aria@3.53.1.patch` applies
[the upstream correction](https://github.com/adobe/react-spectrum/commit/390f18208720ff528d814b9b0de7a55340d5137f)
to the published JavaScript module formats. The pnpm registration matches only `react-aria@3.53.1`; it does not apply to
newer versions. The default unused-patch check remains enabled.

When upgrading to a release containing that commit, run `pnpm patch-remove react-aria@3.53.1` and commit the regenerated
lockfile. Remove the Dockerfile's patch-directory `COPY`, this entry and its README link in the same change. A frozen
install and the existing Settings browser tests must pass without the patch.
