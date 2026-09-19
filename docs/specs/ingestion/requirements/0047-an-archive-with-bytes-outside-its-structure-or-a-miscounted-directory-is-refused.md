---
type: REQ
profile-version: 1
id: "REV-REQ-0047"
title: "An archive with bytes outside its structure or a miscounted directory is refused"
governed-by:
  - "REV-ADR-0049"
---

# An archive with bytes outside its structure or a miscounted directory is refused

## Statement

WHEN any byte of an EPUB archive's file precedes the position of its first local file header, or any byte follows the
true end of its end-of-central-directory record, or the number of entries actually present in its central directory
differs from the number the end-of-central-directory record declares, the EPUB validator MUST refuse the archive.

## Rationale

A byte preceding the first entry or trailing the recognised end of the archive is exactly the kind of ambiguity a
lenient reader might tolerate and a crafted archive could use to carry data past validation while still opening normally
in a more permissive tool. A central directory that holds more or fewer entries than its own end-of-central-directory
record declares means the two structures the format depends on to agree with each other do not, leaving any component
that trusts one over the other exposed to acting on an incomplete or duplicated view of the archive's contents. Refusing
every one of these disagreements outright, rather than tolerating the ones a permissive reader might explain away, keeps
a single crafted archive from being read two different ways by two different tools.

## Acceptance criteria

- One byte appended after the true end of the archive causes refusal. Checked by `trailing_one_byte_is_quarantined` in
  `backend/src/services/epub/zip_layer.rs`.
- 70,000 bytes appended after the true end of the archive causes refusal. Checked by
  `trailing_70000_bytes_is_quarantined` in `backend/src/services/epub/zip_layer.rs`.
- A forged end-of-central-directory record placed earlier in the file, pointing away from the true central directory,
  causes refusal rather than being read as a genuine trailing comment. Checked by
  `decoy_end_record_with_bogus_offset_is_quarantined` in `backend/src/services/epub/zip_layer.rs`.
- Data preceding the true first entry's local header, judged by file position rather than by central-directory order,
  causes refusal. Checked by `prepended_junk_before_archive_is_quarantined` in `backend/src/services/epub/zip_layer.rs`.
- A central directory holding more entries than the end-of-central-directory record declares causes refusal. Checked by
  `directory_longer_than_hint_is_quarantined` in `backend/src/services/epub/zip_layer.rs`.
- A central directory holding fewer entries than the end-of-central-directory record declares causes refusal. Checked by
  `directory_shorter_than_hint_is_quarantined` in `backend/src/services/epub/zip_layer.rs`.
- An archive with no comment on its end-of-central-directory record, whose recognised end exactly matches the actual
  length of the file, is not refused on this ground. Checked by `clean_zip_produces_no_issues` in
  `backend/src/services/epub/zip_layer.rs`.
- An archive carrying a genuine, non-empty comment that itself ends exactly at the true end of the file is not refused
  on this ground either; only a declared end that falls short of the file's actual length is. Checked by
  `archive_with_comment_validates_clean` in `backend/src/services/epub/zip_layer.rs`.
- An archive that exactly fills the file, with nothing preceding its first entry and nothing following its
  end-of-central-directory record, passes this check. Checked by `clean_zip_produces_no_issues` and
  `directory_entry_and_file_validate_clean_in_order` in `backend/src/services/epub/zip_layer.rs`.
