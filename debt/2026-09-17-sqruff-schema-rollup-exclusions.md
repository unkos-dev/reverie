---
severity: low
surfaces: [developer, ci]
adopted: 2026-09-17
adopted-because: the pinned SQL linter reports valid PostgreSQL constructs in the initial schema rollup
lift-when-class: internal-refactor
lift-when: the next schema rollup passes SQL lint without the RF03 and ST09 exclusions
---

# SQL lint exclusions preserve the initial schema rollup

The initial schema migration excludes RF03 because it reports qualified correlated references in row-level security
policies as single-table references. ST09 would reorder a join condition recorded in the generated schema. Three later
view migrations also exclude RF03 around correlated lateral queries. Other rules and parser checks remain active.

Remove these exclusions and this entry when the next schema rollup passes `just infra::sqlfluff` and
`just rust::schema-check` without them.
