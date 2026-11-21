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
request's own sort or filters. A JSON list endpoint answers the refusal with `422 Unprocessable Content`. A GraphQL
connection answers it with an error whose code is `invalid-cursor`, a code the server gives no other refusal. For a JSON
list endpoint, a cursor is incompatible unless that endpoint minted it under the request's sort stack and filter set.
For a GraphQL connection, a cursor is incompatible unless that connection minted it under the request's parent, sort
stack and filter set. Two sort stacks are the same only when they name the same columns in the same order with the same
directions. Two filter sets that differ only in the order of a multi-valued filter's values are the same filter set. The
OPDS catalogue feeds are outside this obligation.

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

- A books-list cursor presented with a `?sort=` stack whose columns, order or directions differ from the stack it was
  minted under answers `422`. Checked by `list_endpoint_cross_sort_cursor_rejected` and
  `list_endpoint_cursor_direction_mismatch_returns_422` in `backend/src/routes/library/tests.rs`, and by
  `rejects_spec_order_mismatch` in `backend/src/routes/cursor.rs`.
- A books-list cursor presented with a filter set that differs from the one it was minted under answers `422`, while a
  filter set that differs only in the order of a multi-valued parameter's values is the same set. Checked by
  `filter_cursor_rejects_changed_filter_and_ignores_value_order` in `backend/src/routes/library/tests.rs`.
- A cursor minted by one `/api/v1/` list endpoint and presented to another answers `422`, on each of the books list, the
  shelf list and the shelf item list. Checked by `list_endpoint_legacy_cursor_tag_returns_422` in
  `backend/src/routes/library/tests.rs`, `shelf_items_rejects_cross_endpoint_cursor_replay` in
  `backend/src/routes/shelves/tests.rs`, and `shelf_rejects_foreign_tags` in `backend/src/routes/cursor.rs`.
- A refused cursor never produces a page: the response carries no items. Checked by the same tests, each of which
  asserts the `422` status.
- A GraphQL cursor presented to another connection, under another parent, or with a different sort column, level order,
  direction or filter set produces an error with code `invalid-cursor` at the refused connection's path, and that
  connection returns no page. This holds for a shelves cursor sent to books and a books cursor sent to shelves, for
  nested work, shelf and pending-version connections, and for a cursor whose key type the new sort accepts. Checked by
  `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- A GraphQL books cursor presented with the values of a multi-valued filter in another order returns the next page.
  Checked by `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- A GraphQL books request refused for an invalid sort, rather than for its cursor, carries the code `validation`, not
  `invalid-cursor`. Checked by `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
