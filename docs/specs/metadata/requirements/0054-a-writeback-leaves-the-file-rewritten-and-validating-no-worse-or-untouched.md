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
outcome beforehand MUST be reverted to the original bytes before the writeback is recorded as failed.

## Rationale

This rewrite runs against a file a reader may already have open or be downloading, and again long after a manifestation
first entered the library, so any half-written state observable at that path is a corrupted archive with nothing to show
that anything went wrong. A rewrite whose result validates worse than the file did beforehand would trade a working
archive for a degraded or unreadable one for the sake of a metadata change no reader asked to risk their file over;
reverting it before the writeback is marked failed keeps a metadata change from ever leaving a manifestation's file
worse than it already was.

## Acceptance criteria

- A writeback whose rewritten file validates no worse than before the rewrite replaces the file atomically with the
  rewritten archive. Checked by `run_once_finds_non_default_opf_and_updates_hash` in
  `backend/src/services/writeback/orchestrator.rs`.
- A writeback whose rewritten file validates worse than the file validated before the rewrite restores the original
  bytes at the same path, and this restoration happens before the writeback is recorded as failed. Checked by
  `finalise_post_writeback_rolls_back_on_regression` in `backend/src/services/writeback/orchestrator.rs`. The
  manifestation's stored file hash and file path are left at their pre-writeback values by a rolled-back attempt, since
  the code path that updates them is never reached when a rollback occurs; no automated test inspects the stored row
  after a rollback to confirm this directly.
- A writeback whose post-write validation itself fails to run, rather than producing a worse or an equal outcome, is
  treated the same as a worse outcome and restores the original bytes. Checked by
  `finalise_post_writeback_rolls_back_on_validator_error` in `backend/src/services/writeback/orchestrator.rs`.
- When the rewritten file is persisted across a filesystem boundary, the persisted bytes are compared against the source
  bytes by content hash, and only once they match is the intermediate file removed. Checked by
  `exdev_fallback_writes_same_bytes` in `backend/src/services/writeback/path_rename.rs`, which exercises the comparison
  directly; a genuine cross-filesystem environment, and the case where the hash comparison fails, are not exercised by
  any automated test.
- No automated test kills the process between finishing the rewritten file and putting it in place of the original, or
  between finishing the restored bytes and putting them in place during a rollback; the original file's state at either
  exact instant is not exercised by anything in the repository.
