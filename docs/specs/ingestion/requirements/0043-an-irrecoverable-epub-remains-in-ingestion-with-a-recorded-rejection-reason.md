---
type: REQ
profile-version: 1
id: "REV-REQ-0043"
title: "An irrecoverable EPUB remains in ingestion with a recorded rejection reason"
---

# An irrecoverable EPUB remains in ingestion with a recorded rejection reason

## Statement

WHEN EPUB ingestion reports an irrecoverable validation finding, Reverie MUST preserve the watched original in the
ingestion folder with its bytes unchanged, MUST record the rejection reason, and MUST NOT create a manifestation for
that rejected attempt or retain a separate quarantine copy.

## Rationale

An irrecoverable file cannot support a manifestation's assertion that library content is usable. Retaining the original
and its rejection reason gives the operator evidence to correct the input without duplicating storage or changing
externally shared source bytes.

## Acceptance criteria

- A corrupt archive leaves the original byte-for-byte unchanged, records rejected input state and a reason, and creates
  no manifestation or separate file copy. Checked by `capability_ingestion_owner_retains_corrupt_epub_and_reason` and
  `capability_ingestion_coordinator_import_duplicate_rejection_cleanup_and_restart_suppression`.
- An archive with no `container.xml` and no `.opf` file, an empty archive, and an archive with a CRC-32 failure each
  reach the same outcome. Checked by `capability_ingestion_owner_rejects_epub_without_container_or_package`,
  `capability_ingestion_owner_rejects_empty_zip` and `capability_ingestion_owner_rejects_epub_failing_crc`.
- A readable archive with recoverable findings is not rejected. Checked by
  `capability_ingestion_owner_repaired_epub_stores_repaired_status` and
  `capability_ingestion_owner_degraded_epub_stores_degraded_status`.
- An unsafe archive name reaches the same irrecoverable outcome. The archive boundary is checked by
  `path_traversal_is_quarantined`; the ingestion handling applies to every irrecoverable outcome.
- Candidate disposal precedes final publication for rejection. Checked by
  `capability_ingestion_acquisition_rejected_candidate_never_publishes`.
- An unchanged rejected generation remains suppressed across startup and an admin scan. A changed fingerprint restores
  eligibility as a new generation.
- A validator error that is a verdict on the file, a refused repair candidate or a failed required repair, reaches the
  same outcome. Checked by `capability_ingestion_owner_rejects_candidate_refused_by_validation` and
  `capability_ingestion_owner_rejects_epub_whose_required_repair_fails`; `only_content_verdicts_are_file_defects` fixes
  which errors count.
- A validator error that is a storage fault is a separate trigger:
  `capability_ingestion_owner_validator_error_stores_failed_status` checks that the accepted file can be registered with
  validation status `failed`.
