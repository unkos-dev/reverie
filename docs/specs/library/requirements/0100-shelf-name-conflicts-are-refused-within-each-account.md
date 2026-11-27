---
type: REQ
profile-version: 1
id: "REV-REQ-0100"
title: "Shelf name conflicts are refused within each account"
---

# Shelf name conflicts are refused within each account

## Statement

WHEN a caller creates or renames a shelf, the server MUST refuse a name whose trimmed, case-insensitive, accent-folded
value equals another shelf name owned by that account, including system shelves and concurrent writes. Different
accounts MUST be able to use the same name. A duplicate-name refusal MUST identify the conflict through the calling
transport and the existing browser form with "You already have a shelf with this name."

## Rationale

Readers distinguish their shelves by name. Duplicate names make the intended destination unclear, and a retry after a
failed acknowledgement can otherwise create another indistinguishable shelf.

## Acceptance criteria

- Creating `Café`, then creating or renaming another shelf to `CAFE` or ` Café `, refuses the second name and leaves
  stored shelves unchanged. Renaming the same shelf without changing its folded identity succeeds. Checked by
  `shelf_names_are_unique_per_account` in `backend/src/routes/shelves/tests.rs`.
- Concurrent create/create, rename/rename and create/rename attempts for one account and folded name leave exactly one
  winner and no duplicate name. Checked by `shelf_names_are_unique_per_account` in
  `backend/src/routes/shelves/tests.rs`.
- Different accounts can use the same folded name. User and system shelves conflict within one account regardless of
  which shelf kind holds the name first. Checked by `shelf_names_are_unique_per_account` in
  `backend/src/routes/shelves/tests.rs`.
- Duplicate REST creates and renames return HTTP 409 Problem Details with type `shelf-name-taken` and detail "You
  already have a shelf with this name." Checked by `shelf_names_are_unique_per_account` in
  `backend/src/routes/shelves/tests.rs`.
- Duplicate GraphQL creates and renames return `ShelfNameTaken` with the same message and no execution errors. Checked
  by `shelf_name_conflicts_are_typed` in `backend/src/graphql/mod.rs`.
- The browser's create and rename dialogs display that message, retain the attempted name and stay open after the
  refusal. Save stops being pending and the refusal does not replay the write. Checked by
  `shelf_name_conflicts_retain_input` in `frontend/tests/browser/ShelvesListPage.test.tsx`.
