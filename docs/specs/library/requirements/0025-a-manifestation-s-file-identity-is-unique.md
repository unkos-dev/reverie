---
type: REQ
profile-version: 1
id: "REV-REQ-0025"
title: "A manifestation's file identity is unique"
---

# A manifestation's file identity is unique

## Statement

Within each library, Reverie MUST NOT hold two manifestations with the same relative file path. Across all libraries,
Reverie MUST NOT hold two manifestations with the same content hash taken at ingestion. When either identity already
exists, ingestion MUST skip or refuse the file without creating another manifestation.

## Rationale

A file counted twice would double downstream counts and edits. Library-scoped paths allow independent libraries to use
the same filenames; the global ingestion hash catches duplicate content even after relocation.

## Acceptance criteria

- The same library and relative path cannot appear twice, but different libraries can use the same path with different
  hashes. Checked by `library_storage_schema_library_namespace_and_global_hash`.
- An ingestion hash cannot appear twice, including across libraries. The same schema test exercises this constraint.
- Ingesting a duplicate skips it without another row. Checked by `scan_once_skips_duplicate_on_second_run` in
  `backend/src/services/ingestion/orchestrator.rs`.
