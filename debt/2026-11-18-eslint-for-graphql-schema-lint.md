---
severity: low
surfaces: [developer, ci]
adopted: 2026-11-18
adopted-because: oxlint's JS plugins cannot parse non-JavaScript files, so GraphQL-ESLint needs ESLint to lint the schema
lift-when-class: dep-unblocks
lift-when: the oxlint bundled with vite-plus lets JS plugins lint non-JavaScript file formats, so GraphQL-ESLint runs under oxlint and ESLint is removed
---

# ESLint runs beside oxlint for schema lint

oxlint is the repository's linter. Its JS plugins accept JavaScript and TypeScript only; the oxc JS-plugins guide lists
custom file formats as not supported yet. GraphQL-ESLint parses `.graphql` files, so `just js::schema-lint` runs it
under ESLint, configured in the root `eslint.config.mjs` and limited to `backend/schema.graphql`.

The Renovate rule for `vite-plus` reminds the reviewer of each bump to check for custom parser support. When it ships,
load `@graphql-eslint/eslint-plugin` through `lint.jsPlugins` in `vite.config.ts` with the same rules and options, have
the `schema-lint` recipe call `vp lint`, remove `eslint` and `eslint.config.mjs`, and delete this entry, its README link
and the `eslint.config.mjs` path-filter line in the same change. `just js::schema-lint` must still fail on a planted
connection-shape violation.
