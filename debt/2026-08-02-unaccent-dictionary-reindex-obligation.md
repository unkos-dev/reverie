---
severity: medium
surfaces: [server-operator]
adopted: 2026-08-02
adopted-because: accent-insensitive matching needs unaccent() inside expression indexes and Postgres refuses STABLE functions there, so the immutable_unaccent wrapper declares an immutability the unaccent dictionary does not actually guarantee across Postgres major versions; that converts index correctness from a planner-checked property into an operational obligation
lift-when-class: upstream
lift-when: Postgres ships an unaccent (or equivalent folding primitive) whose dictionary is versioned or genuinely immutable, so the folded expression indexes can declare their true volatility and staleness becomes detectable instead of silent
---

# Folded search indexes trust a dictionary Postgres may change

## Constraint

The eight `immutable_unaccent(...)` trigram expression indexes (authors,
series, works title/subtitle, genres, moods, tags, manifestations
publisher) and the `unaccent_english`-built `works.search_vector` all
embed the output of the `unaccent` dictionary at write time. Postgres
declares `unaccent()` STABLE precisely because the installed dictionary
can change; the `IMMUTABLE` wrapper is the documented workaround to make
the expressions index-eligible, and it removes the only failure signal.
If a Postgres major-version bump ships a changed `unaccent.rules`, the
indexes hold entries computed under the old dictionary, the planner
trusts them, and folded queries silently return wrong rows. Nothing
errors.

## Workaround

Accept the fake-IMMUTABLE declaration and carry the recomputation duty
by hand: any Postgres major-version bump against a persistent database
(the dev volume, staging, any production deployment) must `REINDEX` the
eight folded indexes and recompute `works.search_vector` (re-run the
backfill `UPDATE` plus `REINDEX INDEX idx_works_search_vector`) in the
same maintenance window. The dev and staging compose files carry a
warning comment on their pinned `postgres` images pointing here; CI
databases are created fresh per run and carry no such obligation.

## Lift trigger

Upstream: a Postgres release where the unaccent dictionary is versioned
(or the folding primitive is honestly immutable), letting the wrapper
and this recomputation duty disappear. Until then the obligation stands
for every major bump.
