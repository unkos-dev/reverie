---
type: REQ
profile-version: 1
id: "REV-REQ-0008"
title: "Requests write manifestations only within the caller's RLS scope"
governed-by:
  - "REV-ADR-0028"
---

# Requests write manifestations only within the caller's RLS scope

## Statement

WHEN Reverie handles a request other than an administrator's ingestion scan, it MUST NOT write a `manifestations` row
that the requesting account's row-level-security scope does not permit that account to write.

## Rationale

A caller must not be able to make a write it has no permission for. If a request-handling connection could write a
manifestation outside its caller's scope, a caller without permission to change that manifestation could bypass the role
and ownership checks entirely. The administrator's ingestion scan is the one request handler that makes such a write: it
runs through the ingestion pool's unconditional policy, and the admin check is its only boundary, because a scan has no
single caller to scope to.

## Acceptance criteria

- Over an ordinary application connection whose caller may not update a given manifestation (a `child`-role account, or
  no user context at all), an `UPDATE` of that manifestation changes no row. Checked against a migrated database.
- Outside test support, the only code that marks a connection as a writeback connection is the connection setup in
  `init_writeback_pool` (`backend/src/db.rs`); a search of `backend/src` for the setting finds no other writer.
- Outside test support, the administrator's ingestion scan (`scan` in `backend/src/routes/ingestion.rs`) is the only
  request handler that writes a `manifestations` row through the ingestion pool. A search of `backend/src/routes` for
  `state.ingestion_pool` finds that handler and the enrichment dry run, which writes provider responses to `api_cache`
  and no `manifestations` row.
