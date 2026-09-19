---
type: REQ
profile-version: 1
id: "REV-REQ-0046"
title: "An archive exceeding a resource bound is refused before extraction"
governed-by:
  - "REV-ADR-0049"
---

# An archive exceeding a resource bound is refused before extraction

## Statement

WHEN an EPUB archive exceeds any of the following bounds (a file size on disk of 2,147,483,648 bytes; a
central-directory entry count of 20,000 entries; a single entry's declared uncompressed size of 524,288,000 bytes; or an
aggregate declared uncompressed size, summed across every entry, of 2,147,483,648 bytes), the EPUB validator MUST refuse
the archive before any of its entries are extracted.

## Rationale

A hostile or malformed archive can declare an entry count or an uncompressed size far beyond anything a real EPUB
carries, forcing the pipeline to allocate memory or decompress data far out of proportion to the file's own size on disk
before any of it is even usable. Refusing such an archive before extraction protects every other file waiting in the
same ingestion run, and the process itself, from being starved of memory or disk by one crafted file. The bound on the
archive's file size on disk catches the same threat earlier still, before any of its bytes are read into memory at all.

## Acceptance criteria

Each bound below is enforced against the declared value named, not against a size verified by reading the archive:

| Bound                                                   | Limit                       |
| ------------------------------------------------------- | --------------------------- |
| Archive file size on disk                               | 2,147,483,648 bytes (2 GiB) |
| Declared central-directory entry count                  | 20,000 entries              |
| A single entry's declared uncompressed size             | 524,288,000 bytes (500 MiB) |
| Declared aggregate uncompressed size across all entries | 2,147,483,648 bytes (2 GiB) |

- An archive one byte over the file-size bound is refused before any of its bytes are read. Checked by
  `oversized_archive_is_quarantined_unread` in `backend/src/services/epub/zip_layer.rs`. Not checked by any automated
  test at the exact bound: no test confirms the positive case of a file exactly at the limit.
- A declared central-directory entry count of exactly 20,000 passes this check. Checked by `entry_count_at_cap_passes`
  in `backend/src/services/epub/zip_layer.rs`.
- A declared entry count of 20,001 is refused before any central-directory header is parsed. Checked by
  `entries_hint_over_cap_is_refused_before_iteration` in `backend/src/services/epub/zip_layer.rs`.
- An archive whose end-of-central-directory record understates its entry count as exactly 20,000, while its central
  directory in truth holds 20,001 entries, is refused by a second, counted check made during iteration, at the 20,001st
  entry, regardless of the declared count. Checked by `counted_iteration_cap_fires_despite_understated_hint` in
  `backend/src/services/epub/zip_layer.rs`.
- A single entry declaring an uncompressed size of 4,095 bytes or fewer, whose actual decompressed content fills the
  whole probe used to catch a size lie, is refused because the filled probe proves the declared size understates the
  truth; an entry declaring 4,096 bytes or more can lie about its true size without this specific check catching it.
  Checked by `stored_entry_larger_than_probe_cap_validates_clean_and_reads_back_whole` in
  `backend/src/services/epub/zip_layer.rs` for the companion negative case, an 8,192-byte entry above the probe that is
  not caught this way.
- Not checked by any automated test: the single-entry and aggregate declared-size bounds themselves, at or over their
  own limits. Both bound only the size an entry declares, not a size verified by reading the whole entry.
