---
type: ADR
profile-version: 1
id: "REV-ADR-0051"
title: "Capability-based file I/O and validated publication"
status: "proposed"
recorded-on: "2026-09-30"
decision-makers:
  - "John Unkovich"
---

# Capability-based file I/O and validated publication

## Context and problem statement

Managed-library reads and writes select filesystem objects independently of the database rows that name them. A path
check followed by another open leaves a lookup gap: replacing the directory entry can select a different object.
Rewriting a live EPUB before validating its result also exposes readers to a candidate that may need restoration.

The decision covers authority over managed files and how complete replacements reach their destination. Database
ownership, authentication and archive resource limits retain their existing contracts.

## Decision drivers

- Download metadata and bytes describe one opened file, even across directory-entry replacement.
- Existing absolute database paths and links resolving inside the library remain usable.
- Reverie owns managed-library writes and reorganisation; external tools coordinate their changes with it.
- EPUB candidates can be rejected while the original bytes remain untouched.
- Maintained filesystem primitives reduce the replacement and recovery code Reverie owns.
- Durable EPUBs and rebuildable cover caches have different synchronisation needs.

## Considered options

- Opened directory capabilities with validated candidate publication through maintained crates.
- Retain canonical path guards and separate path-based filesystem operations.
- Retain raw `tempfile` life cycles and maintain publication, synchronisation and recovery in Reverie.

## Decision outcome

Chosen option: **opened directory capabilities with validated candidate publication through maintained crates**, because
it assigns filesystem authority to a workflow and separates building a replacement from exposing it to readers.

Use a concrete `LibraryFiles` boundary with cap-std, preserving stored absolute paths through a compatibility adapter.
The adapter resolves paths for classification and derives a relative target; the opened directory grants authority for
the actual open. Root acquisition is lazy and caches success only, so an unavailable library introduces no new startup
failure. Opened roots identify directory objects; external relocation or replacement requires a coordinated restart.

For writer adoption, select cap-std-ext for durable replacement and cap-tempfile for temporary ownership and cache
publication. Build, repair, finish, flush and validate an independent candidate before publishing it once; hash the
finalised candidate rather than write calls that a random-access archive writer may later revise. Durable EPUB
replacement syncs the file and parent directory; rebuildable caches use atomic publication without forced sync.

Keep blocking filesystem and archive work off request threads, within the workflow's concurrency bounds. Preserve
existing queue and database compensation ownership: filesystem publication and a Postgres transaction are separate
operations. A failure after rename can leave published bytes with unconfirmed durability, requiring an explicit error
policy rather than a blind rollback. Atomic replacement does not supply no-overwrite destination selection.

### Consequences

- Positive: a download's handle supplies both metadata and bytes, and contained opening closes the path lookup gap.
- Positive: candidate validation can reject a rewrite without changing the original file.
- Positive: maintained crates own contained resolution and replacement mechanics.
- Negative: absolute-link compatibility still needs ambient classification before contained opening.
- Negative: pinned roots require operator coordination when storage is relocated or replaced.
- Negative: additional dependencies and separate filesystem/database failure handling remain maintenance costs.

## Pros and cons of the options

### Opened directory capabilities with validated candidate publication through maintained crates

- Positive: file operations use explicit root authority and replacements have an independent validation boundary.
- Negative: permission preservation and errors after publication still need application policy.

### Retain canonical path guards and separate path-based filesystem operations

- Positive: preserves the current dependency footprint and path-oriented interfaces.
- Negative: repeated path lookups can select different objects after the containment check.

### Retain raw `tempfile` life cycles

- Positive: existing private temporary-file ownership can remain useful for path-oriented fixtures.
- Negative: Reverie owns the surrounding sync, publication and rollback mechanics; path-based reopen adds another
  lookup.

## More information

OPDS downloads apply the capability boundary. Writer migration is a separate delivery; this record states the selected
publication direction without claiming those callers already use it.

- [cap-std capability model](https://github.com/bytecodealliance/cap-std/blob/v4.0.3/README.md).
- [cap-std-ext replacement implementation](https://github.com/coreos/cap-std-ext/blob/v5.1.2/src/dirext.rs).
- [cap-tempfile ownership and replacement](https://github.com/bytecodealliance/cap-std/blob/v4.0.3/cap-tempfile/src/tempfile.rs).
- [NamedTempFile persistence](https://docs.rs/tempfile/3.27.0/tempfile/struct.NamedTempFile.html).
