---
type: REQ
profile-version: 1
id: "REV-REQ-0093"
title: "A GraphQL connection honours a cursor only under the connection, parent, sort and filters that minted it"
governed-by:
  - "REV-ADR-0019"
---

# A GraphQL connection honours a cursor only under the connection, parent, sort and filters that minted it

## Statement

WHEN a request to a GraphQL connection presents a cursor, the server MUST resume the page walk only if that same
connection minted the cursor under the request's parent, sort stack and filter set, and MUST otherwise refuse that
connection at its own field path with an execution error whose `extensions.code` is `invalid-cursor`, a code
[REV-REQ-0088](../../api/requirements/0088-a-graphql-execution-error-carries-a-registered-classification-code.md#statement)
registers, and no page, without reinterpreting the cursor against the request's own sort or filters and without failing
independent sibling connections in the same query beyond standard GraphQL null propagation. A connection's parent is the
object the connection field belongs to, such as the work whose `manifestations` the request pages. A revision's authors
and other relations each have their own list under that revision, including when the owning work or manifestation
exposes the same relation. A root connection such as `books` has no parent. A sort stack is the ordered list of `sort`
levels with their directions, and two stacks are the same only when they name the same columns in the same order with
the same directions. A filter set is the `filter` argument's fields with their values, and two filter sets that differ
only in the order of a multi-valued field's values are the same filter set. The refusal covers a cursor minted by a
different connection, by the same connection under a different parent, or by an `/api/v1/` list endpoint.

## Rationale

A client that walks a connection holds an opaque cursor whose meaning is fixed by the ordering and the row set it was
cut from. A cursor accepted under a different ordering, filter or parent would resume at a boundary computed for other
rows, so the next page would skip or repeat rows with nothing to show the client it had happened. The Library depends on
a refused cursor being loud and distinct from filter or sort validation, so it can restart the walk from the first page
without matching error message text. Attributing a refused cursor to its own connection field keeps a stale cursor on
one batched connection from discarding valid sibling connections in the same query. Refusing an `/api/v1/` cursor as
well keeps each API's cursor format private to that API, so neither format becomes a contract the other API has to keep.

## Acceptance criteria

- A `books` cursor presented under the sort stack and filter set it was minted under continues the walk at the next
  book, including when the `createdAt` bounds name the same instants with a different UTC offset. Checked by
  `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- A `books` cursor presented with a different sort column, level order or direction, or with a different filter set,
  produces an execution error with the `invalid-cursor` code and no `books` page. Reordering the values of a
  multi-valued filter continues the walk. Checked by `graphql_cursor_contract`.
- A cursor from a work's `manifestations` or a shelf's `items` presented under a different parent, or from a
  manifestation's pending versions presented under a different manifestation, produces an execution error with the
  `invalid-cursor` code and no page. Checked by `graphql_cursor_contract`.
- An authors cursor continues the list on the revision that minted it. The next revision of the same book refuses that
  cursor, even when the authors have not changed. Checked by `graphql_cursor_contract`.
- A refused cursor on one batched relation connection, pending-versions connection or `MetadataRevision` connection
  reports `invalid-cursor` at that connection's own field path while a valid sibling connection under an independent
  nullable ancestor in the same query returns its page. Checked by `graphql_cursor_contract`.
- A cursor from a different connection produces an execution error with the `invalid-cursor` code and no page: a
  `shelves` cursor or a relation cursor presented to `books`, and a `books` cursor presented to `shelves`. Checked by
  `graphql_cursor_contract`.
- A cursor in the `/api/v1/books` format presented to `books` under the same sort and filters, and one in the
  `/api/v1/shelves` format presented to `shelves`, each produce an execution error with the `invalid-cursor` code and no
  page. Checked by `graphql_cursor_contract`, which presents a copy of a GraphQL cursor carrying the `/api/v1` tag.
