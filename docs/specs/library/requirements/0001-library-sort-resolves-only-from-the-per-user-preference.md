---
type: REQ
profile-version: 1
id: "REV-REQ-0001"
title: "Library sort resolves only from the per-user preference"
governed-by:
  - "REV-ADR-0001"
---

# Library sort resolves only from the per-user preference

## Statement

The library surface MUST resolve the active sort stack from the reader's stored `sort_stack` preference alone, falling
back to the installation default when that preference is unset. A component on the library route MUST NOT read or
write a `?sort=` URL parameter, and every sort gesture MUST persist its result as the reader's `sort_stack` preference.
WHEN the books list request is built, the client MUST send the reader's non-empty override explicitly as the `sort`
query parameter and MUST NOT send a `sort` parameter when the reader has no override, so the installation default is
never serialised onto the wire.

## Rationale

A URL-carried sort and an account-level sort are two sources of truth for one value, and two sources let surfaces
disagree on the active order, let a clearing gesture write an absent value over an absent value, and give the route
loader a cache key that cannot match a reader with a stored override. Binding resolution to the one preference removes
the second source by construction. Sending the installation default explicitly would reintroduce the mismatch, because
an inheriting reader's list-query key would then depend on server state the route loader cannot see. The decision and
its drivers are recorded in
[Library sort is a per-user preference, resolved client-side, never URL state](../../../../adr/2026-08-08-library-sort-per-user-preference.md).

## Acceptance criteria

- No component under the library route reads a `?sort=` URL parameter to resolve the active sort stack.
- No component under the library route writes a `?sort=` URL parameter.
- A stale `?sort=` value present in the URL (for example from an old bookmark) does not change the resolved sort stack;
  the stored preference, or the installation default when the reader has no override, still applies.
- A table header click and every action in the sort stack editor (add a level, remove a level, reorder, flip direction,
  reset) reach the wire as a `sort_stack` write on `/auth/me/preferences`, and no other write path for the sort stack
  exists.
- Removing the last level of an inherited descending stack, or pressing the sort section's reset control, sends
  `sort_stack: null` and the display transitions to the installation order; it never becomes unsorted.
- WHEN the reader has a non-empty `sort_stack` override, the books list request (`GET /api/v1/books`) carries that stack
  as its `sort` query parameter.
- WHEN the reader has no override, the books list request carries no `sort` parameter.

## More information

- [Multi-column sort stack on the keyset list contract](../../../../adr/2026-07-07-multi-column-sort-stack.md): the
  wire grammar and whitelist the resolved stack is expressed in.
- Satisfied by [Library filter and sort state](../design/0001-library-filter-and-sort-state.md).
