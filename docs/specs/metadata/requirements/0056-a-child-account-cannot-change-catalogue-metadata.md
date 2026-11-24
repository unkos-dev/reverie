---
type: REQ
profile-version: 1
id: "REV-REQ-0056"
title: "A child account cannot change catalogue metadata"
governed-by:
  - "REV-ADR-0056"
---

# A child account cannot change catalogue metadata

## Statement

WHEN the caller is a child account, which is an account whose role is `child`, the system MUST refuse any request that
would accept, reject or revert a metadata proposal, set or clear a metadata field lock, patch catalogue metadata, set or
clear an external identifier, or trigger an enrichment run, whatever scope the caller's credential carries. A refused
request MUST leave catalogue metadata, canonical pointers, proposals, field locks, writeback jobs and enrichment state
unchanged.

## Rationale

A child account's own credential can carry write scope, since a child manages its own settings and the books on the
shelves it owns, so scope alone cannot be what keeps a child from altering metadata the whole household's catalogue
shares. Gating on the caller's identity, independent of scope, protects that shared catalogue from a child's credential
regardless of what capability it happens to carry, and stops a wrongly scoped or compromised child token from corrupting
data every other account relies on.

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
- A child account's request to trigger an enrichment re-run answers 403 and leaves the manifestation's enrichment status
  and attempt count unchanged. Checked by `trigger_admin_resets_enrichment_state` in `backend/src/routes/enrichment.rs`.
- A child's GraphQL `updateMetadata`, `refreshMetadata`, `acceptMetadataVersion`, `rejectMetadataVersion`,
  `revertMetadataField`, `lockMetadataField` and `unlockMetadataField` each produce an error with the `forbidden` code.
  Checked by `graphql_request_boundary` in `backend/src/graphql/mod.rs`.
- The refusals hold for a credential that carries write scope: every child request in the tests above uses a child
  credential that carries write scope.
- An adult is not refused by this check. A non-administrator adult succeeds at accept, reject, revert, lock and unlock,
  checked by the five review-handler tests above. An administrator succeeds at a patch and at triggering enrichment,
  checked by `patch_child_account_forbidden` and `trigger_admin_resets_enrichment_state`.
