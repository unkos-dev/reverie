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

The Library MUST resolve its active sort order from the reader's stored per-user sort preference alone, falling back to
the installation default when that preference is unset. The Library MUST ignore any `?sort=` URL query parameter and
MUST NOT write a `?sort=` URL query parameter. Every sort gesture MUST persist its result as the reader's sort
preference, and clearing the last sort level or activating Reset MUST clear the reader's override so the Library returns
to the installation default without ever becoming unsorted.

## Rationale

Carrying sort order in both the URL and account preferences creates two competing sources of truth, allowing views to
disagree on the active order and stale bookmarks to override a reader's chosen preference. Resolving sort strictly from
the per-user preference keeps the displayed order consistent across visits and devices while preserving the installation
default whenever no override is set.

## Acceptance criteria

- A `?sort=` query parameter in the Library URL does not change the resolved sort order; the reader's stored preference,
  or the installation default when unset, still applies. Checked by
  `a stale URL ?sort= is inert: $resolved still applies` in `frontend/tests/browser/LibraryPage.test.tsx`.
- A table header click persists the sort change to the reader's preference and writes no `?sort=` parameter to the URL.
  Checked by `table header click persists the sort to the account and never touches the URL` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- Adding, reordering, flipping or removing levels in the sort editor dispatches the edited stack to the page's sort
  handler. Checked for editor dispatch through a test-local owner by
  `adding a field appends a level to the effective stack` and
  `direction toggle, reorder, and remove dispatch the edited stack` in `frontend/tests/browser/FilterRail.test.tsx`.
- Removing the last level dispatches the reset gesture without producing an unsorted stack, and activating Reset in the
  rail persists the cleared override and returns the Library to the installation default. Checked by
  `removing the last level is the reset gesture, not an unsorted state` in `frontend/tests/browser/FilterRail.test.tsx`
  and `the rail's reset drops the override and returns to the installation order` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- When the reader's stored sort preference arrives after an initial load using the installation default, the Library
  re-sorts to match the stored preference. Checked by `a late preferences response re-sorts the initial list once` in
  `frontend/tests/browser/LibraryPage.test.tsx`.

## More information

- [Multi-column sort stack on the keyset list contract](../../../adr/0037-multi-column-sort-stack-on-the-keyset-list-contract.md):
  the sort vocabulary and stack grammar the resolved order uses.
