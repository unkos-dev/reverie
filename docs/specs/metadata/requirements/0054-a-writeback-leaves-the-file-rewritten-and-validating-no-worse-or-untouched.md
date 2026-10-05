---
type: REQ
profile-version: 1
id: "REV-REQ-0054"
title: "A writeback leaves the file rewritten and validating no worse, or untouched"
---

# A writeback leaves the file rewritten and validating no worse, or untouched

## Statement

WHEN a writeback rewrites the file behind a manifestation, the file at that manifestation's path MUST end up, once the
writeback finishes, either the complete rewritten archive with a validation outcome no worse than the file's own outcome
before the rewrite began, or byte-for-byte identical to the file's state before the rewrite began; a partially written
state MUST NOT be observable at that path, and a rewrite whose resulting validation outcome is worse than the file's
outcome beforehand MUST be rejected before publication, leaving the original source bytes unchanged.

## Rationale

This rewrite runs against a file a reader may already have open or be downloading, and again long after a manifestation
first entered the library, so any half-written state observable at that path is a corrupted archive with nothing to show
that anything went wrong. A rewrite whose result validates worse than the file did beforehand would trade a working
archive for a degraded or unreadable one for the sake of a metadata change no reader asked to risk their file over;
refusing the finished candidate before publication keeps a metadata change from leaving the file worse than it was.
Successful repair is recorded separately; unresolved severity determines whether the candidate validates no worse.

## Acceptance criteria

- A writeback whose rewritten file validates no worse than before the rewrite replaces the file atomically with the
  rewritten archive. Checked by `run_once_finds_non_default_opf_and_updates_hash` in
  `backend/src/services/writeback/orchestrator.rs`.
- A candidate that validates worse is rejected before replacement, leaving source bytes and the stored location/hash
  unchanged. Checked by `candidate_publication_rejects_regression_before_replacement` in
  `backend/src/services/epub/mod.rs`.
- A validator or required repair error prevents publication and preserves the source. Checked by
  `candidate_publication_validator_error_leaves_source_untouched` and
  `candidate_publication_required_repair_error_leaves_source_untouched` in `backend/src/services/epub/mod.rs`.
- Accepted candidate report, hash and size describe the final persisted bytes. Checked by
  `candidate_publication_accepts_final_archive_and_hash` in `backend/src/services/epub/mod.rs`.
- Cross-filesystem relocation independently verifies the destination before removing the source. Corruption and read
  failures preserve the source. Checked by the `contained_move_cross_fs_` cases in
  `backend/src/services/writeback/path_rename.rs` through the injected EXDEV rename boundary.
- No automated test kills the process during replacement, parent sync or the interval between a file move and SQL
  bookkeeping.
