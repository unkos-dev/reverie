---
type: REQ
profile-version: 1
id: "REV-REQ-0094"
title: "A GraphQL books sort naming an unrecognised or repeated column is refused before any query runs"
governed-by:
  - "REV-ADR-0037"
---

# A GraphQL books sort naming an unrecognised or repeated column is refused before any query runs

## Statement

WHEN a request to the GraphQL `books` connection carries a `sort` argument that names a column outside the `SortField`
enum, names the same column in more than one level, or has more than three levels, the server MUST refuse the request
with an error and no page before it builds or runs a list query for it. The `SortField` enum is the case-sensitive
values `TITLE`, `AUTHOR`, `CREATED_AT` and `PAGES`.

## Rationale

The Library reads its books through this connection and pages them with cursors minted under the sort it sent. A
repeated or unknown column silently dropped would produce an ordering the client did not ask for, and a cursor minted
under it would not match the stack the client believes it holds. Refusing before the query is built keeps a malformed
sort from reaching the ordering clause of a SQL statement.

## Acceptance criteria

- A `sort` argument naming a differently cased column, the same column twice in either direction, or four levels
  produces an error with the `validation` code and no `books` page. Checked by `graphql_cursor_contract` in
  `backend/src/graphql/mod.rs`.
- The refusal happens before any query is built: `books_connection` in `backend/src/services/reader.rs` parses the sort
  stack, and returns the failure, before it opens a transaction or constructs its query. Satisfaction is determined by
  reading that function's order of operations; no automated check asserts the absence of a query on the refusal path.
