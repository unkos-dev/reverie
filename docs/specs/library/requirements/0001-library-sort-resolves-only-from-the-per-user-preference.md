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

The Library MUST order every view by one sort stack per account: the reader's stored sort preference, or the
installation default when the reader has none. A sort stack is an ordered list of sort columns, each with a direction,
and the installation default is the stack that applies to every account without a stored sort preference. A sort change
or reset in any view MUST become the reader's stored preference, and a `sort` value in a Library URL MUST NOT change the
order. Outside the Library, `GET /api/v1/books` MUST order its list by the stack in its `sort` query parameter when the
request carries one, and by the installation default when it does not.

## Rationale

A sort carried in the URL and a sort stored on the account are two sources of truth for one value. With two sources,
views disagree on the active order, and an old bookmark reorders a reader's Library without the reader asking. One
account-level stack removes the second source. Non-browser clients of the books list choose their own order through its
`sort` parameter, so that contract stays independent of the Library's preference.

## Acceptance criteria

- Covers and Table order the same books by the same sort stack.
- A table header click, the Covers sort control and every action in the sort stack editor (add a level, remove a level,
  reorder, flip direction, reset) change the reader's stored sort preference, and the new order applies in every view.
- Removing the last level of the reader's stack, or pressing the sort section's reset control, removes the reader's
  stored sort preference. The Library then shows the installation default order and is never unsorted.
- Opening the Library from a URL that carries a `sort` value, for example an old bookmark, leaves the order at the
  reader's stored preference, or at the installation default when the reader has none.
- No Library action adds or changes a `sort` parameter in the browser URL.
- `GET /api/v1/books?sort=title` returns books in ascending title order, and `GET /api/v1/books` without `sort` returns
  them in the installation default order.

## More information

- [Multi-column sort stack on the keyset list contract](../../../adr/0037-multi-column-sort-stack-on-the-keyset-list-contract.md):
  the wire grammar and whitelist the resolved stack is expressed in.
