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
proposal, patch catalogue metadata, set or clear an external identifier, or trigger an enrichment run, whatever scope
the caller's credential carries.

## Rationale

A child account's own credential can carry write scope, since a child manages its own settings and shelves, so scope
alone cannot be what keeps a child from altering metadata the whole household's catalogue shares. Gating on the caller's
identity, independent of scope, protects that shared catalogue from a child's credential regardless of what capability
it happens to carry, and stops a wrongly scoped or compromised child token from corrupting data every other account
relies on.

## Acceptance criteria

- A child account's request to accept a pending metadata proposal is refused with a 403 status. Not checked by any
  automated test.
- A child account's request to reject a pending metadata proposal is refused with a 403 status. Not checked by any
  automated test.
- A child account's request to revert a field to a prior version or to its cleared state is refused with a 403 status.
  Not checked by any automated test.
- A child account's request to patch catalogue metadata, including a request that only sets or clears a work's or
  manifestation's external identifier, is refused with a 403 status. Checked by `patch_child_account_forbidden` in
  `backend/src/routes/metadata.rs`.
- A child account's request to trigger an enrichment re-run is refused with a 403 status. Not checked by any automated
  test.
- The refusal holds even when the child's credential carries write scope, the same scope an adult account presents to
  succeed at the same request: `patch_child_account_forbidden`'s child credential carries write scope and is still
  refused. No automated test makes the same comparison for the accept, reject, revert or enrichment-trigger requests.
- A non-child adult account succeeds at each of these five requests (the boundary). Not checked by any automated test
  against a non-administrator adult account: every passing test for these requests authenticates as an administrator.
- A child account's read of a book's detail returns an empty pending list and no pending versions, while an adult's read
  of the same book returns them. Checked by `detail_endpoint_omits_pending_versions_for_child_caller` in
  `backend/src/routes/library/tests.rs`. That read is outside this obligation, which governs only the five write and
  enrichment-trigger requests above.
