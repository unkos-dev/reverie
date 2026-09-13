---
type: REQ
profile-version: 1
id: "REV-REQ-0028"
title: "Deleting a metadata version leaves the canonical value in place"
---

# Deleting a metadata version leaves the canonical value in place

## Statement

WHEN a metadata version is deleted, every canonical field value it supplied MUST remain on the work or manifestation,
and only the attribution to that version MUST be cleared.

## Rationale

[PostgreSQL's foreign-key actions](https://www.postgresql.org/docs/current/ddl-constraints.html#DDL-CONSTRAINTS-FK)
make the choice between losing the value and losing its attribution; the model keeps the value.

## Acceptance criteria

- Every foreign key to `metadata_versions` in `backend/schema.sql` is `ON DELETE SET NULL`: 17 such foreign keys exist
  in the dump, CI-rebuilt from the migrations, and none uses a different action. No test deletes a metadata version; the
  constraint action is the check.
