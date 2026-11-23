---
type: REQ
profile-version: 1
id: "REV-REQ-0057"
title: "A REST list cursor is honoured only by the endpoint, sort and filter set that minted it"
governed-by:
  - "REV-ADR-0019"
---

# A REST list cursor is honoured only by the endpoint, sort and filter set that minted it

## Statement

WHEN a request to a paginated JSON list endpoint under `/api/v1/` presents a cursor, the server MUST resume the page
walk only if that same endpoint minted the cursor under the request's sort stack and filter set, and MUST otherwise
refuse the request with `422 Unprocessable Content` without reinterpreting the cursor against the request's own sort or
filters. An endpoint is one list together with its path parameters, so the items of one shelf and the items of another
are different endpoints. A sort stack is the ordered list of `?sort=` columns with their directions, and two stacks are
the same only when they name the same columns in the same order with the same directions. A filter set is the request's
filter parameters with their values, and two filter sets that differ only in the order of a multi-valued parameter's
values are the same filter set. The refusal covers a cursor minted by a different `/api/v1/` list endpoint or by a
GraphQL connection.

## Rationale

A client that walks pages holds an opaque token whose meaning is fixed by the ordering and the row set it was cut from.
If the server accepted such a token under a different ordering or filter, the resumed page would skip or repeat rows
with nothing to tell the client it had happened, and a token from one list would silently position a walk in another.
The client depends on a refused cursor being loud, so it can restart the walk from the first page rather than render a
page that looks complete and is not. Refusing a GraphQL cursor as well keeps each API's cursor format private to that
API, so neither format becomes a contract the other API has to keep.

## Acceptance criteria

- A books-list cursor presented with a `?sort=` stack whose columns, order or directions differ from the stack it was
  minted under answers `422`. Checked by `list_endpoint_cross_sort_cursor_rejected` and
  `list_endpoint_cursor_direction_mismatch_returns_422` in `backend/src/routes/library/tests.rs`, and by
  `rejects_spec_order_mismatch` in `backend/src/routes/cursor.rs`.
- A books-list cursor presented with a filter set that differs from the one it was minted under answers `422`, while a
  filter set that differs only in the order of a multi-valued parameter's values continues the walk. Checked by
  `filter_cursor_rejects_changed_filter_and_ignores_value_order` in `backend/src/routes/library/tests.rs`.
- A cursor minted by one `/api/v1/` list endpoint and presented to another answers `422`, on each of the books list, the
  shelf list and the shelf item list. Checked by `list_endpoint_legacy_cursor_tag_returns_422` in
  `backend/src/routes/library/tests.rs`, `shelf_items_rejects_cross_endpoint_cursor_replay` in
  `backend/src/routes/shelves/tests.rs`, and `shelf_rejects_foreign_tags` in `backend/src/routes/cursor.rs`.
- A shelf item cursor minted for one shelf continues that shelf's walk, and presented for another shelf of the same
  owner answers `422`. Checked by `shelf_items_rejects_a_cursor_minted_for_another_shelf` in
  `backend/src/routes/shelves/tests.rs`.
- A cursor minted by the GraphQL `books` connection and presented to `GET /api/v1/books` under the same sort and filters
  answers `422`, and so does a GraphQL `shelves` cursor presented to `GET /api/v1/shelves`. Checked by
  `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- A refused cursor produces no page: the `422` response is a Problem Details document that carries no `items`. Checked
  by the same tests, each of which asserts the `422` status.
