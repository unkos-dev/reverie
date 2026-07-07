---
status: "accepted"
date: 2026-07-07
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Multi-column sort stack on the keyset list contract

## Context and Problem Statement

The [keyset pagination list contract](2026-06-08-keyset-pagination-list-contract.md)
made keyset the default for every growing list and recorded the price: each sort
axis must encode its sort key in the cursor with a stable tiebreaker, or
pagination silently drops or duplicates rows at a page boundary. Under that
contract the books list offered three fixed single-axis sorts with hardcoded
directions, each with a hand-written cursor variant.

A large library needs richer ordering than one fixed axis: scan by author and
then by newest within an author, sort by page count, reverse any axis. Naively
generalising to a client-controlled multi-column sort runs into three
traps. A client sort string interpolated into SQL is an injection surface. A
tuple comparison across levels is wrong the moment directions differ or a
column is nullable, because a nullable cascade without an explicit null branch
drops every null-valued row after the first page. And an ordering with no
matching index degrades to a full sort of the whole table at scale.

What is the contract for a multi-level, mixed-direction, nullable-aware sort on
a keyset-paginated list, such that it stays injection-safe, index-backed, and
total across page boundaries?

## Decision Drivers

- **Keyset totality.** The stack must page without dropping or duplicating a row
  at any boundary, under any mix of directions and any nullable column.
- **Injection safety.** A client never names a raw SQL identifier; sort input
  resolves through a closed allow-list before any SQL is built.
- **Index-backed ordering.** Every orderable column must resolve as an index
  range scan at large-library scale, not a full sort.
- **Shareable, recoverable state.** A sort stack lives in the URL so it is
  bookmarkable, and the API rejects a stale cursor rather than returning a
  quietly wrong page.
- **Small surface.** Reuse the existing opaque-cursor mechanism rather than
  inventing a second pagination shape.

## Considered Options

- **A: JSON:API sort syntax over a hard column whitelist, encoded into a
  versioned opaque cursor.**
- **B: OData `$orderby` syntax (`?orderby=author asc, created_at desc`).**
- **C: Keep fixed single-axis sorts; add more only as new hardcoded enum
  values.**

## Decision Outcome

Chosen option: **A**.

- **Sort is JSON:API syntax.** `?sort=author,-created_at`: comma-separated
  fields, a leading `-` reverses that level to descending, order of appearance
  sets priority. This matches the shape the frontend grid already emits and the
  project's JSON:API convention. An absent parameter means `-created_at`, the
  established recency default.
- **Orderable columns are a hard server-side whitelist.** Only `title`,
  `author`, `created_at`, and `pages` are sortable. Client field names resolve
  through a closed enum before any SQL is assembled; an unwhitelisted field is a
  400, never an interpolated identifier. A column enters the whitelist only
  together with its ordering indexes in the same migration, so "sortable" and
  "index-backed" cannot drift apart.
- **Nullable columns order NULLS LAST in both directions.** Unknown values sink
  to the tail whether the axis is ascending or descending, because surfacing the
  page-less or author-less stubs first is useless. Postgres orders descending as
  NULLS FIRST by default, so each nullable orderable column carries an explicit
  `DESC NULLS LAST` composite index; the cursor cascade carries an explicit
  `OR IS NULL` branch in both directions so the null bucket is never dropped.
- **The stack is capped at three levels; duplicate columns are rejected.** Both
  limits are enforced server-side and return 400. Three levels covers real
  curation without unbounded cursor growth.
- **The cursor is a versioned opaque payload.** It stays a base64url string of a
  tag plus a JSON body carrying the canonical sort spec, one typed boundary
  value per level, and the manifestation-id tiebreaker that keeps any stack
  total. A cursor names the key space it was minted for: replayed against a
  different sort it is rejected with 422 rather than paging a mismatched
  ordering. The manifestation id alone is the tiebreaker, which is unique and
  all totality needs.

### Consequences

- Good, because one grammar expresses every stack the whitelist allows, and the
  same URL round-trips through the grid, the cursor, and a shared link.
- Good, because the whitelist-with-indexes rule keeps every orderable column an
  index range scan at scale and makes injection unrepresentable by construction.
- Good, because the explicit null-bucket branch and the id tiebreaker keep every
  mixed-direction, nullable stack total across page boundaries.
- Bad, because adding a sortable column is real work: a whitelist entry, its
  ascending and descending NULLS LAST indexes, and a cursor round-trip, all in
  one migration.
- Bad, because a cursor cannot survive a sort change; the client re-requests the
  first page when the stack changes, which is correct but not free.
- Neutral, because the versioned cursor tag leaves room for a future payload
  shape without breaking the tag-dispatch family it shares with the other
  cursors.

### Confirmation

The sort whitelist is a closed Rust enum; client strings resolve through it and
only fixed column expressions ever reach the query builder. Every whitelisted
column is index-backed in the same migration that admits it, ordering NULLS LAST
in both directions. Boundary-walk tests exercise mixed directions, both null
buckets, and tie-breaking to prove no row is dropped or duplicated at a page
boundary, and a cursor replayed under a changed sort returns 422.

## Pros and Cons of the Options

### A: JSON:API sort over a whitelist, versioned opaque cursor

- Good, because the grammar is compact, matches the grid's native sort array,
  and follows the project's existing JSON:API convention.
- Good, because the whitelist makes injection unrepresentable and ties every
  axis to an index.
- Bad, because each new axis is a migration plus a cursor round-trip, not a free
  toggle.

### B: OData `$orderby`

- Good, because the per-field `asc`/`desc` suffix is explicit and widely known.
- Bad, because it is more verbose, does not match the grid's sort-array shape,
  and pulls in an OData surface the project uses nowhere else.

### C: Keep fixed single-axis sorts

- Good, because it needs no new parsing or cursor work.
- Bad, because it cannot express multi-level ordering at all, and every new axis
  is another hardcoded enum arm and bespoke cursor variant, which is the cost the
  keyset contract already flagged.

## More Information

- [Keyset pagination list contract](2026-06-08-keyset-pagination-list-contract.md):
  the contract this extends; it priced in "each sort axis is real work", and this
  ADR is the multi-axis instance of that price. Not superseded.
- [JSON API conventions](2026-05-22-json-api-conventions.md): the opaque-cursor
  mechanism and JSON:API stance this reuses. Not superseded.
- Revisit trigger: if resource naming standardises on an explicit field-selection
  grammar across the API, reconcile this sort syntax with it in a new ADR. If a
  deep multi-level stack over a filtered set shows the planner misestimating an
  incremental sort at scale, weigh a covering index or a per-query planner knob
  rather than widening the whitelist.
