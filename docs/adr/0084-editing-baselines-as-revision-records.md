---
type: ADR
profile-version: 1
id: "REV-ADR-0084"
title: "Editing baselines as revision records"
status: "accepted"
recorded-on: "2026-11-23"
decided-on: "2026-11-17"
decision-makers:
  - "John Unkovich"
---

# Editing baselines as revision records

## Context and problem statement

Browser application data runs on Relay under REV-ADR-0077. Relay normalises objects by global ID and merges partial
field selections into the existing record for that ID. Book metadata editing relies on optimistic concurrency: a save
must send the entity-tag belonging to the exact revision the edited values came from, and must never pair values from
one revision with an entity-tag written by another query or mutation.

If editable metadata fields and their entity-tag live directly on the mutable `Manifestation` record, a partial read
that refreshes `etag` or one field can overwrite part of the record while an open editor still holds values from an
earlier revision. How should GraphQL and Relay model editable metadata so a baseline's values and validator stay atomic
across store merges?

## Decision drivers

- Values and their matching entity-tag stay bound to one revision across partial reads and live store updates.
- Relay normalisation updates displayed metadata automatically when a mutation or fresh read returns a new revision.
- Editable relation pages such as contributors, genres, moods, tags and identifiers stay tied to the same revision as
  the scalar fields.
- Open editors keep their starting revision without blocking the rest of the screen from showing newer data.
- Avoid maintaining a second general-purpose client cache beside Relay's store.

## Considered options

- Revision-keyed `MetadataRevision` records in Relay
- Editable metadata fields directly on `Manifestation` with a separate client snapshot map
- Plain non-ID `MetadataSnapshot` objects replaced wholesale on every read

## Decision outcome

Chosen option: **Revision-keyed `MetadataRevision` records in Relay**, because giving each committed revision its own
immutable record ID prevents Relay normalisation from merging fields of one revision onto another revision's entity-tag,
while still letting mutations advance `manifestation.metadata` to the new revision automatically.

### Consequences

- Positive: a partial read of a newer revision writes a separate record in the Relay store and cannot alter the values
  or validator of an earlier revision an open editor holds.
- Positive: a successful mutation returns the manifestation pointing at the new revision record, so displayed views
  update through normal Relay store propagation without custom store update handlers.
- Positive: relation entries scoped under the revision record stay paired with that revision's scalar values and
  entity-tag.
- Negative: revision records accumulate in the client store until Relay garbage collection reclaims unused revisions.
- Negative: `MetadataRevision` cannot implement `Node` or support fetching an older revision by ID, because the server
  materialises only the current committed revision.
- Negative: whole-list editing of a paginated relation still has to collect every page from one revision before saving,
  because one revision record holds only the pages a query selected.

## Pros and cons of the options

### Revision-keyed `MetadataRevision` records in Relay

- Positive: structural isolation inside Relay's own normalisation rules prevents cross-revision mixing by construction.
- Positive: mutations and reads share one schema shape without duplicating fields across separate cache layers.
- Negative: adds a dedicated GraphQL type and deterministic revision ID derivation on the server.

### Editable metadata fields directly on `Manifestation` with a separate client snapshot map

- Positive: simpler GraphQL schema with no intermediate revision type.
- Negative: creates a second client cache beside Relay that every editing flow must keep synchronised manually, and
  still allows Relay fragments to read mixed revisions between partial queries.

### Plain non-ID `MetadataSnapshot` objects replaced wholesale on every read

- Positive: avoids deriving revision IDs on the server.
- Negative: every read of `manifestation.metadata` overwrites the parent's linked slot in place, so a background read
  clobbers an editor's active baseline unless the client copies every field out of Relay anyway.
