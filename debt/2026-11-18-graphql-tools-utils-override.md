---
severity: low
surfaces: [developer, ci]
adopted: 2026-11-18
adopted-because: GraphQL-ESLint 4.4.2 depends on a version of @graphql-tools/utils with two high-severity prototype pollution advisories
lift-when-class: dep-unblocks
lift-when: a GraphQL-ESLint release resolves @graphql-tools/utils to a version unaffected by GHSA-7mx3-vvmw-hjmv and GHSA-c4qx-w738-7hr3 without the override, and just js::schema-lint passes and still fails a planted connection-shape violation
---

# GraphQL-ESLint dependency override

The workspace overrides `@graphql-tools/utils@10` with `12.1.0` to address
[GHSA-7mx3-vvmw-hjmv](https://github.com/advisories/GHSA-7mx3-vvmw-hjmv) and
[GHSA-c4qx-w738-7hr3](https://github.com/advisories/GHSA-c4qx-w738-7hr3). The only affected path is
`@graphql-eslint/eslint-plugin` 4.4.2, a root development dependency, which declares `@graphql-tools/utils` `^10.0.0`.
Both advisories are fixed only from 12.0.2, and the rest of the plugin's dependency tree already resolves 12.1.0.

The plugin runs against a newer major version of the package than it declares. `just js::schema-lint` passes the
exported schema under the override, and planted connection, edge, `PageInfo`, argument and naming violations each still
fail it.

Remove the override when a GraphQL-ESLint release resolves an unaffected version without it. `just js::schema-lint` must
pass and still fail a planted connection-shape violation. Delete this entry and its README link with the override.
