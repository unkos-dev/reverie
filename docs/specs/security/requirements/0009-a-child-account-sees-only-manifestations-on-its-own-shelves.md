---
type: REQ
profile-version: 1
id: "REV-REQ-0009"
title: "A child account sees only manifestations on its own shelves"
governed-by:
  - "REV-ADR-0028"
---

# A child account sees only manifestations on its own shelves

## Statement

WHEN the caller of a database transaction is an account whose role is `child`, a query against `manifestations` MUST
return a row only if one of that account's own shelves holds it.

## Rationale

A child account's access to the library is limited to what an adult or administrator has put on that child's shelves,
not the whole shared collection. Enforcing the limit on the rows themselves, rather than trusting each caller to filter
a response afterwards, keeps it in force where an application bug, a new endpoint or another access path might forget to
apply it.

## Acceptance criteria

- A manifestation held on a shelf owned by a given `child`-role account is returned by a `SELECT` against
  `manifestations` run under that account's context.
- A manifestation not held on any shelf that account owns is not returned by the same query, even when no other
  condition hides it from an `adult`- or `admin`-role caller.
- A manifestation held only on another account's shelf is not returned to the `child`-role account.
- A `child`-role account whose shelves hold nothing receives zero rows from a `SELECT` against `manifestations`.
