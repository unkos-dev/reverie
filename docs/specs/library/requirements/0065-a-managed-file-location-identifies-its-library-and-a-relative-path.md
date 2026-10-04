---
type: REQ
profile-version: 1
id: "REV-REQ-0065"
title: "A managed file location identifies its library and a relative path"
governed-by:
  - "REV-ADR-0051"
---

# A managed file location identifies its library and a relative path

## Statement

For each managed manifestation, Reverie MUST record the file's owning library identity and its actual path relative to
that library's root. A recorded path MUST be non-empty and contain no absolute prefix, backslash, drive prefix, empty
component, dot component or parent component. A download MUST select the recorded location independently of metadata or
organisation rules; an unknown library identity MUST NOT select another library's root.

## Rationale

A stable library identity separates ownership from deployment paths and permits different libraries to contain the same
relative filename. Recording the actual location prevents metadata edits or organisation policies from redirecting reads
before a file has moved.

## Acceptance criteria

- A missing or unknown library foreign key and each forbidden path form are rejected by the database. The
  `library_storage_schema_` tests exercise actual constraints.
- A publication or relocation records its owning library and actual relative destination. Ingestion and writeback tests
  must assert that recorded value alongside the destination bytes.
- Two libraries containing the same relative filename deliver their own bytes and handle-derived lengths. Checked by
  `library_storage_download_two_library_same_name`.
- Changing a title leaves a recorded nested file readable. Checked by
  `library_storage_download_nested_location_survives_metadata_change`.
- A recorded library without a bound root returns a generic internal error and no file from another library. Checked by
  `library_storage_download_unknown_identity_is_generic`.
