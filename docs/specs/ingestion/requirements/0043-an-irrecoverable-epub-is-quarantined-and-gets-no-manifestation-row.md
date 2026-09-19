---
type: REQ
profile-version: 1
id: "REV-REQ-0043"
title: "An irrecoverable EPUB is quarantined and gets no manifestation row"
---

# An irrecoverable EPUB is quarantined and gets no manifestation row

## Statement

WHEN the structural validation performed during EPUB ingestion reports a finding of irrecoverable severity, the
ingestion pipeline MUST move the file to quarantine together with a sidecar record of the reason, and MUST NOT create a
manifestation row for it.

## Rationale

A manifestation row asserts that a file is part of the library and ready to be opened by a reader; a file whose own
structure the validator could not accept must never carry that assertion, or a subsequent attempt to open it fails
against a row that promised something usable. Moving the file to quarantine rather than deleting it preserves the
evidence an operator needs to diagnose the failure, instead of leaving no trace of a file that was offered for ingestion
and rejected.

## Acceptance criteria

- A corrupt archive, one that does not parse as a valid ZIP structure, is moved to quarantine and produces no
  manifestation row. Checked by `scan_once_quarantines_corrupt_epub` in
  `backend/src/services/ingestion/orchestrator.rs`.
- An archive containing an entry with an unsafe name reaches the same irrecoverable outcome as a corrupt archive:
  quarantined, with no manifestation row created. The unsafe name finding itself is checked by
  `path_traversal_is_quarantined` in `backend/src/services/epub/zip_layer.rs`; the quarantine handling that follows any
  irrecoverable finding is checked end-to-end by `scan_once_quarantines_corrupt_epub` in
  `backend/src/services/ingestion/orchestrator.rs`.
- Quarantining a file produces a sidecar file alongside it in the quarantine location. Checked by
  `scan_once_quarantines_corrupt_epub` in `backend/src/services/ingestion/orchestrator.rs`, which asserts the quarantine
  location holds at least one entry afterwards; no test asserts the sidecar's own content.
- No manifestation row exists at the file's intended destination path once a quarantined ingestion attempt completes.
  Checked by `scan_once_quarantines_corrupt_epub` in `backend/src/services/ingestion/orchestrator.rs`.
- A validator that itself fails to run, rather than reporting an irrecoverable finding, is not this obligation's
  trigger: the file is still moved into the library with a manifestation row, and its validation status is recorded as
  failed rather than any other value. Checked by `scan_once_validator_error_stores_failed_status` in
  `backend/src/services/ingestion/orchestrator.rs`.
