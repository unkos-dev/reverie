---
type: REQ
profile-version: 1
id: "REV-REQ-0097"
title: "A child account cannot create, rename or delete a shelf"
governed-by:
  - "REV-ADR-0056"
---

# A child account cannot create, rename or delete a shelf

## Statement

WHEN the caller is a child account, which is an account whose role is `child`, the system MUST refuse any request to
create, rename or delete a shelf, whatever scope the caller's credential carries, and MUST leave the caller's shelves
unchanged.

## Rationale

A child account sees only the books on its own shelves
([REV-REQ-0009](../../security/requirements/0009-a-child-account-sees-only-manifestations-on-its-own-shelves.md)), so
which shelves a child has, and what they are called, is how an adult in the household arranges that child's library. The
child can still add and remove books on the shelves it owns. A child credential carries write scope for those changes,
so scope alone cannot keep a child from changing the shelves themselves.

## Acceptance criteria

- A child's `POST /api/v1/shelves` answers `403` and the child owns no new shelf afterwards. Checked by
  `child_cannot_create_shelf` in `backend/src/routes/shelves/tests.rs`.
- A child's `PATCH /api/v1/shelves/{id}` on a shelf it owns answers `403` and the shelf keeps its name. Checked by
  `child_cannot_rename_shelf` in `backend/src/routes/shelves/tests.rs`.
- A child's `DELETE /api/v1/shelves/{id}` on a shelf it owns answers `403` and the shelf still exists. Checked by
  `child_cannot_delete_shelf` in `backend/src/routes/shelves/tests.rs`.
- A child's GraphQL `createShelf`, `renameShelf` and `deleteShelf` each produce an error with the `forbidden` code,
  while an adult administrator's write-scoped credential is admitted to each. Checked by `graphql_request_boundary` in
  `backend/src/graphql/mod.rs`.
- The refusals hold for a credential that carries write scope: every child request above uses HTTP Basic credentials,
  which carry the account's role-derived write scope.
