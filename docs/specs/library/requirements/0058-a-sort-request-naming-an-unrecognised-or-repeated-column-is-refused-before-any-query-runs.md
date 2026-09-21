---
type: REQ
profile-version: 1
id: "REV-REQ-0058"
title: "A sort request naming an unrecognised or repeated column is refused before any query runs"
governed-by:
  - "REV-ADR-0037"
---

# A sort request naming an unrecognised or repeated column is refused before any query runs

## Statement

WHEN a request to `GET /api/v1/books` carries a `?sort=` value that names a column outside the endpoint's published sort
vocabulary, names the same column more than once, or names more than three columns, the server MUST refuse the request
with `400 Bad Request` and MUST NOT build or execute a list query for it; only a column name from the published
vocabulary MAY reach the ordering clause of any query the endpoint runs.

## Rationale

The sort value is caller-supplied text that ends up next to the ordering clause of a SQL statement. The database
boundary depends on that text never reaching the statement unless it is one of a fixed set of names the server itself
maps to column expressions; a name that passed through without that mapping would be an identifier injection surface.
The paging client depends on the refusal too: a repeated or unknown column silently dropped would produce an ordering
the client did not ask for, and a cursor minted under it would not match the stack the client believes it holds. The
[sort stack decision](../../../adr/0037-multi-column-sort-stack-on-the-keyset-list-contract.md) fixes the vocabulary and
the three-level cap this obligation enforces.

## Acceptance criteria

- A `?sort=` value naming a column outside the published vocabulary answers `400`. Checked by
  `list_filter_malformed_sort_returns_400` in `backend/src/routes/library/tests.rs` and `rejects_unknown_field` in
  `backend/src/routes/sort_spec.rs`.
- A `?sort=` value naming the same column twice, in either direction, answers `400`. Checked by
  `list_filter_duplicate_sort_column_returns_400` in `backend/src/routes/library/tests.rs` and
  `rejects_duplicate_column` in `backend/src/routes/sort_spec.rs`.
- A `?sort=` value naming four or more columns answers `400`. Checked by `list_filter_too_many_sort_levels_returns_400`
  in `backend/src/routes/library/tests.rs` and `rejects_more_than_three_levels` in `backend/src/routes/sort_spec.rs`.
- A column name is matched case-sensitively against the vocabulary, so a differently cased spelling of a published name
  answers `400`. Checked by `rejects_uppercase_field_case_sensitively` in `backend/src/routes/sort_spec.rs`.
- The refusal happens before any query is built: the parse of the sort value completes, and fails, before the endpoint
  constructs its query. Satisfaction is determined by reading the handler's order of operations; no automated check
  asserts the absence of a query on the refusal path.
