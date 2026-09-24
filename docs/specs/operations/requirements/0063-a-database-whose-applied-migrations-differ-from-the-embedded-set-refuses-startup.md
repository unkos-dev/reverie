---
type: REQ
profile-version: 1
id: "REV-REQ-0063"
title: "A database whose applied migrations differ from the embedded set refuses startup"
governed-by:
  - "REV-ADR-0014"
---

# A database whose applied migrations differ from the embedded set refuses startup

## Statement

WHEN automatic migration is disabled and the set of migrations the database records as applied differs, in either
direction, from the set the running binary embeds, Reverie MUST refuse to serve requests.

## Rationale

The two directions fail differently and both fail badly unattended. A database ahead of the binary answers queries the
binary no longer expects; a database behind it is missing columns the binary reads, which surfaces as scattered SQL
errors spread across whichever requests happen to touch the missing objects rather than as one legible refusal at the
moment of the mistake. Behind is also the more common operator error, since deploying a new image without running the
migration step produces it. An operator who is mid-upgrade depends on the refusal to tell them the upgrade is half done
while the old image is still available to pin.

## Acceptance criteria

- With automatic migration disabled and the database recording a migration version the binary does not embed, startup
  fails and no request is served. Checked by `verify_schema_current_detects_ahead` in `backend/src/db.rs`; that no
  request is served is determined by reading `run` in `backend/src/lib.rs`, which verifies the schema before it binds
  the listener.
- With automatic migration disabled and the binary embedding a migration version the database does not record, startup
  fails and no request is served. Checked by `verify_schema_current_detects_behind` in `backend/src/db.rs`; that no
  request is served is determined by reading `run` in `backend/src/lib.rs`, which verifies the schema before it binds
  the listener.
- With automatic migration disabled against a database that has never been migrated, startup fails reporting that the
  database is not initialised, rather than propagating the missing-relation error the absent tracking table would
  otherwise raise. Checked by `verify_schema_current_table_absent_is_not_initialized` in `backend/src/db.rs`.
- With automatic migration disabled and the two sets matching exactly, startup proceeds. Checked by
  `apply_or_verify_flag_off_verifies_ok` in `backend/src/lib.rs`.
