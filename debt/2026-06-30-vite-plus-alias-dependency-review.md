---
severity: low
surfaces: [security, ci]
adopted: 2026-06-30
adopted-because: PR #558; the vite-plus npm alias makes the fork match real-vite advisories in dependency-review
lift-when-class: upstream
lift-when: vp exposes a non-aliased vite-plus surface (or ships real vite-version metadata), or dependency-review resolves npm aliases, so the gate passes with no allow-ghsas list
---

# dependency-review allow-ghsas suppresses vite-alias false positives

vp installs its vite fork under the `vite` npm alias
(`"vite": "npm:@voidzero-dev/vite-plus-core@0.2.1"`), which the peer graph
(`@vitejs/plugin-react`, `@tailwindcss/vite`, `vitest` all want `vite@^8`)
requires. The lockfile entry therefore reads `vite@0.2.1`, and
`dependency-review-action` matches that version against every historical real-vite
advisory because `0.2.1` sorts below all real vite versions.

The match is a false positive: vite-plus-core 0.2.1 is vite 8.x underneath, which
carries the fixes for the flagged `server.fs.deny` and path-traversal advisories,
and they are dev-server issues that never reach the production build. The CI gate
suppresses the 14 matched GHSAs with an inline `allow-ghsas` list on the
`dependency-review` job in `ci.yml`.

The list is the cost: a vp version bump can surface new real-vite advisories that
match the alias and need adding, which doubles as the deliberate review trigger for
a vp upgrade. Lift the entry (and delete the `allow-ghsas` list) once vp removes the
need for the `vite` alias or ships version metadata dependency-review can resolve,
then confirm the `dependency-review` gate passes clean.
