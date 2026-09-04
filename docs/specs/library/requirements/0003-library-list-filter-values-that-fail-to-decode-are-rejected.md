---
type: REQ
profile-version: 1
id: "REV-REQ-0003"
title: "Library list filter values that fail to decode are rejected"
---

# Library list filter values that fail to decode are rejected

## Statement

WHEN a request to `GET /api/v1/books` carries a filter query parameter whose value does not decode into the typed
column condition its suffix declares (an ill-formed UUID on an id-valued parameter, a non-integer on an integer-valued
parameter, or a value that is not a `YYYY-MM-DD` calendar date on a date-valued parameter), the server MUST reject the
request with `400 Bad Request` and MUST NOT execute the list query with that parameter silently dropped or coerced to
a default.

## Rationale

The typed filter grammar closes the column-name surface to a fixed set of suffix parameters so that no client input
ever names a raw SQL identifier; the corresponding obligation on the value side is that a value the grammar cannot
type-check is refused rather than absorbed. Silently dropping a condition that is not parseable would let a client believe a
narrowing filter is active when the server applied none, returning a broader result set than the request asked for
without any signal that anything was wrong. Rejecting the request instead makes the mismatch visible at the point it
occurs.

## Acceptance criteria

- A request to `GET /api/v1/books` with an id-valued filter parameter (for example `author`, `series`, or `shelf`) set
  to a value that is not a valid UUID returns `400 Bad Request`, not `200 OK` with that condition omitted.
- A request with an integer-valued filter parameter (`pages_gte`, `pages_lte`, `rating_gte`, `rating_lte`) set to a
  non-integer value returns `400 Bad Request`.
- A request with a date-valued filter parameter (`created_at_gte`, `created_at_lte`) set to a value that is not a
  `YYYY-MM-DD` calendar date returns `400 Bad Request`.
- A request with a boolean-valued filter parameter (for example `pages_empty`, `subtitle_empty`) set to a value that is
  not a recognised boolean literal returns `400 Bad Request`.
- The response body for each rejection above is a Problem Details document, matching the documented `400` response for
  `GET /api/v1/books` in the OpenAPI contract.

## More information

- [Typed filter grammar on list endpoints](../../../../adr/2026-07-07-typed-filter-grammar-list-endpoints.md): the
  suffix-operator grammar and closed column set this obligation guards the value side of.
- A filter value that decodes successfully but violates a semantic bound (an over-cap value list, over-long text, an
  out-of-range rating, a negative page bound, or an unrecognised status token) is a distinct failure class, rejected
  with `422 Unprocessable Entity` rather than `400`; that class is not part of this obligation.
- Satisfied by [Library filter and sort state](../design/0001-library-filter-and-sort-state.md).
