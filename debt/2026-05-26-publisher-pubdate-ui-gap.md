---
severity: low
surfaces: [end-user]
adopted: 2026-05-24
adopted-because: "11c (PR #316) ships metadata edit UI but BookDetail does not yet carry publisher/pub_date canonical columns; UI confirmation for those fields deferred"
lift-when-class: internal-refactor
lift-when: BookDetail API response includes publisher and pub_date canonical columns; same PR adds UI fields to the metadata edit dialog for confirming/editing those values
---

# Publisher and pub_date missing from metadata edit UI

## Constraint

The metadata edit dialog (`EditMetadataDialog`) lets users
accept/reject/edit metadata fields. However, `publisher` and
`pub_date` are not included because the `BookDetail` API response
does not yet surface those canonical columns. Users cannot confirm
or correct publisher/publication date through the UI.

## Workaround

Those fields are only editable via direct API calls or through the
enrichment pipeline's automatic metadata flow. The UI gap means
manual correction requires API knowledge.

## Lift trigger

Extend `BookDetail` query and response to include `publisher` and
`pub_date` from the canonical metadata. Add corresponding fields to
the edit dialog. Wire accept/reject for both fields.
