---
type: REQ
profile-version: 1
id: "REV-REQ-0026"
title: "A manifestation's ingestion hash never changes"
---

# A manifestation's ingestion hash never changes

## Statement

Once a manifestation exists, its ingestion hash MUST NOT change, whatever later rewrites the file.

## Rationale

The ingestion hash is the identity the file held when it entered the library, and it is the anchor a later on-disk
rewrite is measured against through `current_file_hash`.

## Acceptance criteria

- After a manifestation's file is rewritten more than once by the writeback pipeline, `ingestion_file_hash` still holds
  the value recorded at ingestion while `current_file_hash` reflects each rewrite. Checked by
  `ingestion_file_hash_immutable_across_writeback_chain` in `backend/src/services/writeback/orchestrator.rs`. No schema
  constraint or runtime guard enforces this; the test is the check.
