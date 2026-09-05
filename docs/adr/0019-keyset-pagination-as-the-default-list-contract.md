---
type: ADR
profile-version: 1
id: "REV-ADR-0019"
title: "Keyset pagination as the default list contract"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-08"
decision-makers:
  - "John Unkovich"
---

# Keyset pagination as the default list contract

## Context and problem statement

The [JSON API conventions ADR](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md) fixed the cursor
mechanism for the browser JSON surface: opaque base64url cursors, an RFC 8288 `Link` header plus a body
`next_cursor`, offset rejected. That decision was scoped to the `/api/books` surface. What is not on record is the
project-wide contract: whether keyset pagination is the rule for every growing list (browser, OPDS, admin, future),
what "no unbounded queries" means as a hard rule, and the tradeoffs the project is accepting by choosing keyset.

The contract is also not yet uniformly met. The catalog (`/api/books`) and the OPDS acquisition feeds are
keyset-paginated with a stable tiebreaker, and the OPDS series-editions feed is a capped single page. But several
lists issue `LIMIT`-less `SELECT`s whose row count grows with library or user size: `/api/shelves`, `/api/users`,
`GET /api/shelves/{id}` items, and the OPDS authors- and series-navigation feeds all return the entire set with no
cap. So at the time of this decision the surface was a mix of keyset, capped single-page, and unbounded queries,
with no recorded rule distinguishing them.

This matters at scale (large libraries) and for the threat model: Reverie's is the multi-user exposed instance,
where an unbounded list query is a resource-exhaustion surface. What is the list contract, and what does the
project give up to get it?

## Decision drivers

- Large-library scale. Offset pagination degrades as the table grows and shifts page boundaries under the
  asynchronous enrichment writes; keyset is O(log N) per page and stable under concurrent inserts.
- No small-library regression. A cursor is invisible at small N: the same contract serves a small and a large
  library without a UX fork.
- Threat model. On a multi-user exposed instance, a `LIMIT`-less list is a cheap resource-exhaustion vector; no
  query may scale its row count with attacker-influenced data.
- Enables the N+1 discipline. A bounded page is the precondition for the set-based-query / no-N+1 invariant to
  hold at scale.

## Considered options

- Keyset pagination by default, with a capped single page as a justified exception
- Offset / page-number pagination
- No project-wide contract; ad hoc per endpoint

## Decision outcome

Chosen option: **Keyset pagination by default, with a capped single page as a justified exception**, because it
bounds every list by construction, reuses the cursor mechanism already fixed for the browser JSON surface, and
keeps the resource-exhaustion surface a multi-user exposed instance presents from growing with library or user
size.

No unbounded queries: no list query may return a row count that grows without bound. Every list is bounded by
construction, either by keyset pagination or by a single page with a hard `LIMIT` cap. There are no `LIMIT`-less
scans.

Keyset pagination is the default: any set whose size grows with library or user size (catalog, search, OPDS
acquisition feeds, future admin lists) is keyset-paginated using the mechanism the
[JSON API conventions ADR](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md) already fixed
(opaque base64url cursor, `Link` header plus body `next_cursor`). Offset is rejected for the reasons in that ADR.

A capped single page is the justified exception, not an omission: a list with a known small natural ceiling, for
example a single series' editions, may return whole on one page, but only as a deliberate decision and only with a
defensive `LIMIT` so it is bounded by construction rather than by assumption (the OPDS series-editions feed already
does this). A list whose size grows with library or user count does not qualify: `/api/shelves`, `/api/users`,
`GET /api/shelves/{id}` items, and the OPDS authors- and series-navigation feeds returned uncapped sets at the time
of this decision.

Accepted tradeoffs, eyes open: keyset gives up two things the project consciously forgoes, cheap random access (no
"jump to page N", only next/prev from a cursor) and an exact total inline. A total count, where needed, is a
separate approximate or cached query, never an exact `COUNT(*)` on the hot path. Every sort axis must encode its
sort key in the cursor plus a stable tiebreaker to keep pagination total, already lived in the title and author
sorts, which carry an id tiebreaker and, for author, a NULL-bucket sub-tag so no row is dropped at a page boundary.

### Consequences

- Positive: the catalog scales to large libraries with stable, O(log N) pages that do not shift under concurrent
  enrichment writes.
- Positive: the same contract is invisible at small N, so there is no small-library UX regression and no
  per-size code fork.
- Positive: "no unbounded queries" bounds the resource-exhaustion surface a multi-user exposed instance presents.
- Positive: keyset is the contract that makes correct continuous/infinite scroll possible; a cursor walk is
  stable under the asynchronous enrichment writes, so a user scrolling never sees the duplicate or skipped rows
  that offset would produce mid-scroll. `next_cursor` / `Link rel="next"` is consumed directly by the frontend's
  incremental-loading path.
- Negative: there is no random page access and no exact inline total; a UI that wants either must design around
  next/prev and an approximate count.
- Negative: each new sort axis is real work: the sort key must be encoded in the cursor with a correct, stable
  tiebreaker, or pagination silently drops or duplicates boundary rows.

## Pros and cons of the options

### Keyset pagination by default, with a capped single page as a justified exception

- Positive: it scales, stays stable under concurrent writes, and bounds the query surface.
- Positive: it reuses the mechanism already shipped for the browser surface.
- Negative: it forgoes random page access and exact inline totals, and makes each sort axis a cursor-encoding
  exercise.

### Offset / page-number pagination

- Positive: it offers trivial random access (`?page=N`) and a familiar UI.
- Negative: it degrades at scale and shifts page boundaries under the asynchronous enrichment writes, displaying
  duplicates and skipping rows, the correctness failure the JSON API conventions ADR already rejected it for.

### No project-wide contract; ad hoc per endpoint

- Positive: each endpoint can do the locally simplest thing.
- Negative: it is how `/api/shelves`, `/api/users`, and the OPDS navigation feeds became unbounded in the first
  place; without a rule, the resource-exhaustion surface grows silently.

## More information

- [JSON API conventions ADR](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md): the cursor
  mechanism (encoding, `Link` header, offset rejection) this contract makes project-wide; that ADR is not
  superseded, it is the prior art this references.
- Pairs with the `backend/CLAUDE.md` "No N+1 queries" invariant: a bounded page is the precondition for set-based
  queries to hold at scale.
- The synthetic large-library perf fixture verifies the contract holds at both small and large N.
- Revisit trigger: if a genuine random-access need appears (a UI that must jump to an arbitrary page, or an
  export that needs an exact live total), weigh a bounded offset window or a materialised count against this
  contract in a new ADR rather than reintroducing offset ad hoc.
