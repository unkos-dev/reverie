---
severity: low
surfaces: [security, ci]
adopted: 2026-06-30
adopted-because: the vite-plus npm alias carries an encoded core version that dependency-review compares against real-vite advisory ranges
lift-when-class: upstream
lift-when: dependency-review (or the advisory database) checks the upstream Vite version a vite-plus-core release bundles against Vite advisory ranges
---

# dependency-review compares the encoded vite-plus-core version, not the bundled Vite

vp installs its Vite fork under the `vite` npm alias (`"vite": "npm:@voidzero-dev/vite-plus-core@8.3.8001"`), which the
peer graph (`@vitejs/plugin-react`, `@tailwindcss/vite` and `vitest` all want `vite@^8`) requires. The lockfile entry
therefore reads `vite@8.3.8001`, and `dependency-review-action` matches that version against real-vite advisory ranges.

The core encodes the upstream Vite release it bundles plus a fork counter: `8.3.8001` bundles Vite 8.3.8. The encoded
version sorts above the bundled one, so an advisory fixed in Vite 8.3.9 would read as already patched and the gate would
pass while the bundled Vite is still affected. dependency-review cannot see the bundled version, so it cannot catch that
case.

The cost is a manual check: on each `vite-plus` bump, compare the Vite advisories against the bundled upstream version,
as `.github/CONTRIBUTING.md` sets out. Lift the entry once dependency-review or the advisory database resolves the
bundled Vite version for `@voidzero-dev/vite-plus-core`, so the gate itself flags an advisory the bundled release does
not carry the fix for.
