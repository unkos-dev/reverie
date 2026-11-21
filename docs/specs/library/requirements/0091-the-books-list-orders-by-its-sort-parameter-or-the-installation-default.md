---
type: REQ
profile-version: 1
id: "REV-REQ-0091"
title: "The books list orders by its sort parameter or the installation default"
governed-by:
  - "REV-ADR-0037"
---

# The books list orders by its sort parameter or the installation default

## Statement

WHEN a request to `GET /api/v1/books` is accepted, the server MUST order the list by the sort stack in the request's
`sort` query parameter, or by the
[installation default](0001-library-sort-resolves-only-from-the-per-user-preference.md#statement) when the request
carries no `sort` parameter, whatever sort override the caller has stored.

## Rationale

Clients of the books list other than the Library, such as scripts that use a device token, choose their own order
through the `sort` parameter. If the server ordered their requests by an account's stored override, the same request
would return a different order for different accounts, and a cursor walk could change order between pages after the
account's override changed.

## Acceptance criteria

- `GET /api/v1/books?sort=title` returns books in ascending title order.
- `GET /api/v1/books?sort=-title,created_at` returns books in descending title order, with books of equal title in
  ascending order of creation.
- `GET /api/v1/books` without `sort` returns books in the installation default order, newest first by creation time.
- A caller whose stored sort override is title order receives `GET /api/v1/books` without `sort` in the installation
  default order.
