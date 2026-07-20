---
severity: low
surfaces: [ci]
adopted: 2026-07-20
adopted-because: the vite-plus npm alias fails plugin peer ranges, so a clean npm resolution and Renovate lock file maintenance both error with ERESOLVE
lift-when-class: upstream
lift-when: vp exposes a non-aliased vite surface (or ships a vite version that satisfies plugin `vite@^5||^6||^7||^8` peer ranges), so a clean `npm install --package-lock-only` regenerates the root lockfile with default (strict) peer resolution
---

# Root .npmrc forces legacy-peer-deps for the vite-plus alias

vp installs its vite fork under the `vite` npm alias
(`"vite": "npm:@voidzero-dev/vite-plus-core@0.2.5"`). The lock entry reads
`vite@0.2.5`, which does not satisfy the peer ranges plugins declare
(`@tailwindcss/vite` wants `vite@^5.2.0 || ^6 || ^7 || ^8`). A clean
resolution therefore fails:

```text
npm error code ERESOLVE
npm error peer overridden vite@"npm:@voidzero-dev/vite-plus-core@0.2.5"
  (was "^5.2.0 || ^6 || ^7 || ^8") from @tailwindcss/vite@4.3.3
```

`npm ci` reads the committed lockfile without re-resolving peers, so CI stays
green, but any clean `npm install` and Renovate lock file maintenance (which
deletes and regenerates the lockfile) hit the error. Renovate could refresh
`backend/Cargo.lock` but not `package-lock.json`, leaving the branch's
`renovate/artifacts` check red.

The root `.npmrc` sets `legacy-peer-deps=true` so clean installs and the
Renovate regenerate step reproduce the resolution the lockfile already encodes.
The cost is that the flag relaxes peer-dependency checks across the npm
workspaces, not just for the vite alias. Lockfile integrity is unaffected:
`npm ci` still verifies the committed hashes.

This shares a root cause with the vite-alias dependency-review suppression in
`2026-06-30-vite-plus-alias-dependency-review.md`; both lift when vp stops
aliasing vite.
