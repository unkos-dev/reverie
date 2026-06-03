---
status: lifted
severity: medium
surfaces: [end-user, developer]
adopted: 2026-05-24
adopted-because: "11c (PR #316) ships PATCH /api/books/{id}/metadata without ISBN format validation; prioritised the merge-patch flow over input validation of a single field"
lift-when-class: internal-refactor
lift-when: PR adds ISBN-10/ISBN-13 checksum + length validation to the PATCH metadata endpoint; same PR adds tests for malformed ISBNs returning 422
lifted: 2026-06-03
lifted-because: "update_book_metadata now runs present isbn_10/isbn_13 values through services::metadata::isbn::checked_isbn10/checked_isbn13 before journalling: invalid values (bad length/check-digit/non-numeric) return AppError::Validation (422); valid values are normalised to digits-only (uppercase X) so the stored value matches the ingestion surface and rematch's exact-equality join finds twins. Guard is confined to the PATCH loop (not apply_version) so accept/revert paths still accept historical pre-validation values. Migration 20260603032915_normalise_existing_isbns backfills pre-existing dashed/spaced/prefixed rows. Tests: patch_isbn13_bad_check_digit_returns_422, patch_isbn10_wrong_length_returns_422, patch_isbn13_non_numeric_returns_422, patch_valid_isbn10_accepted, patch_hyphenated_isbn13_stored_normalized, patch_dashed_isbn_rematches_undashed_twin, plus checked_isbn10/13 unit tests"
superseded-by: https://github.com/unkos-dev/reverie/pull/414
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
