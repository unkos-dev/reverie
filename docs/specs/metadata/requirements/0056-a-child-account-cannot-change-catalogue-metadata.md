---
type: REQ
profile-version: 1
id: "REV-REQ-0056"
title: "A child account cannot change catalogue metadata"
governed-by:
  - "REV-ADR-0028"
---

# A child account cannot change catalogue metadata

## Statement

WHEN the caller is a child account, the system MUST refuse any request that would accept, reject or revert a metadata
proposal, set or clear a metadata field lock, patch catalogue metadata, set or clear an external identifier, or trigger
an enrichment run, whatever scope the caller's credential carries. A refused request MUST leave catalogue metadata,
canonical pointers, proposals, field locks and writeback jobs unchanged.

## Rationale

A child account's own credential can carry write scope, since a child manages its own settings and shelves, so scope
alone cannot be what keeps a child from altering metadata the whole household's catalogue shares. Gating on the caller's
identity, independent of scope, protects that shared catalogue from a child's credential regardless of what capability
it happens to carry, and stops a wrongly scoped or compromised child token from corrupting data every other account
relies on.

## Acceptance criteria

- A child account's request to accept a pending metadata proposal returns 403 without writes. Checked by
  `accept_child_account_forbidden_without_writes` in `backend/src/routes/metadata.rs`.
- A child account's request to reject a pending metadata proposal returns 403 without writes. Checked by
  `reject_child_account_forbidden_without_writes` in `backend/src/routes/metadata.rs`.
- A child account's request to revert a field returns 403 without writes. Checked by
  `revert_child_account_forbidden_without_writes` in `backend/src/routes/metadata.rs` for restoring a version and
  clearing a populated field.
- A child account's request to set a metadata field lock returns 403 without writes. Checked by
  `lock_child_account_forbidden_without_writes` in `backend/src/routes/metadata.rs`.
- A child account's request to clear an existing metadata field lock returns 403 without writes. Checked by
  `unlock_child_account_forbidden_without_writes` in `backend/src/routes/metadata.rs`.
- A child account's request to patch catalogue metadata, including a request that only sets or clears a work's or
  manifestation's external identifier, is refused with a 403 status. Checked by `patch_child_account_forbidden` in
  `backend/src/routes/metadata.rs`.
- A child account's request to trigger an enrichment re-run is refused with a 403 status. Not checked by any automated
  test.
- The refusal holds even when the child's credential carries write scope. Each of the five review-handler tests uses a
  write-scoped child credential and compares the canonical records, journal, locks and writeback jobs before and after
  refusal. `patch_child_account_forbidden` also uses write scope; the enrichment-trigger request has no automated
  refusal test.
- A non-administrator adult succeeds at the same accept, reject, revert, lock and unlock requests, with the expected
  state changes. Each of the five review-handler tests includes this control. The patch and enrichment-trigger tests
  cover administrator success but have no non-administrator adult control.
- A child account's read of a book's detail returns an empty pending list and no pending versions, while an adult's read
  of the same book returns them. Checked by `detail_endpoint_omits_pending_versions_for_child_caller` in
  `backend/src/routes/library/tests.rs`. That read is outside this obligation, which governs metadata mutation and
  enrichment-trigger requests.
