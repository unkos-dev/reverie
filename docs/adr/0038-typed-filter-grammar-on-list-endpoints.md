---
type: ADR
profile-version: 1
id: "REV-ADR-0038"
title: "Typed filter grammar on list endpoints"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-07-07"
decision-makers:
  - "John Unkovich"
---

# Typed filter grammar on list endpoints

## Context and problem statement

The keyset-paginated books list already carries vocabulary filters (tags, genres, and moods as all-of, any-of, and
none-of sets) alongside three single-id filters. Those vocabulary filters fixed a convention on the wire: one URL
parameter per condition, with the set operator as a suffix on the key (`tag_any`, `genre_none`), which native
`URLSearchParams` parses without a custom syntax.

A large library needs typed per-column conditions on top of that: text contains and equals on title, subtitle, and ISBN;
numeric ranges on page count; date ranges on when a book was added; a reading-status filter; a rating filter; a
multi-value author filter; and a quick-search box that narrows the visible table. Naively adding each of these ad hoc
would fork a second filter shape against the one already shipped, and a grammar that puts the operator inside an
expression syntax needs its own parser and reintroduces an injection surface every time a value is stitched into SQL.

What is the URL grammar for flat typed filter conditions on a keyset list, such that it stays injection-safe,
index-backed, readable as ordinary query name/value pairs on both client and server, and consistent with the vocabulary
filters already shipped?

## Decision drivers

- Injection safety: a client never names a raw SQL identifier, the column set is closed, and every filter value is
  parameter-bound before any SQL is built.
- Native parseability: every condition must round-trip as an ordinary query name/value pair, such as native
  `URLSearchParams` reads on the client, with no expression parser on either end.
- Convention consistency: typed conditions must extend the vocabulary-filter suffix convention (`tag_any`, `genre_none`)
  already on the wire, not stand up a second filter shape beside it.
- Flat AND-only semantics: the scope is a flat conjunction of per-column conditions; every active filter narrows the set
  further. Nothing here needs OR across columns or nested boolean logic.
- Keyset-cursor correctness: a cursor minted under one filter set must not silently page a boundary computed under a
  different one.

## Considered options

- Flat suffix operator grammar
- Bracketed operator syntax
- Single filter-string mini-language

## Decision outcome

Chosen option: **flat suffix operator grammar**, because it extends the vocabulary-filter suffix convention already in
the API rather than introducing a second filter shape, each condition is an ordinary name/value pair with no expression
parser to build or harden, and the closed column set plus parameter-bound values make injection unrepresentable by
construction.

The chosen grammar uses one typed parameter per column condition and a closed set of suffix operators. The status filter
admits an `unread` value that matches the absence of a set reading status, not the absence of a reading-state row,
because a row can exist carrying only a rating; a rated but unread book is therefore still unread. Quick search narrows
the current ordered list rather than ranking results. Ranked search stays a separate surface that jumps to a single
book: the split is deliberate, one surface for finding a book and one for refining a view. Inputs are bounded and
parameter-bound, and a cursor is tied to the active filter set.

### Consequences

- Positive: one grammar covers every typed condition the closed column set allows, and the same URL round-trips through
  the grid, the cursor, and a shared link.
- Positive: every condition is an ordinary name/value pair on both ends, with no expression parser to build or harden.
- Positive: the closed column set plus parameter-bound values make injection unrepresentable, consistent with the sort
  whitelist.
- Negative: the flat suffix grammar is AND-only: it cannot express OR across columns or nested boolean logic.
- Negative: each new comparator is a new typed parameter, so the parameter surface grows roughly linearly with the
  number of filterable columns.

## Pros and cons of the options

### Flat suffix operator grammar

- Positive: it extends the vocabulary-filter suffix convention already on the wire, so the whole filter surface follows
  one operator convention.
- Positive: the closed suffix-parameter set means a column name is never client input, which makes injection
  unrepresentable and keeps each condition bound to a fixed, index-backed column expression.
- Neutral: the grammar is intentionally narrow; growing it past a flat AND is a separate decision, not a stretch of this
  one.
- Negative: it is AND-only and grows one parameter per comparator, so the surface widens roughly linearly as filterable
  columns are added.

PostgREST models horizontal filtering as one parameter per condition with the operator in the value (`?pages=gte.300`).
This option adopts that same one-parameter-per-typed-condition model but moves the operator into the key, which is
exactly the shape the repo already shipped for the vocabulary filters (`tag_any`, `genre_none`). Either placement parses
as an ordinary name/value pair; `URLSearchParams` does not interpret the operator in either.

### Bracketed operator syntax

- Positive: the bracketed operator is explicit and widely seen in Stripe and JSON:API clients.
- Negative: it adds a second operator convention beside the suffix convention (`tag_any`, `genre_none`) already on the
  wire. `URLSearchParams` reads `created[gte]` as an ordinary name, so the bracketed operator still needs the same
  application-level interpretation a suffix does.

### Single filter-string mini-language

- Positive: one expression parameter can express arbitrary boolean logic, well past a flat AND.
- Negative: it is over-engineered for a flat AND grammar: it needs a full expression parser and its own
  injection-hardening surface, neither of which a flat conjunction of typed conditions requires.

## More information

- [Multi-column sort stack on the keyset list contract](./0037-multi-column-sort-stack-on-the-keyset-list-contract.md):
  the companion sort decision on the same list; filtering and sorting share the cursor and the closed-column-set stance.
- [No unbounded queries: keyset pagination as the default list contract](./0019-keyset-pagination-as-the-default-list-contract.md):
  the keyset list contract this filtering rides on.
- [JSON API conventions for Reverie's browser-facing REST surface](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md):
  the opaque-cursor mechanism and query-shape conventions this reuses.
- [Match-time accent folding via unaccent expression indexes](./0044-match-time-accent-folding-via-unaccent-expression-indexes.md):
  the later decision on accent folding across search, suggest, and the text filters' contains conditions.
- Revisit trigger: the flat suffix grammar is AND-only and tops out around the current parameter count. A requirement
  for OR across columns, nested boolean conditions, or continued unbounded parameter growth switches list filtering to a
  single expression-string parameter (AIP-160 style) in a superseding ADR, rather than stretching the suffix grammar
  further.
