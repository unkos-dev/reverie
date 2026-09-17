---
severity: low
surfaces: [developer, ci]
adopted: 2026-09-17
adopted-because: the pinned SQL linter reports valid PostgreSQL constructs in the initial schema rollup
lift-when-class: internal-refactor
lift-when: the next schema rollup passes SQL lint without the CV11, RF01, RF03, and ST09 exclusions
---

# SQL lint exclusions preserve the initial schema rollup

The initial schema migration excludes four lint rules. CV11 incorrectly reports PostgreSQL shorthand casts, RF01 and
RF03 incorrectly report correlated references in row-level security policies, and ST09 would reorder a join condition
recorded in the generated schema. The remaining rules and parser failures stay enforced.

Remove the exclusions and this entry when the next schema rollup replaces the migration. Verify the new rollup with
`just infra::sqruff` and `just rust::schema-check` before removing either one.
