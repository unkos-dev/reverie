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

Repair can change a file a reader already has open. Publishing an unfinished archive would corrupt that reader's next
open and conceal whether the repair completed. A finished candidate checked before replacement keeps repair failures
from exposing partial or rejected content.

## Acceptance criteria

- A repair that completes successfully leaves the file as a complete, readable archive carrying every repaired-severity
  fix applied in that one pass. Checked by `invalid_mimetype_is_repaired_end_to_end` in
  `backend/src/services/epub/mod.rs` and `repackage_mimetype_is_first_and_stored` in
  `backend/src/services/epub/repair.rs`.
- A required repair that cannot read or convert an entry propagates its failure and leaves the original location
  unchanged. Checked by `candidate_publication_required_repair_error_leaves_source_untouched` in
  `backend/src/services/epub/mod.rs`.
- Rejection of the finished candidate leaves the original file exactly as it was. Pre-publication validator refusal is
  checked by `candidate_publication_validator_error_leaves_source_untouched` in `backend/src/services/epub/mod.rs`. An
  error after callback acceptance is reported as publication uncertainty; the source may be the complete candidate. No
  automated test interrupts file replacement or parent sync.
