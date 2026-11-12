---
type: ADR
profile-version: 1
id: "REV-ADR-0073"
title: "Use rawzip for EPUB reads and writes"
status: "accepted"
recorded-on: "2026-11-12"
decided-on: "2026-11-12"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0049"
---

# Use rawzip for EPUB reads and writes

## Context and problem statement

EPUB validation needs lazy ZIP directory reads so archive limits apply before entries are materialised. Repair and
metadata writeback also need a writer that copies untouched compressed content and produces complete, readable EPUBs.
Maintaining a second ZIP library for writing duplicates dependency and API maintenance for the same format.

Which ZIP library should own EPUB reading and writing, and which archive properties matter to that choice?

## Decision drivers

- Archive admission keeps its bounded, lazy read path.
- Untouched file entries retain their names, compressed payloads, checksums, sizes and compression methods.
- Finished candidates pass validation before atomic publication.
- Library-specific ZIP timestamps, attributes and ancillary metadata have no established role in the library workflow.
- The dependency choice reduces maintenance without introducing custom ZIP-header manipulation.

## Considered options

- Use rawzip for EPUB reads and writes.
- Keep rawzip for reading and zip for writing.
- Extend rawzip upstream to reproduce ancillary ZIP metadata exactly.

## Decision outcome

Chosen option: **Use rawzip for EPUB reads and writes**, because it supports bounded reads and compressed-byte copying
through one dependency. Existing Deflate support supplies compression for changed entries. Content integrity and EPUB
readability govern publication. Exact reproduction of ZIP entry timestamps, external attributes, comments and extra
fields does not justify another writer or custom header handling.

### Consequences

- Positive: one ZIP library handles validation, repair, metadata writeback and fixture construction.
- Positive: untouched file payloads copy without decompressing or compressing them again.
- Negative: repacking normalises ancillary ZIP metadata, so external archive tools may report different timestamps or
  attributes for internal entries.
- Negative: rawzip uses data descriptors for file entries, including the Stored mimetype entry. Reader compatibility
  remains part of qualifying EPUB output.
- Negative: rawzip has one maintainer. Its small dependency footprint limits the code that Reverie inherits.
