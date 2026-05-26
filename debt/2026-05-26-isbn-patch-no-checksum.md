---
status: active
severity: medium
surfaces: [end-user, developer]
adopted: 2026-05-24
adopted-because: "11c (PR #316) ships PATCH /api/books/{id}/metadata without ISBN format validation; prioritised the merge-patch flow over input validation of a single field"
lift-when-class: internal-refactor
lift-when: PR adds ISBN-10/ISBN-13 checksum + length validation to the PATCH metadata endpoint; same PR adds tests for malformed ISBNs returning 422
lifted: ~
superseded-by: ~
---

# ISBN not validated on metadata PATCH

## Constraint

The `PATCH /api/books/{id}/metadata` endpoint (RFC 7396 JSON Merge
Patch) accepts an `isbn` field as a plain string. No checksum digit
validation or length check is performed — any string is accepted and
persisted.

## Workaround

None. Invalid ISBNs are stored as-is. The field is not used for
lookups or dedup today, so the impact is limited to data quality.

## Lift trigger

Add validation (ISBN-10 check digit, ISBN-13 check digit, length)
to the PATCH handler's deserialization or a dedicated validator.
Return 422 for malformed values. Add negative tests covering
common malformations (wrong length, bad check digit, non-numeric).
