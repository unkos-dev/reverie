---
type: REQ
profile-version: 1
id: "REV-REQ-0003"
title: "A REST books list range filter applies its bounds as declared"
governed-by:
  - "REV-ADR-0066"
  - "REV-ADR-0038"
---

# A REST books list range filter applies its bounds as declared

## Statement

WHEN a request to `GET /api/v1/books` carries a range filter, which is one of the `pages_gte`, `pages_lte`,
`rating_gte`, `rating_lte`, `created_at_gte` and `created_at_lt` parameters, the server MUST include a book whose value
equals a `_gte` or `_lte` bound, MUST exclude a book whose value equals a `_lt` bound, and MUST compare `created_at`
bounds as instants rather than as the text the request wrote them in.

## Rationale

A range filter that is wrong at its boundary shows or hides exactly the books on that boundary, such as a book added at
the first instant of a chosen day, and nothing on the page signals the mistake. Comparing instants rather than text
keeps a page walk valid when a client writes the same instant with a different offset.

## Acceptance criteria

- Equal numeric bounds match that exact value, and inverted numeric bounds return a successful empty page. Checked by
  `filter_numeric_equal_bounds_match_and_inverted_bounds_are_empty` in `backend/src/routes/library/tests.rs`.
- A `created_at` filter includes a book created at its lower instant and excludes one created at its upper instant.
  Equal or inverted `created_at` bounds return a successful empty page. Checked by
  `filter_created_at_inclusive_lower_exclusive_upper_and_cursor_identity` in `backend/src/routes/library/tests.rs`.
- `created_at` bounds that name the same instants with a different UTC offset continue the same cursor walk, while a
  bound that names a different instant does not. Checked by
  `filter_created_at_inclusive_lower_exclusive_upper_and_cursor_identity`.
