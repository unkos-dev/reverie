---
status: active
severity: low
surfaces: [developer]
adopted: 2026-05-24
adopted-because: "11c (PR #316) ships manual metadata edit alongside the enrichment pipeline; the two paths normalize publisher whitespace differently before hashing, producing distinct value_hash entries for semantically identical values"
lift-when-class: internal-refactor
lift-when: PR unifies whitespace normalization for publisher values across manual edit (PATCH metadata) and enrichment pipeline paths; same PR adds a test proving identical publisher strings produce identical value_hash regardless of entry path
lifted: ~
superseded-by: ~
---

# Publisher whitespace hash-normalization diverges between manual and enrichment paths

## Constraint

The enrichment pipeline normalises publisher strings (collapse
whitespace, trim) before computing `value_hash`. The manual metadata
edit path (`PATCH /api/books/{id}/metadata`) does not apply the same
normalisation. Two entries with the same logical publisher but
different whitespace will produce different hashes, defeating dedup
in the metadata journal.

## Workaround

None applied. The divergence is cosmetic for now — dedup still works
on exact-match hash, and users rarely enter publishers with trailing
whitespace. But the inconsistency accumulates silently.

## Lift trigger

Extract the publisher normalisation into a shared function (or apply
it at the model layer before hashing) so both paths produce identical
hashes for semantically identical values.
