---
type: REQ
profile-version: 1
id: "REV-REQ-0044"
title: "A repair leaves the file fully rewritten or untouched"
---

# A repair leaves the file fully rewritten or untouched

## Statement

WHEN the repair step is applied to a file, the file at that location MUST end up either the complete repaired archive or
byte-for-byte identical to the file's own state before the repair began, with no partially written state ever observable
at that location, whether the repair succeeds or fails.

## Rationale

The ingestion pipeline invokes this repair while first building the library's copy of a file, and the Writeback pipeline
subject invokes it again afterward, against a file a reader may already be opening, solely to detect a regression in its
own edit. A half-applied repair would corrupt the archive with no indication that anything failed, and a reader, or a
repair or validation pass run against the same file afterwards, could encounter it before anyone notices. This guarantee
is what stands between a failure at any point during the repair and a manifestation whose file no reader can open.

## Acceptance criteria

- A repair that completes successfully leaves the file as a complete, readable archive carrying every repaired-severity
  fix applied in that one pass. Checked by `invalid_mimetype_is_repaired_end_to_end` in
  `backend/src/services/epub/mod.rs` and `repackage_mimetype_is_first_and_stored` in
  `backend/src/services/epub/repair.rs`.
- A repair that fails partway, for example because an entry it needs to rewrite cannot be read back from the source
  archive, propagates that failure rather than returning a partially rewritten result, and leaves the file at the
  original location unmodified. Not checked by any automated test: no test induces a mid-repair failure and then
  inspects the original file.
- A failure between finishing the rewritten copy and putting it in place of the original leaves the original file
  exactly as it was, never a mixture of the two. Not checked by any automated test: nothing in the repository interrupts
  a repair at that exact point.
