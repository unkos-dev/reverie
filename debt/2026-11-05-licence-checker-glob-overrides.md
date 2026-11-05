---
severity: medium
surfaces: [developer, ci]
adopted: 2026-11-05
adopted-because: the licence checker depends on a vulnerable pattern expansion implementation
lift-when-class: dep-unblocks
lift-when: the licence checker's dependency graph contains no version affected by GHSA-g3hv-75wg-m249 without either override, and a frozen install and all three licence checks pass
---

# Licence checker glob overrides

The workspace overrides `glob@13` with `14.0.3` and `minimatch@10` with `11.0.4` to address
[GHSA-g3hv-75wg-m249](https://github.com/isaacs/minimatch/security/advisories/GHSA-g3hv-75wg-m249). Every affected
dependency path originates from the root development dependency `license-checker-rseidelsohn`.

The overrides move together. Glob 13 bundles vulnerable `minimatch` code in its default export, and its source
implementation expects regular expressions where `minimatch` 11 returns `Matcher` objects. Glob 14 supports those
objects. The other direct `minimatch` consumers use matching APIs that `minimatch` 11 retains.

Remove both overrides when the licence checker's dependency graph resolves no version affected by the advisory without
them. A frozen install and the existing production licence checks for the root, frontend and website must pass. Delete
this entry and its README link with the overrides.
