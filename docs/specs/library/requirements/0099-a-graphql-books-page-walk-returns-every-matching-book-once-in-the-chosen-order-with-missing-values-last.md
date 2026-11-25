---
type: REQ
profile-version: 1
id: "REV-REQ-0099"
title: "A GraphQL books page walk returns every matching book once in the chosen order with missing values last"
governed-by:
  - "REV-ADR-0037"
---

# A GraphQL books page walk returns every matching book once in the chosen order with missing values last

## Statement

WHEN a client walks the GraphQL `books` connection from the first page to the last under any valid `sort` stack and
`filter` set, the server MUST return each matching book once, in the chosen column and direction order, with books that
have no value for a sort column appearing after every book that has a value in both ascending and descending directions.
For `SERIES`, the server MUST order books by their displayed series name and then by their number within that series in
the chosen direction, placing unnumbered books in a series after numbered books in that same series and books in no
series last.

## Rationale

Readers page through large libraries sorted by any displayed column. Skipping or repeating a book across page boundaries
hides books or corrupts selection, and placing missing values first buries the books a reader sorted to find. Ordering a
series by name and then by number within the series keeps instalments grouped and readable in either direction.

## Acceptance criteria

- Walking the `books` connection page by page to completion under a supported sort column in ascending and descending
  order returns the same ordered list as a single-page read, with each matching book appearing once and
  `hasPreviousPage` false on the first page and true on every following page. Checked for `SUBTITLE`, `SERIES`,
  `ISBN_13`, `STATUS` and `RATING` by `graphql_books_page_walk_ordering` in `backend/src/graphql/mod.rs`.
- In both ascending and descending order on a column where some matching books have no value, every book without a value
  appears after every book with a value. Checked by `graphql_books_page_walk_ordering`.
- Under `SERIES` ascending, books in a series appear in ascending series-name order and ascending numeric order within
  each series, followed by unnumbered books in that series, with books in no series last; under `SERIES` descending,
  books appear in descending series-name order and descending numeric order within each series, followed by unnumbered
  books in that series, with books in no series last. Checked by `graphql_books_page_walk_ordering`.
