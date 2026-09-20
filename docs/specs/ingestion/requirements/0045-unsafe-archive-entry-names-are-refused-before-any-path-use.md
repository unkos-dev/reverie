---
type: REQ
profile-version: 1
id: "REV-REQ-0045"
title: "Unsafe archive entry names are refused before any path use"
---

# Unsafe archive entry names are refused before any path use

## Statement

WHEN an entry name inside an EPUB archive contains two consecutive dots anywhere in the name, written literally or
percent-encoded in any letter case, a backslash, a leading slash, or a leading slash written as a percent-encoding in
any letter case, the EPUB validator MUST refuse the archive before that name is joined to a filesystem path or used to
read the entry's data. Two consecutive dots cover every parent-directory component and also a name such as `cover..jpg`,
which carries no traversal but is refused all the same.

## Rationale

An entry name is the only thing a downstream reader has to decide where an entry's bytes belong once extracted; an
archive that smuggles a traversal or an absolute path through that name could otherwise place or overwrite a file
outside the location the ingestion pipeline intends, or point extraction at a path never sanctioned by the library.
Refusing the whole archive on the first such name, rather than skipping only that one entry, keeps a crafted archive
from surviving with the rest of its content still admitted to the library.

## Acceptance criteria

- An entry name containing a literal parent-directory component ("..") causes the archive to be refused. Checked by
  `path_traversal_is_quarantined` in `backend/src/services/epub/zip_layer.rs`.
- An entry name containing two consecutive dots that form no parent-directory component, such as `cover..jpg`, causes
  the archive to be refused. Not checked by any automated test.
- An entry name containing a percent-encoded parent-directory component, in either letter case, causes the archive to be
  refused. Not checked by any automated test: only the literal form is exercised by name.
- An entry name containing a backslash causes the archive to be refused. Not checked by any automated test.
- An entry name starting with a leading slash causes the archive to be refused. Not checked by any automated test.
- An entry name starting with a percent-encoded leading slash, in either letter case, causes the archive to be refused.
  Not checked by any automated test.
- A manifest item whose href fails the same underlying safety check marks this obligation's boundary rather than an
  instance of it: the item is dropped from the manifest and the archive is not refused. Not checked by any automated
  test.
