---
type: ADR
profile-version: 1
id: "REV-ADR-0044"
title: "Match-time accent folding via unaccent expression indexes"
status: "accepted"
recorded-on: "2026-09-06"
decided-on: "2026-08-02"
decision-makers:
  - "John Unkovich"
---

# Match-time accent folding via unaccent expression indexes

## Context and problem statement

Search, suggest, and the typed text filters were accent-sensitive: `garcia` did not find `García`. Library metadata is
multilingual, users type unaccented queries, and every fuzzy surface (tsvector search, trigram suggest, `_contains`
filters) needed the same accent-insensitive behaviour without corrupting stored values. Where should folding happen, and
what does it cost?

## Decision drivers

- Stored metadata must keep its accents; display and export are exact.
- All match surfaces must fold identically, or the same needle finds different rows on different endpoints.
- Fuzzy legs must stay index-backed at library scale; a fold that disqualifies the trigram or GIN indexes is a
  regression.
- One folding authority: a Rust-side fold can drift from the dictionary the database indexes were built under.

## Considered options

- Match-time folding in SQL: `unaccent` behind an `IMMUTABLE` wrapper, folded expression indexes, and an
  `unaccent_english` text search configuration.
- Store-side folding: shadow `*_folded` columns (or generated columns) populated at write time, indexed raw.
- Application-side folding: strip diacritics in Rust before binding.

## Decision outcome

Chosen option: **Match-time folding in SQL**, because it keeps stored values untouched, gives every surface the same
dictionary, and stays index-eligible: the `immutable_unaccent` wrapper makes the folded expressions legal in trigram/GIN
indexes, and queries fold both sides through the same wrapper. Store-side shadow columns double write paths and storage
for a match-only concern; application-side folding creates a second folding authority that can disagree with the index
expressions and cannot serve the tsvector leg at all.

Two subtleties are part of the decision, not incidental:

- The `IMMUTABLE` wrapper assumes a stable dictionary. Postgres marks `unaccent()` STABLE because the dictionary can
  change; wrapping it trades a plan-time rejection for silent staleness whenever the effective dictionary changes.
  Folded expression indexes and stored search vectors can then disagree with queries even when the schema is unchanged.
  An image refresh, extension update or rules-file change can alter folding behaviour; a new image digest alone does not
  establish that it has changed.
- Escaping composes with folding in one direction only. The unaccent dictionary maps fullwidth punctuation into
  `%`/`_`/`\`, so escaping before folding can turn a literal needle into a wildcard pattern. SQL-side escaping after
  folding preserves literal matching through the same dictionary.

### Consequences

- Positive: `garcia` finds `García` on search, suggest, and filters alike, with stored accents intact and every leg
  index-backed.
- Positive: `isbn_13` stays unfolded: digits and `X` have nothing to fold, so it keeps the plain escaped `ILIKE` path.
- Negative: index and stored search-vector correctness depend on dictionary stability with no failure signal. Preserving
  search correctness across dictionary changes adds regeneration and coordination to the upgrade process.
- Negative: schema rollback is deliberately partial: the wrappers, configuration, and extension survive the down
  migration so a deployed binary that names them keeps returning accent-insensitive results instead of erroring, but its
  folded trigram expressions run unindexed until the application image is rolled back too; full removal requires rolling
  back the image first.

## More information

Persistent database upgrades are a reason to revisit the dictionary-stability assumption. The maintenance mechanism
needs separate evaluation; this record does not select a detection or regeneration process or assign maintenance to end
users.

Revisit if match-time folding extends to write-side identity, where dictionary changes would also affect identity
comparisons.
