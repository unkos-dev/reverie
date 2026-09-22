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
field, so this obligation bounds what the process holds rather than what it does with it.

Applying the flag after deserialisation, on every load, is what makes that hold whether or not the operator remembers to
omit the variable: exporting `DATABASE_URL_MIGRATION` so the separate migrate step can use it does not also put it into
the configuration the server runs on.

## Acceptance criteria

- With automatic migration disabled and `DATABASE_URL_MIGRATION` set in the process environment, the loaded
  configuration leaves `migration_database_url` unset. Checked by `migration_url_nulled_when_auto_migrate_off` in
  `backend/src/config/mod.rs`.
- With automatic migration disabled, startup takes the read-only verification branch and reads `migration_database_url`
  at no point. Checked by `apply_or_verify_flag_off_takes_verify_branch` in `backend/src/lib.rs`.
- With automatic migration enabled, the field is required: a load that cannot supply it fails rather than starting. That
  is the boundary this obligation does not bind. Checked by `auto_migrate_blank_migration_url_is_missing_var` in
  `backend/src/config/mod.rs`.
- Satisfying this obligation says nothing about what the process environment holds. The loader does not unset
  `DATABASE_URL_MIGRATION`, so anything able to read that environment can still read the credential, and satisfaction
  here is not evidence that the serving process is free of a schema-changing credential.
- Satisfying it says nothing about the other connection strings either. Nothing validates what `DATABASE_URL` or
  `DATABASE_URL_INGESTION` is entitled to do, so a privileged credential supplied through one of those reaches the
  serving paths whatever this obligation holds.
