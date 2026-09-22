---
type: REQ
profile-version: 1
id: "REV-REQ-0064"
title: "The serving process holds no schema-changing credential"
governed-by:
  - "REV-ADR-0014"
---

# The serving process holds no schema-changing credential

## Statement

WHEN automatic migration is disabled, the Reverie process that serves requests MUST NOT hold a credential able to change
the database schema, whatever the process environment supplies.

## Rationale

The credential that can alter the schema is the credential that can drop it, and the serving process is the one exposed
to every request. Keeping that credential out of it means anything reaching the long-lived process finds only the
row-scoped application identity, so the worst a compromise there reaches is data the row-level-security policies already
bound. The operator running the shipped topology depends on this without doing anything to obtain it, because they never
set the migration variable on the serving service. The operator who does set it, from an older deployment or a copied
environment file, depends on it more heavily: nothing else would stop the process from picking the credential up and
holding it for its whole life.

## Acceptance criteria

- With automatic migration disabled and the migration connection string present in the process environment, the loaded
  configuration holds no migration credential. Checked by `migration_url_nulled_when_auto_migrate_off` in
  `backend/src/config/mod.rs`.
- With automatic migration disabled, startup verifies the schema over the application credential and reads no migration
  credential at any point. Checked by `apply_or_verify_flag_off_takes_verify_branch` in `backend/src/lib.rs`.
- With automatic migration enabled, the process does hold the migration credential for as long as it runs. That is the
  trade an operator without orchestration opts into, and this obligation does not bind it.
