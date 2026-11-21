---
type: REQ
profile-version: 1
id: "REV-REQ-0057"
title: "A list cursor is honoured only under the sort and filter set that minted it"
governed-by:
  - "REV-ADR-0019"
---

# A list cursor is honoured only under the sort and filter set that minted it

## Statement

WHEN a request to a paginated JSON list endpoint under `/api/v1/`, or to a GraphQL connection, presents an incompatible
cursor, the server MUST refuse the request without returning a page and MUST NOT reinterpret the cursor against the
request's own sort or filters. A cursor is compatible only when the server minted it for the same logical list under the
request's sort stack and filter set. A logical list is the sequence a cursor walks: the books list, the shelf list, one
shelf's items, or one GraphQL connection under one parent, such as a work's manifestations or a revision's authors. The
HTTP books list and the GraphQL `books` connection are one logical list with one cursor format, and so are the HTTP
shelf list and the GraphQL `shelves` connection, so a cursor minted through either root continues the other. The HTTP
shelf item cursor carries no shelf, so over HTTP the items of every shelf form one logical list. Two sort stacks are the
same only when they name the same columns in the same order with the same directions. Two filter sets that differ only
in the order of a multi-valued filter's values are the same filter set. A JSON list endpoint answers the refusal with
`422 Unprocessable Content`. A GraphQL connection answers it with an error whose code is `invalid-cursor`, a code the
server gives no other refusal. The OPDS catalogue feeds are outside this obligation.

## Rationale

A client that walks pages holds an opaque token whose meaning is fixed by the ordering and the row set it was cut from.
If the server accepted such a token under a different ordering or filter, the resumed page would skip or repeat rows
with nothing to tell the client it had happened, and a token from one list would silently position a walk in another.
The client depends on a refused cursor being loud, so it can restart the walk from the first page rather than render a
page that looks complete and is not. A code reserved for cursor refusals lets the client restart only for this refusal,
and leave other invalid input to the reader. The OPDS acquisition feeds sit outside the obligation because every one of
them walks the same ordering and bound, so a cursor replayed across feeds starts the walk mid-list rather than skipping
or repeating rows within it.

## Acceptance criteria

- A books-list cursor presented to `GET /api/v1/books` with a `?sort=` stack whose columns, level order or directions
  differ from the stack it was minted under answers `422` with no items. Checked by
  `list_endpoint_cross_sort_cursor_rejected` and `list_endpoint_cursor_direction_mismatch_returns_422` in
  `backend/src/routes/library/tests.rs`.
- A books-list cursor presented with a filter set that differs from the one it was minted under answers `422` with no
  items, while a filter set that differs only in the order of a multi-valued parameter's values is the same set. Checked
  by `filter_cursor_rejects_changed_filter_and_ignores_value_order` in `backend/src/routes/library/tests.rs`.
- A cursor from another logical list, presented to the books list, the shelf list or a shelf's item list over HTTP,
  answers `422` with no items. Checked by `list_endpoint_foreign_list_cursor_returns_422` in
  `backend/src/routes/library/tests.rs` and `shelf_lists_reject_foreign_list_cursors` in
  `backend/src/routes/shelves/tests.rs`.
- A cursor minted by the GraphQL `books` connection continues `GET /api/v1/books` under the same sort and filters, and a
  cursor minted by the GraphQL `shelves` connection continues `GET /api/v1/shelves`. Checked by
  `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- A GraphQL cursor presented to another connection, under another parent, or with a different sort column, level order,
  direction or filter set produces an error with code `invalid-cursor` at the refused connection's path, and that
  connection returns no page. This holds for a shelves cursor sent to books and a books cursor sent to shelves, for
  nested work, shelf, metadata revision and pending-version connections, and for a cursor whose key type the new sort
  accepts. Checked by `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- A GraphQL books cursor presented with the values of a multi-valued filter in another order returns the next page.
  Checked by `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- A GraphQL books request refused for an invalid sort or page size, rather than for its cursor, carries the code
  `validation`, not `invalid-cursor`. Checked by `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- The Library restarts a walk from the first page once when a continuation is refused with `invalid-cursor`, whatever
  the error message, and does not restart for a `validation` refusal. Checked by
  `unchanged returns preserve each visit's books and known-book position` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
