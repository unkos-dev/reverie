---
type: REQ
profile-version: 1
id: "REV-REQ-0003"
title: "Library list range filters apply their bounds as declared"
governed-by:
  - "REV-ADR-0066"
  - "REV-ADR-0038"
---

# Library list range filters apply their bounds as declared

## Statement

WHEN a request to `GET /api/v1/books` carries a numeric or timestamp range filter, the server MUST include a value equal
to a `_gte` or `_lte` bound, MUST exclude a value equal to a `_lt` bound, and MUST compare timestamp bounds as instants
rather than as the text the request wrote them in.

## Rationale

A range filter that is wrong at its boundary shows or hides exactly the books on that boundary, such as a book added at
the first instant of a chosen day, and nothing on the page signals the mistake. Comparing instants rather than text
keeps a page walk valid when a client writes the same instant with a different offset.

## Acceptance criteria

- Complete inverted numeric or timestamp bounds return a successful empty page. Equal numeric bounds match that exact
  value; equal timestamp bounds form an empty half-open interval. Checked by
  `filter_numeric_equal_bounds_match_and_inverted_bounds_are_empty` and
  `filter_created_at_inclusive_lower_exclusive_upper_and_cursor_identity` in `backend/src/routes/library/tests.rs`.
- Timestamp predicates include their lower instant and exclude their upper instant. Equivalent offsets preserve cursor
  identity; changing either instant invalidates the cursor. Checked by
  `filter_created_at_inclusive_lower_exclusive_upper_and_cursor_identity` in `backend/src/routes/library/tests.rs`.
