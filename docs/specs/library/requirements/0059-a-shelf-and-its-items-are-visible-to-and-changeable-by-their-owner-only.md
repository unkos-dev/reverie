---
type: REQ
profile-version: 1
id: "REV-REQ-0059"
title: "A shelf and its items are visible to and changeable by their owner only"
governed-by:
  - "REV-ADR-0028"
---

# A shelf and its items are visible to and changeable by their owner only

## Statement

WHEN a request reads, renames, deletes, or changes the items of a shelf, or filters the books list by a shelf, the
server MUST act only on a shelf the authenticated account owns; a shelf owned by another account MUST be treated as not
found, so the request neither reveals that the shelf exists nor changes it, and a books-list filter naming another
account's shelf MUST match no book.

## Rationale

A shelf is a personal arrangement of books, and the account that made it relies on no other account seeing or altering
it, including a child account on the same instance whose reading is scoped away from the rest of the catalogue. The
shelf tables are not protected by the database's row-level policies, so the ownership check in the request path is the
only thing standing between one account's shelves and the rest; if it were missing on a single route, that route would
leak or alter another account's arrangement with nothing else to catch it. The
[authorization axes decision](../../../adr/0028-api-authorization-orthogonal-scope-role-and-ownership-axes.md) names
resource ownership as an axis every mutating route enforces server-side.

## Acceptance criteria

- Listing shelves returns only the caller's shelves. Checked by `list_shelves_returns_only_callers_shelves` in
  `backend/src/routes/shelves/tests.rs`.
- Renaming or deleting another account's shelf answers `404` and leaves the shelf unchanged. Checked by
  `rename_other_users_shelf_returns_404` and `delete_other_users_shelf_returns_404` in
  `backend/src/routes/shelves/tests.rs`.
- Reading, adding to, removing from, or reordering the items of another account's shelf answers `404` and changes
  nothing. Satisfaction is determined by reading each handler's ownership-bound lookup, which precedes every write; no
  automated check exercises these routes against another account's shelf.
- Filtering the books list by a shelf id returns only books on that shelf when the caller owns it, and no books
  otherwise. Checked by `list_filter_by_shelf_scoped_to_caller` in `backend/src/routes/library/tests.rs`.
- A child account reads its own shelves. Checked by `child_can_view_own_shelves` in
  `backend/src/routes/shelves/tests.rs`.
