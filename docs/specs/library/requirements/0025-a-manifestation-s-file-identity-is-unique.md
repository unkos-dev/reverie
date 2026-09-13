---
type: REQ
profile-version: 1
id: "REV-REQ-0025"
title: "A manifestation's file identity is unique"
---

# A manifestation's file identity is unique

## Statement

The library MUST NOT hold two manifestations with the same file identity, where a file identity is the file path and the
content hash taken at ingestion; WHEN a file matching an existing path or an existing ingestion hash is ingested again,
the ingestion MUST be skipped or refused without creating a row.

## Rationale

A file is the unit of the library, and one file counted twice would double every downstream count and edit. The hash
catches a moved copy and the path catches an in-place rewrite.

## Acceptance criteria

- No two rows in `manifestations` share a `file_path` or an `ingestion_file_hash`. Enforced by the
  `manifestations_file_path_key` and `manifestations_file_hash_unique` constraints in `backend/schema.sql`, CI-rebuilt
  from the migrations and diffed. No automated check exercises a constraint violation directly.
- Ingesting a file whose path or ingestion hash already exists in the library skips it rather than creating a second
  row. Checked by `scan_once_skips_duplicate_on_second_run` in `backend/src/services/ingestion/orchestrator.rs`.
