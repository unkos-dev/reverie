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

WHEN a request to a paginated JSON list endpoint under `/api/v1/` presents a cursor, the server MUST resume the page
walk only if the cursor was minted by that same endpoint under the same sort stack and the same filter set as the
request presents; a cursor minted by a different list endpoint, or under a different sort stack or filter set, MUST be
refused with `422 Unprocessable Content` and MUST NOT be reinterpreted against the request's own sort or filter. The
OPDS catalogue feeds are outside this obligation.

## Rationale

A client that walks pages holds an opaque token whose meaning is fixed by the ordering and the row set it was cut from.
If the server accepted such a token under a different ordering or filter, the resumed page would skip or repeat rows
with nothing to tell the client it had happened, and a token from one list would silently position a walk in another.
The client depends on a refused cursor being loud, so it can restart the walk from the first page rather than render a
page that looks complete and is not. The
[keyset pagination decision](../../../adr/0019-keyset-pagination-as-the-default-list-contract.md) fixes the opaque
cursor as the list contract this obligation protects. The OPDS acquisition feeds sit outside the obligation because
every one of them walks the same ordering and bound, so a cursor replayed across feeds starts the walk mid-list rather
than skipping or repeating rows within it.

## Acceptance criteria

- A books-list cursor presented with a `?sort=` stack whose columns, order or directions differ from the stack it was
  minted under answers `422`. Checked by `list_endpoint_cross_sort_cursor_rejected` and
  `list_endpoint_cursor_direction_mismatch_returns_422` in `backend/src/routes/library/tests.rs`, and by
  `rejects_spec_direction_mismatch`, `rejects_spec_order_mismatch` and `rejects_spec_column_mismatch` in
  `backend/src/routes/cursor.rs`.
- A books-list cursor presented with a filter set that differs from the one it was minted under answers `422`, while a
  filter set that differs only in the order of a multi-valued parameter's values is the same set. Checked by
  `filter_cursor_rejects_changed_filter_and_ignores_value_order` in `backend/src/routes/library/tests.rs` and
  `rejects_filter_fingerprint_mismatch` in `backend/src/routes/cursor.rs`.
- A cursor minted by one `/api/v1/` list endpoint and presented to another answers `422`, on each of the books list, the
  shelf list and the shelf item list. Checked by `list_endpoint_legacy_cursor_tag_returns_422` in
  `backend/src/routes/library/tests.rs`, `shelf_items_rejects_cross_endpoint_cursor_replay` in
  `backend/src/routes/shelves/tests.rs`, and `rejects_unknown_tag`, `shelf_rejects_foreign_tags` and
  `shelf_item_rejects_garbage` in `backend/src/routes/cursor.rs`.
- A refused cursor never produces a page: the response carries no items. Checked by the same tests, each of which
  asserts the `422` status.
