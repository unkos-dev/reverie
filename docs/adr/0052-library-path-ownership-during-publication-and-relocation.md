---
type: ADR
profile-version: 1
id: "REV-ADR-0052"
title: "Library path ownership during publication and relocation"
status: proposed
recorded-on: "2026-10-03"
decision-makers:
  - John
---

# Library path ownership during publication and relocation

## Context and problem statement

Ingestion and metadata writeback select names in the same managed library. A relocation intent can reserve a name while
no file is present. Filesystem occupancy cannot distinguish that reservation from a free name, or establish which
manifestation owns an existing file during recovery and failed-ingestion cleanup.

## Decision drivers

- Coordinate managed publishers through the existing PostgreSQL transactions.
- Preserve the recorded location and both relocation names until recovery resolves their ownership.
- Enforce owner-correct locations even when a caller omits an application check.
- Delete claims with their manifestation.
- Refuse conflicting development data without silently selecting an owner.

## Considered options

- Shared PostgreSQL claims with deferred owner and location references.
- Filesystem-only occupancy checks and no-overwrite publication.
- Advisory exclusion without persistent ownership records.

## Decision outcome

Chosen option: **Shared PostgreSQL claims with deferred owner and location references**, because persistent claims
coordinate ingestion and writeback across missing files and retained intents. Native constraints enforce one owner
independently of the calling path.

Claims use the library identity and canonical relative path as their immediate unique key. Every recorded location and
non-null intent name references a claim belonging to that manifestation. Each claim references its owner's library and
cascades on owner deletion. Deferred circular foreign keys permit both sides to be inserted or removed in one
transaction. Transactional backfill takes the distinct union of recorded and intent names; a conflicting owner aborts
the migration. Ingestion and the writeback system context have claim access; ordinary application sessions do not.

### Consequences

- Positive: Managed ownership persists through interrupted moves and filesystem absence.
- Positive: Recorded locations, intents and owner deletion share database-enforced integrity.
- Positive: A conflicting backfill fails atomically rather than discarding an owner's location.
- Negative: Fixture writers and publishers must insert claims with manifestations in one transaction.
- Negative: Circular references require deferred checks, so integrity failures can surface at commit.
- Negative: Claims cannot establish external filesystem provenance or NAS durability. No-overwrite publication and
  content evidence remain necessary.

## Pros and cons of the options

### Filesystem-only occupancy checks and no-overwrite publication

- Positive: Publication protects an occupied destination without a database reservation.
- Negative: An absent intent destination appears free, and identical bytes cannot establish manifestation ownership.

### Advisory exclusion without persistent ownership records

- Positive: Serialises cooperating publishers and cleanup while a transaction holds the lock.
- Negative: Exclusion ends with the transaction and cannot represent an interrupted relocation's retained ownership.
