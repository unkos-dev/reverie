---
type: ADR
profile-version: 1
id: "REV-ADR-0085"
title: "Database-maintained aggregate metadata revisions"
status: "accepted"
recorded-on: "2026-11-24"
decided-on: "2026-11-24"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0069"
---

# Database-maintained aggregate metadata revisions

## Context and problem statement

The catalogue accepts metadata changes from readers, ingestion and enrichment. A validator must distinguish an
intervening change even when the visible value returns to its earlier state or only its attribution changes. The
catalogue retains a version journal for review. Ordinary editing needs a write precondition across all writers.

## Decision drivers

- Detect intervening changes to shared work and edition metadata, including relations and provenance.
- Keep revision advancement atomic with the change it describes.
- Limit saved-edit state to the data needed for write preconditions and version review.

## Considered options

- Database-maintained aggregate metadata revisions.
- Application-maintained aggregate metadata revisions.
- Database-maintained aggregate and field revisions with session-bound restoration receipts.

## Decision outcome

Chosen option: **database-maintained aggregate metadata revisions**, because the database detects changes across all
canonical writers in the same transaction. Aggregate validators protect ordinary saves. Version review uses the metadata
journal.

### Consequences

- Positive. A previous validator stays stale when the canonical value changes away and back.
- Positive. Writers share revision triggers and the catalogue holds no session-bound copies of saved edits.
- Negative. Relational and provenance changes require trigger coverage and ordered parent locks.
- Negative. After a cell save, the reader must make another edit or use version review to restore a prior value.
