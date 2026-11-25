---
type: ADR
profile-version: 1
id: "REV-ADR-0037"
title: "Multi-column sort stack on the keyset list contract"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-07-07"
decision-makers:
  - "John Unkovich"
---

# Multi-column sort stack on the keyset list contract

## Context and problem statement

The [keyset pagination list contract](./0019-keyset-pagination-as-the-default-list-contract.md) made keyset the default
for every growing list and recorded the price: each sort axis must encode its sort key in the cursor with a stable
tiebreaker, or pagination silently drops or duplicates rows at a page boundary. Under that contract the books list
offered three fixed single-axis sorts with hardcoded directions, each with a hand-written cursor variant.

A large library needs richer ordering than one fixed axis: scan by author and then by newest within an author, sort by
page count, reverse any axis. Naively generalising to a client-controlled multi-column sort runs into three traps. A
client sort string interpolated into SQL is an injection surface. A tuple comparison across levels is wrong the moment
directions differ or a column is nullable, because a nullable cascade without an explicit null branch drops every
null-valued row after the first page. And an ordering on a paged-row column with no matching index degrades to a full
sort of the whole table at scale, while joined and per-user attributes cannot be indexed on the paged row without
duplicating columns or multiplying rows per user.

What is the contract for a multi-level, mixed-direction, nullable-aware sort on a keyset-paginated list, such that it
stays injection-safe, index-backed on paged-row columns, and total across page boundaries?

## Decision drivers

- **Keyset totality.** The stack must page without dropping or duplicating a row at any boundary, under any mix of
  directions and any nullable column.
- **Injection safety.** A client never names a raw SQL identifier; sort input resolves through a closed allow-list
  before any SQL is built.
- **Index-backed ordering on the paged row.** Columns on the paged row (`works` and `manifestations`) resolve through
  ordering indexes rather than a full table sort. Joined and per-user columns (`series`, `status`, and `rating`) sort
  over the filtered set rather than requiring duplicated columns or per-user cross-product rows.
- **Shareable, recoverable state.** A sort stack lives in the URL so it is bookmarkable, and the API rejects a stale
  cursor rather than returning a quietly wrong page.
- **Small surface.** Reuse the existing opaque-cursor mechanism rather than inventing a second pagination shape.

## Considered options

- JSON:API sort syntax over a whitelist, versioned opaque cursor
- OData `$orderby` syntax
- Fixed single-axis sorts with hardcoded enum values

## Decision outcome

Chosen option: **JSON:API sort syntax over a whitelist, versioned opaque cursor**, because it matches the sort-array
shape the frontend grid already emits and the project's existing JSON:API convention, reuses the existing opaque-cursor
mechanism, and lets a hard server-side whitelist keep every orderable column injection-safe while enforcing ordering
indexes on paged-row columns.

The sort parameter uses comma-separated fields with a leading minus for descending order. A closed server-side whitelist
bounds sortable columns. Columns on the paged row (`works` and `manifestations`) enter the whitelist only with matching
ordering indexes. Joined and per-user columns (`series`, `status`, and `rating`) enter the whitelist without an ordering
index and sort over the filtered set. Nullable columns order NULLS LAST in both directions, because books with an
unknown author or page count should not lead the results when an axis is reversed. Duplicate columns are rejected, and
the stack is capped at three levels, which covers real curation without unbounded cursor growth. The opaque cursor
records the sort specification and boundary values so replay under a different sort fails.

### Consequences

- Positive: one grammar expresses every stack the whitelist allows, and the same URL round-trips through the grid, the
  cursor, and a shared link.
- Positive: the whitelist keeps injection unrepresentable by construction and keeps every paged-row column (`works` and
  `manifestations`) backed by an ordering index.
- Positive: the explicit null-bucket branch and the id tiebreaker keep every mixed-direction, nullable stack total
  across page boundaries.
- Positive: the versioned cursor tag leaves room for a future payload shape without breaking the tag-dispatch family it
  shares with the other cursors.
- Negative: adding a sortable paged-row column is real work: a whitelist entry, its ascending and descending NULLS LAST
  indexes when nullable, and a cursor round-trip, all in one migration.
- Negative: sorting on joined or per-user columns (`series`, `status`, and `rating`) sorts the filtered set rather than
  reading an ordering index, so page cost grows with the filtered row count.
- Negative: a cursor cannot survive a sort change; the client re-requests the first page when the stack changes, which
  is correct but not free.

## Pros and cons of the options

### JSON:API sort syntax over a whitelist, versioned opaque cursor

- Positive: the grammar is compact, matches the grid's native sort array, and follows the project's existing JSON:API
  convention.
- Positive: the whitelist makes injection unrepresentable and ties every paged-row axis to an ordering index.
- Negative: each new paged-row axis is an index migration plus a cursor round-trip, and joined or per-user axes pay a
  sort over the filtered set.

### OData `$orderby` syntax

- Positive: the per-field `asc`/`desc` suffix is explicit and widely known.
- Negative: it is more verbose, does not match the grid's sort-array shape, and pulls in an OData surface the project
  uses nowhere else.

### Fixed single-axis sorts with hardcoded enum values

- Positive: it needs no new parsing or cursor work.
- Negative: it cannot express multi-level ordering at all, and every new axis is another hardcoded enum arm and bespoke
  cursor variant, which is the cost the keyset contract already flagged.

## More information

- [Keyset pagination list contract](./0019-keyset-pagination-as-the-default-list-contract.md): the contract this
  extends; it priced in "each sort axis is real work", and this record is the multi-axis instance of that price.
- [JSON API conventions](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md): the opaque-cursor
  mechanism and JSON:API stance this reuses.
- [Library sort is a per-user preference, never URL state](./0047-library-sort-is-a-per-user-preference-never-url-state.md):
  the later decision that moves the library's sort stack out of the URL into a per-user preference, revising the
  URL-state driver here while leaving this record's wire sort contract unchanged.
- Revisit trigger: if resource naming standardises on an explicit field-selection grammar across the API, reconcile this
  sort syntax with it in a new record. If page latency on a joined or per-user sort (`series`, `status`, or `rating`) or
  a deep multi-level stack exceeds acceptable bounds when measured at a named library size, weigh stored sort keys on
  the paged row, a covering index, or query restructuring rather than widening the whitelist.
