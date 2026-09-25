---
type: ADR
profile-version: 1
id: "REV-ADR-0049"
title: "Read EPUB archives with rawzip"
status: "accepted"
recorded-on: "2026-09-14"
decided-on: "2026-09-14"
decision-makers:
  - "John Unkovich"
---

# Read EPUB archives with rawzip

## Context and problem statement

EPUB ingestion validates the structure of every archive before the file enters the library. Layer 1 of that pipeline is
responsible for keeping a hostile or malformed archive from ever reaching the container, OPF, XHTML, or cover layers
beneath it, and one of its jobs is bounding how many central-directory entries an archive may declare before the rest of
the pipeline runs.

The `zip` crate this layer read through builds an in-memory map of every central-directory header before
`ZipArchive::new` returns, and exposes no count, size, or laziness control over that parse. A crafted archive that
declares a central-directory entry count far beyond anything a real EPUB carries forces the crate to materialise every
one of those headers regardless of any cap a caller applies afterwards, because the cap can only run on a structure the
crate has already built in full. The guard has to run ahead of the parse, not after it, and the `zip` crate gives no
seam to do that.

How should Layer 1 read a ZIP archive so that an entry-count cap, and the structural checks around path safety,
compression method, and duplicate names, can run without first paying for the crate's own unbounded parse?

## Decision drivers

- The entry-count guard must run before any per-entry header is materialised, not as a check applied to a structure the
  crate has already built.
- The replacement must expose central-directory iteration lazily, with no per-entry allocation, so a counted stop at the
  cap is possible.
- EPUB containers use only Stored and Deflate compression; a replacement should not need a decoder for methods no valid
  EPUB uses.
- Reverie already writes ZIP archives with `zip`'s `ZipWriter`; a read-side change should not force an immediate rewrite
  of the write side.
- The dependency sits directly on the ingestion path for hostile files, so its own footprint (dependency count, unsafe
  code, maintenance) matters as much as its API.

## Considered options

- `rawzip`: reads the end-of-central-directory record and iterates central-directory headers lazily, with a configurable
  search window and no per-entry allocation.
- `rc-zip`: read-only, and keeps its declared entry count private to the crate, so a caller cannot read it ahead of
  iterating.
- `piz`: pre-allocates on the raw ZIP64 declared count before any cap can be applied.
- `async_zip`: exposes no synchronous API.
- Wait for the `zip` crate to add a limit surface upstream.

## Decision outcome

Chosen option: **rawzip**, because it is the only candidate that reads the declared entry count from the
end-of-central-directory record ahead of any header parse and iterates the central directory lazily, letting the
entry-count cap run as a counted stop rather than a check applied after the fact. It has no runtime dependencies,
forbids unsafe code, and is already relied on downstream by `gix-archive`. The `zip` crate keeps its role for writing;
this decision covers only the read side.

### Consequences

- Positive: the entry-count cap runs as a counted iteration that stops at the limit, with no per-entry allocation ahead
  of the cap.
- Negative: two ZIP-reading crates now sit in the dependency tree, `rawzip` for reads and `zip` for writes, until `zip`
  is retired from repack.
- Negative: `rawzip` is maintained by one person. The risk is offset by its empty dependency list, its
  `forbid(unsafe_code)`, and its existing use in `gix-archive`.

## Pros and cons of the options

### rawzip

- Positive: the declared entry count is readable before any header parse, and central-directory iteration is lazy with
  no per-entry allocation.
- Positive: no runtime dependencies, and unsafe code is forbidden throughout.
- Neutral: maintained by one person, with no organisational backing.

### rc-zip

- Negative: the declared entry count is private to the crate, so a caller cannot apply a cap before iterating.
- Neutral: read-only, which is no worse than what this decision needs.

### piz

- Negative: pre-allocates on the raw ZIP64 declared count before a cap can be applied, defeating the purpose of the
  change.
- Negative: no release since 2022.

### async_zip

- Negative: exposes no synchronous API, and the EPUB validation pipeline is synchronous throughout.

### Wait for `zip` upstream

- Negative: the relevant upstream issues are open with no committed fix, and the entry-count gap is exploitable today.
