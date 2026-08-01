---
status: "accepted"
date: 2026-08-02
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Match-time accent folding via unaccent expression indexes

## Context and Problem Statement

Search, suggest, and the typed text filters were accent-sensitive:
`garcia` did not find `García`. Library metadata is multilingual, users
type unaccented queries, and every fuzzy surface (tsvector search,
trigram suggest, `_contains` filters) needed the same accent-insensitive
behavior without corrupting stored values. Where should folding happen,
and what does it cost?

## Decision Drivers

- Stored metadata must keep its accents; display and export are exact.
- All match surfaces must fold identically, or the same needle finds
  different rows on different endpoints.
- Fuzzy legs must stay index-backed at library scale; a fold that
  disqualifies the trigram or GIN indexes is a regression.
- One folding authority: a Rust-side fold can drift from the dictionary
  the database indexes were built under.

## Considered Options

- Match-time folding in SQL: `unaccent` behind an `IMMUTABLE` wrapper,
  folded expression indexes, and an `unaccent_english` text search
  configuration.
- Store-side folding: shadow `*_folded` columns (or generated columns)
  populated at write time, indexed raw.
- Application-side folding: strip diacritics in Rust before binding.

## Decision Outcome

Chosen option: "match-time folding in SQL", because it keeps stored
values untouched, gives every surface the same dictionary, and stays
index-eligible: the `immutable_unaccent` wrapper makes the folded
expressions legal in trigram/GIN indexes, and queries fold both sides
through the same wrapper. Store-side shadow columns double write paths
and storage for a match-only concern; application-side folding creates
a second folding authority that can disagree with the index expressions
and cannot serve the tsvector leg at all.

Two subtleties are part of the decision, not incidental:

- The `IMMUTABLE` declaration is a documented lie. Postgres marks
  `unaccent()` STABLE because the dictionary can change; wrapping it
  trades a plan-time rejection for silent staleness whenever the
  effective dictionary changes. This is a permanent operating duty of
  the design, not tracked debt: any change to the dictionary against a
  persistent database (a Postgres image refresh, major bump or not; an
  extension update; a rules-file change) requires a `REINDEX` of the
  folded expression indexes and a `works.search_vector` recompute. CI
  databases are created fresh per run and carry no such duty. The
  warning lives at the dev and staging image pins, the only places a
  dictionary change enters a persistent database here.
- Escaping composes with folding in one direction only. The unaccent
  dictionary maps fullwidth punctuation into `%`/`_`/`\`, so LIKE
  escaping must run after folding, in SQL (`immutable_unaccent_like`);
  callers bind raw needles on folded legs and never pre-escape.

### Consequences

- Good, because `garcia` finds `García` on search, suggest, and
  filters alike, with stored accents intact and every leg index-backed.
- Bad, because index correctness now depends on dictionary stability
  with no failure signal; the recomputation duty above is the
  compensating control.
- Bad, because schema rollback is deliberately partial: the wrappers,
  configuration, and extension survive the down migration so a deployed
  binary that names them degrades to accent-sensitive matching instead
  of erroring; full removal requires rolling back the image first.
- Neutral, because `isbn_13` stays unfolded: digits and `X` have
  nothing to fold, so it keeps the plain escaped `ILIKE` path.

### Confirmation

Folded legs bind raw needles through `immutable_unaccent_like`; no call
site applies `escape_like` before a folded comparison. Metacharacter
smuggling and index eligibility are pinned by tests (fullwidth-needle
literal matching; EXPLAIN guards asserting the folded indexes serve
50k-row queries).

## More Information

Revisit on any Postgres image or unaccent change (the recomputation
duty above) and if match-time folding ever extends to write-side
identity (dedup uses raw-column similarity by design today).
