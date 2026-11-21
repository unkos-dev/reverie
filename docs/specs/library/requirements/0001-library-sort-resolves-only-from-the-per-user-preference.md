---
type: REQ
profile-version: 1
id: "REV-REQ-0001"
title: "Library sort resolves only from the per-user preference"
governed-by:
  - "REV-ADR-0047"
---

# Library sort resolves only from the per-user preference

## Statement

The Library MUST order every view by one sort stack per account: the reader's stored sort override, or the installation
default when the reader has none. A sort stack is an ordered list of sort columns, each with a direction, and the
installation default is the stack that applies to every account without a stored sort override. A sort change in any
view MUST become the reader's stored override, a sort reset MUST clear that override, and a `sort` value in a Library
URL MUST NOT change the order.

## Rationale

A sort carried in the URL and a sort stored on the account are two sources of truth for one value. With two sources,
views disagree on the active order, and an old bookmark reorders a reader's Library without the reader asking. One
account-level stack removes the second source.

## Acceptance criteria

- Covers and Table order the same books by the same sort stack.
- A table header click stores the new sort as the reader's override and adds no `sort` parameter to the browser URL.
  Checked by `table header click persists the sort to the account and never touches the URL` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- Each action in the sort stack editor (add a level, remove a level, reorder, flip direction) produces the edited stack.
  Checked by `adding a field appends a level to the effective stack` and
  `direction toggle, reorder, and remove dispatch the edited stack` in `frontend/tests/browser/FilterRail.test.tsx`.
- The Covers sort control and each sort stack editor action store their result as the reader's override.
- Pressing the sort section's reset control clears the reader's override, and the Library then shows the installation
  default order. Checked by `the rail's reset drops the override and returns to the installation order` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- Removing the last level of the reader's stack is a reset, not an unsorted state: the editor then shows the
  installation default. Checked by `removing the last level is the reset gesture, not an unsorted state` in
  `frontend/tests/browser/FilterRail.test.tsx`.
- Opening the Library from a URL that carries a `sort` value, for example an old bookmark, leaves the order at the
  reader's stored override, or at the installation default when the reader has none. Checked by
  `a stale URL ?sort= is inert: $resolved still applies` in `frontend/tests/browser/LibraryPage.test.tsx`.
- The Covers sort control, the sort stack editor and the reset control add no `sort` parameter to the browser URL.

## More information

- [Multi-column sort stack on the keyset list contract](../../../adr/0037-multi-column-sort-stack-on-the-keyset-list-contract.md):
  the wire grammar and whitelist the resolved stack is expressed in.
