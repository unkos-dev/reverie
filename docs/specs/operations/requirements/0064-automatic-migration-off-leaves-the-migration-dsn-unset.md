---
type: REQ
profile-version: 1
id: "REV-REQ-0064"
title: "Automatic migration off leaves the migration DSN unset"
governed-by:
  - "REV-ADR-0014"
---

# Automatic migration off leaves the migration DSN unset

## Statement

WHEN automatic migration is disabled, Reverie's loaded configuration MUST leave `migration_database_url` unset,
regardless of whether `DATABASE_URL_MIGRATION` is present in the process environment.

## Rationale

Every pool Reverie opens, and the startup branch that decides whether to migrate or merely verify, read the loaded
configuration rather than the environment. Clearing the field prevents the server's configuration and its clones from
retaining an unused migration credential. The startup selector independently checks `auto_migrate` before using that
field.

Applying the flag after deserialisation, on every load, is what keeps the field unset whether or not the operator
remembers to omit the variable: exporting `DATABASE_URL_MIGRATION` so the separate migrate step can use it does not also
put it into the configuration the server runs on.

## Acceptance criteria

- With automatic migration disabled and `DATABASE_URL_MIGRATION` set in the process environment, the loaded
  configuration leaves `migration_database_url` unset. Checked by `migration_url_nulled_when_auto_migrate_off` in
  `backend/src/config/mod.rs`.
- With automatic migration disabled and `DATABASE_URL_MIGRATION` set to an empty value, the loaded configuration leaves
  `migration_database_url` unset. Checked by `from_env_empty_migration_url_treated_as_none_when_auto_migrate_off` in
  `backend/src/config/mod.rs`.
