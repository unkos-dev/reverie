---
severity: medium
surfaces: [server-operator, developer]
adopted: 2026-07-31
adopted-because: fixing the dashboard cover-coverage metric to count embedded EPUB covers required a new has_embedded_cover column set at ingestion; no data already stored (no persisted per-manifestation validation-issue rows, and cover_path covers only the enrichment sidecar) can reconstruct that flag for existing rows without re-reading each file
lift-when-class: internal-refactor
lift-when: a backfill pass exists that re-runs the EPUB cover check (the epub::cover_layer logic) against every manifestation where format='epub' and has_embedded_cover IS NULL, and has been run against production; non-EPUB rows are out of the backfill's scope by design (no structural validator exists to derive a value for them) and stay NULL after the lift
---

# Pre-migration manifestations have no embedded-cover flag

## Constraint

`manifestations.has_embedded_cover` is set at ingestion time from the EPUB
structural validator's cover-image check (declared, present, and
decodable/parsable). It is `NULL` for every manifestation ingested before the
column was introduced, for non-EPUB formats (no structural validator runs),
and for rows where the validator crashed.

## Workaround

The dashboard's "Cover" coverage metric treats `NULL` as "not covered" (same
as `false`), so a pre-existing EPUB with a perfectly good embedded cover but
no enrichment sidecar still reads as uncovered until it is re-ingested or a
backfill sets the flag. There is currently no mechanism, migration-time or
otherwise, that re-derives the flag for existing rows: the on-disk EPUB would
have to be re-opened and re-checked file by file.

## Lift trigger

Build a backfill pass that re-runs the cover check against every
manifestation where `format='epub'` and `has_embedded_cover IS NULL`, using
the same `epub::cover_layer` logic ingestion already runs, and run it against
production. Until then the "Cover" metric undercounts embedded covers on
libraries that predate this column.

Non-EPUB rows are deliberately outside this lift: no structural validator
exists for those formats, so their flag stays `NULL` and the metric counts
them as uncovered permanently. That is a property of the metric's
total-manifestations denominator, not of the missing backfill; narrowing the
denominator to formats a cover check can run on is a separate product
decision, and closing this entry does not resolve it.
