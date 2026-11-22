---
type: REQ
profile-version: 1
id: "REV-REQ-0092"
title: "A GraphQL books range filter applies its bounds as declared"
governed-by:
  - "REV-ADR-0066"
---

# A GraphQL books range filter applies its bounds as declared

## Statement

WHEN a request to the GraphQL `books` connection carries a range filter, which is one of the `BooksFilter` fields
`pagesGte`, `pagesLte`, `ratingGte`, `ratingLte`, `createdAtGte` and `createdAtLt`, the server MUST include a book whose
value equals a `Gte` or `Lte` bound, MUST exclude a book whose value equals a `Lt` bound, and MUST compare `createdAt`
bounds as instants rather than as the text the request wrote them in.

## Rationale

The Library reads its books through this connection. A range filter that is wrong at its boundary shows or hides exactly
the books on that boundary, such as a book added at the first instant of a chosen day, and nothing on the page signals
the mistake. Comparing instants rather than text keeps a page walk valid when a client writes the same instant with a
different offset.

## Acceptance criteria

- Equal numeric bounds match that exact value, and inverted numeric bounds return a successful empty connection. Checked
  for `pagesGte` and `pagesLte` by `graphql_cursor_contract` in `backend/src/graphql/mod.rs`.
- A `createdAt` filter includes a book created at its lower instant and excludes one created at its upper instant. Equal
  or inverted `createdAt` bounds return a successful empty connection. Checked by `graphql_cursor_contract`.
- `createdAt` bounds that name the same instants with a different UTC offset continue the same cursor walk, while a
  bound that names a different instant does not. Checked by `graphql_cursor_contract`.
