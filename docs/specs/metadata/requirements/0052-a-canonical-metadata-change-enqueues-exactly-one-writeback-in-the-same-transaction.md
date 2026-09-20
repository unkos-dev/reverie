---
type: REQ
profile-version: 1
id: "REV-REQ-0052"
title: "A canonical metadata change enqueues exactly one writeback in the same transaction"
governed-by:
  - "REV-ADR-0020"
---

# A canonical metadata change enqueues exactly one writeback in the same transaction

## Statement

WHEN a canonical metadata field on a work or a manifestation changes value, whether through an automated apply, an
accept, a revert, or a manual edit, the system MUST enqueue exactly one writeback job for that manifestation as part of
the same atomic change, so that the field change and the enqueue either both take effect or neither does; a change to an
external-identifier field MUST NOT enqueue a writeback job, because an external identifier is never written back to a
file.

## Rationale

The writeback pipeline depends on a job appearing whenever a value it must reflect onto a file has changed; a reader
trusts that the file on disk stays in step with the catalogue's own record. If the enqueue could commit without the
field change, or the field change without the enqueue, the file and the catalogue would drift apart with nothing left to
reconcile them after a crash. An external identifier is a catalogue-only fact that no file format this system handles
carries, so a job for one would waste an attempt on a field the file has nowhere to hold.

## Acceptance criteria

- An automated apply that changes a canonical field enqueues exactly one writeback job as part of the same change.
  Checked by `orchestrator_autofill_applies_when_canonical_empty` and
  `apply_canonical_batch_breaks_after_first_apply_on_agreement` in `backend/src/services/enrichment/orchestrator.rs`.
- Accepting a pending version enqueues exactly one writeback job. Checked by `accept_admin_writes_canonical_title` in
  `backend/src/routes/metadata.rs`.
- Reverting a field, whether to a specific earlier version or to no value, enqueues exactly one writeback job. Checked
  by `revert_admin_clears_field_to_null` in `backend/src/routes/metadata.rs`.
- A manual edit enqueues exactly one writeback job for each field the request changes: a single-field edit enqueues one
  job and a two-field edit enqueues two. Checked by `patch_sets_title_and_writes_canonical` and
  `patch_two_fields_enqueues_one_writeback_per_field` in `backend/src/routes/metadata.rs`.
- Two independent accepts against the same manifestation enqueue two separate jobs; a new job is never collapsed into
  one already queued for the same manifestation. Checked by `double_accept_enqueues_two_jobs` in
  `backend/src/routes/metadata.rs`.
- Setting or clearing an external identifier, whether by accept or by manual edit, enqueues no writeback job. Checked by
  `patch_clears_identifier_slot_without_writeback` and `accept_staged_identifier_writes_registry_without_writeback` in
  `backend/src/routes/metadata.rs`.
- An automated apply that is rejected before the field is actually written enqueues no writeback job. Checked by
  `apply_canonical_batch_skips_malformed_pub_date` in `backend/src/services/enrichment/orchestrator.rs`, which rejects a
  malformed value and asserts zero writeback rows afterward. No automated test interrupts the enclosing transaction
  itself between the field write and the enqueue to confirm the two roll back together; that guarantee rests on both
  writes sharing one transaction, not on a check specific to this path.
