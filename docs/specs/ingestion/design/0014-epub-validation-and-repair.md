---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0014"
title: "EPUB validation and repair"
satisfies:
  - "REV-REQ-0043"
  - "REV-REQ-0044"
  - "REV-REQ-0045"
  - "REV-REQ-0046"
  - "REV-REQ-0047"
governed-by:
  - "REV-ADR-0049"
---

# EPUB validation and repair

This Design covers the five-layer structural check every EPUB file passes through before it is trusted: ZIP archive
integrity read through `rawzip`, `META-INF/container.xml` and `OPF` package-document parsing, `XHTML` spine-document
well-formedness, and cover-image decoding. It covers the shared `Issue` vocabulary those layers append to, the severity
tiers that decide whether a finding is automatically repaired, tolerated, or fatal, and the repair pass that rewrites
and publishes a validated candidate combining repairable findings with any tolerated degraded content.

## Purpose and boundaries

This subject owns: pure opened-file `inspect` and `validate`, the repairing `validate_and_repair` entry point, and their
result types (`backend/src/services/epub/mod.rs`); the `Issue`/`IssueKind`/`Severity`/`Layer` vocabulary every layer
appends findings to, and the shared `is_safe_path` traversal check guarding every archive-relative path before it is
used, whether a layer applies it directly to the path it resolves or, as Layer 5 does for its primary cover path,
inherits it from an earlier layer's check on that same href (Layer 3's, made at manifest insertion), applying the check
itself only to an SVG cover's sibling-image path; Layer 1's ZIP integrity read and its `ZipHandle` backing store, built
on `rawzip` (`zip_layer.rs`); Layer 2's `container.xml` parse and OPF-path discovery, including regeneration when
`container.xml` cannot be read (`container_layer.rs`); Layer 3's OPF package-document parse into `OpfData`, the
manifest, spine, Dublin Core fields, W3C accessibility metadata, and series metadata (`opf_layer.rs`); Layer 4's XHTML
spine-document encoding and well-formedness checks (`xhtml_layer.rs`); Layer 5's cover-image decoding check
(`cover_layer.rs`); the repair plan that records `Repaired`-severity entry instructions (`repair.rs`); and the low-level
repack helper that rebuilds a ZIP archive with the `mimetype` entry first and stored, copying every untouched entry
verbatim (`repack.rs`).

It does not own what a caller does with a `Quarantined` outcome, and the three production callers do three different
things with it. The Design "Ingestion pipeline" discards its owned candidate, retains the drop-zone original and records
the rejection reason without a manifestation row. The Design "Writeback pipeline" rejects a quarantined source or
candidate before publication, leaving rejected content's source bytes and stored hash/location untouched. On-demand
cover extraction in the Design "Covers" (`backend/src/services/covers/extract.rs`) does not call this subject's entry
point at all; it re-runs Layer 1 alone and turns any `Irrecoverable` finding into `CoverError::ArchiveRejected`. This
subject owns only the production of the `Quarantined` value and the issues behind it, not any of those three
dispositions.

It does not own committing a validation outcome to the database. The `validation_status` column, its Postgres enum, and
the `ValidationStatus` Rust type that maps one-to-one onto this subject's own `ValidationOutcome` variants
(`pending`/`clean`/`repaired`/`degraded`, plus a `failed` value this subject never produces itself) are written by the
calling pipeline, chiefly the Design "Ingestion pipeline".

It does not own turning parsed OPF metadata into the canonical fields a work or manifestation row stores. `OpfData` is
this subject's own output; a separate extractor, not part of this subject and not specific to any one subject by its
placement in the module tree, turns it into database-shaped metadata, called only by the Design "Ingestion pipeline".

It does not own extracting, caching, resizing, or serving cover bytes for a reader; that is the Design "Covers". This
subject's cover layer only judges whether an embedded cover is usable at serve time, sharing the exact detection and
SVG-rasterisation logic the Design "Covers" uses so the two never disagree on what counts as a usable cover, without
itself extracting, caching, or resizing anything.

It does not own the OPF and cover-image bytes a metadata rewrite produces. The Design "Writeback pipeline" computes
those bytes and hands them to this subject's own repack function to fold into a fresh archive; the OPF rewrite logic and
cover-embed planning that produce those bytes belong to that subject, not this one.

Depends on: `rawzip` for Layer 1's lazy, allocation-bounded central-directory read; `flate2` for on-the-fly Deflate
decompression during Layer 1's probes and full entry reads; the `zip` crate, built with only the
`deflate-flate2-zlib-rs` and `time` features, for the repack write path; `quick_xml` for `container.xml`, OPF, and XHTML
parsing; `encoding_rs` for XHTML transcoding; `image`, plus `rasterize_svg`, `looks_like_svg`, and `parses_as_svg`,
owned by the Design "Covers", for cover decoding; and `cap-std-ext` and `cap-tempfile` for durable replacement beneath
an opened parent. This subject opens no database connection.

Ingestion passes an opened copied-library file to `validate_and_repair` with its actual parent capability and basename.
Writeback uses `inspect`, builds entry repairs and metadata changes together, then calls `repack::publish`. Cover
extraction consumes an opened file through Layers 1 to 3 and shares cover-path and SVG helpers with Layer 5.

## Structure

**Outcome derivation runs in three stages, and only the first two can quarantine early.** `inspect` runs Layer 1 and
then the whole-archive entry verification, checks for an `Irrecoverable` issue, and returns `Quarantined` immediately if
one exists; runs Layer 2 and repeats the same check; then runs Layers 3, 4, and 5 together before a final severity
sweep. The `Irrecoverable` push sites are Layer 1 and the entry verification (`zip_layer.rs`), Layer 2
(`container_layer.rs`), and the package-document check in `inspect` after Layer 3; no push site in Layer 4 or 5
constructs an `Irrecoverable` issue. The third `has_irrecoverable` check inside `inspect`, after Layers 3 to 5 have run,
therefore fires only for an unreadable package document. Below the `Irrecoverable` tier, the source outcome is
`Repaired` when any issue supplies a repair instruction, otherwise `Degraded` when degraded issues remain, otherwise
`Clean`. Pure checking applies no repairs. Successful repair publication retains `Repaired` status and the successful
repair issues separately from the final candidate's unresolved issues. Remaining severity, not successful repair status,
governs candidate acceptance.

**Layer 1: ZIP integrity (`zip_layer.rs`).** Checks opened-handle metadata against `MAX_ARCHIVE_BYTES` before reading
any bytes. Locates the file-backed archive with `rawzip::ZipArchive::with_max_search_space` bounded to the fixed 22-byte
end-of-central-directory record plus the ZIP format's maximum 65,535-byte comment, and rejects the file outright if the
locator fails, if trailing bytes follow the located end (a short comment or garbage after the archive), or if the end
record's declared entry count exceeds `MAX_ZIP_ENTRIES` before any header is parsed. A module-level comment states the
rationale directly: every ambiguity the locator tolerates instead of rejecting, such as a comment ending early or a
directory-offset error the locator defers to iteration time, is treated as an outright rejection here, because the
entry-count guard is sound only if this layer never opens an archive interpretation the writer side would not itself
have produced. The central directory is then walked as a counted iteration, refusing the moment the count exceeds
`MAX_ZIP_ENTRIES` regardless of what the end record declared (the backstop for an end record that understates the true
count). Per entry, in order: the name must decode as UTF-8 and pass `is_safe_path` before anything else runs against it
(the traversal test matches two consecutive dots anywhere in the name, so `cover..jpg` fails it as surely as `../x`);
the name must not repeat a name already seen (case-sensitive, exact string); the entry must not be encrypted and must
declare Stored or Deflate compression; its declared uncompressed size must not exceed `MAX_ENTRY_UNCOMPRESSED_BYTES`,
and the running sum of declared sizes must not exceed `MAX_AGGREGATE_UNCOMPRESSED_BYTES`; the entry's local header must
be found and, when small enough, a bounded decompression probe confirms its actual size is consistent with what it
declared. After the loop, the smallest recorded local-header offset across every entry must be zero (rejecting any
prelude before the true first entry), and the counted total must equal the declared count in both directions (catching a
central directory that is either longer or shorter than the end record claims). Admitted names map to owned
`ZipArchiveEntryWayfinder` values and compression methods in a bounded index; entry reads use that index without walking
the central directory again.

**The `mimetype` entry's OCF container rules are checked last, and only once the archive has already passed every check
above.** A compliant archive is guaranteed to have at least one entry at offset zero by that point; the `mimetype` check
is keyed by name, not position, so that guarantee is all it needs: it first identifies the entry the central directory
names `mimetype`, then asks whether that specific entry, by both its recorded position and its own local header's name,
actually sits at offset zero. This is a spoofing defence as much as a position check: the position test does not merely
ask whether the central directory's recorded offset for the `mimetype` entry is zero, it independently reads the local
header at that recorded offset and confirms it is actually named `mimetype`, so a central-directory record that lies
about its own entry's position cannot pass. Four independent rules are checked when the entry is found: it must sit at
offset zero (by the test above), its local header must declare Stored (not Deflate), its local header must carry no
extra field, and its first bytes, read up to a 64-byte probe cap, must match `application/epub+zip` exactly. Absence is
reported alone; every other rule is independent, so more than one can fire on the same entry. Every one of these five
findings (absence, wrong position, wrong compression, an extra field, wrong content) carries `Repaired` severity, never
`Irrecoverable`: a non-conformant `mimetype` entry does not admit anything a well-formed one would refuse, so this is
EPUB and e-reader-interoperability conformance, not a defence against a hostile archive, and it is the one finding every
repack fixes as a side effect regardless of which repairable issue triggered the repack, because the repack helper
always writes a compliant `mimetype` entry first, whether or not that was the fix requested.

**Layer 2: `container.xml` (`container_layer.rs`).** If the `META-INF/container.xml` entry cannot be read (absent, or
present but failing the CRC-and-size-verified `read_entry`; the whole-archive entry verification exempts this one entry
for that reason), the layer scans the already-validated entry list for a `.opf` file. If one is found, the layer
regenerates the container path from it and records a `Repaired` `MissingContainer` issue carrying the candidate. A found
candidate is still checked with `is_safe_path`; an unsafe candidate is `Irrecoverable` `UnsafeOpfPath`. If no `.opf`
file exists anywhere in the archive, the `MissingContainer` issue carries no candidate and is `Irrecoverable`, because
no package document can be located. If the entry can be read, its `<rootfile full-path="...">` attribute is extracted
and, if unsafe, likewise rejected as `Irrecoverable`. A `container.xml` entry that reads successfully but whose bytes
are not valid UTF-8, fail XML parsing, or carry no `rootfile` element with a `full-path` attribute is `Irrecoverable`
`CorruptEntry` naming `META-INF/container.xml`; it is not regenerated, because the archive does not say which package
document it intended.

**Layer 3: `OPF` (`opf_layer.rs`).** Parses the manifest (`id` to `href`), the two OPF-native cover fields Layer 5's own
cascade consumes (`cover_href` from the EPUB 3 `properties="cover-image"` attribute; `meta_cover_href` from the EPUB 2
`<meta name="cover">` declaration resolved through the manifest and gated to an image media type), the spine, Dublin
Core bibliographic fields, W3C accessibility `<meta>` elements, and series metadata (calibre or EPUB 3
`belongs-to-collection`) in one pass. A manifest `<item>` whose `href` fails `is_safe_path` is dropped from the manifest
and recorded as `Degraded` `UnsafeManifestHref`, not `Irrecoverable`; a `<spine><itemref>` whose `idref` has no matching
manifest entry, whether because the manifest never declared it or because an unsafe href dropped it, is removed from the
spine and recorded as `Repaired` `BrokenSpineRef`. This layer owns entity and character-reference decoding for every
human-readable text field it emits (element text and `content` attribute values); a downstream consumer must treat those
fields as already decoded and must not decode them again, while the few attributes read raw (`id`, `idref`, `refines`,
`name`, `property`, `properties`, `role`/`opf:role`, `media-type`, `href`) are reference keys compared only against each
other or literal vocabulary terms. Exactly as with Layer 2, if the OPF entry cannot be read, its bytes are not valid
UTF-8, or the XML fails to parse, the function returns `None` without recording an issue of its own. `inspect` treats
that `None` as `Irrecoverable` `CorruptEntry` naming the package document, because no metadata, manifest, or spine can
be trusted without it. Recoverable findings inside a package document that parses stay `Repaired` or `Degraded` as
above.

**Layer 4: `XHTML` (`xhtml_layer.rs`).** If the spine holds more than `MAX_SPINE_ITEMS` spine references, the layer
emits a single `Degraded` `SpineCapExceeded` issue and validates no spine document at all, not just the entries past the
cap. Otherwise, each spine document is read and checked under a three-condition encoding rule: a non-UTF-8 encoding must
be declared (XML declaration or byte-order mark), the raw bytes must fail UTF-8 parsing, and decoding under the declared
encoding must succeed cleanly; only when all three hold does the layer emit a `Repaired` `EncodingMismatch` and validate
the transcoded bytes as XML. The `detected` field this issue carries is set to the literal string `"UTF-8"`
unconditionally; nothing in the layer performs encoding detection beyond the declared-encoding-versus-UTF-8-parse test
the three-condition rule already runs. A UTF-8 parse failure without a usable declared encoding is `Degraded`
`AmbiguousEncoding` and is not transcoded. An XML well-formedness failure on the (possibly transcoded) bytes is
`Degraded` `MalformedXhtml`.

**Layer 5: Cover (`cover_layer.rs`).** Resolves the cover href through its own `find_cover_href`, the three-way cascade
that chains Layer 3's `cover_href`, then Layer 3's `meta_cover_href`, then a legacy magic-id fallback (`cover-image`,
`cover`, `Cover`, `Cover-Image`) looked up directly against the manifest; this is also the function the Design "Covers"
calls to locate the same cover. Layer 5 then reads the entry and accepts it if it decodes as a raster image or, for an
SVG, rasterises to a visible image through `rasterize_svg`, owned by the Design "Covers", with sibling resolution scoped
to the same archive. A missing entry or one that cannot be decoded is `Degraded`; a cover that parses as SVG but renders
nothing visible (empty, or referencing an unresolved sibling) is treated the same as no cover declared, with no issue at
all. The layer returns a plain boolean, `true` only when a cover both exists and renders at serve time, which the
calling pipeline persists directly instead of re-deriving it on each subsequent read.

**Repair and repack (`repair.rs`, `repack.rs`).** `RepairPlan` records entry names, declared encoding labels, broken
spine references and a discoverable container target. It converts each requested entry during repack and releases that
payload after writing. An OPF repair precedes metadata transformation. A regenerated container replaces an unreadable
existing entry; it is added only when absent.

`with_modifications` consumes an admitted `ZipHandle` and a random-access candidate file. The `zip` reader parses the
source only after rawzip admission. The writer emits compliant mimetype first and Stored, writes replacements and
additions, and raw-copies untouched compressed payloads and metadata. It finishes before checking or hashing output.

## Interfaces and dependencies

`inspect(File)` returns an admitted handle and report; `validate(File)` returns the pure report.
`validate_and_repair(File, &Dir, &OsStr)` returns `Validated` with the report and optional accepted hash/size. These
synchronous interfaces run on blocking workers. `repack::publish` accepts the actual parent and basename, a source
report and a build closure, returning the final report, SHA-256 and handle-derived size only after replacement succeeds.

The publication callback builds and flushes the candidate, validates a cloned final-file handle without repair, and
hashes its final bytes. It rejects irrecoverable content, worse remaining severity, or unresolved repair instructions.
`atomic_replace_with` then synchronises the candidate, replaces the basename and synchronises its parent. Callers reuse
the returned hash and size.

`zip_layer::read_entry` reads admitted indexed entries. `cover_layer::find_cover_href` and `is_safe_path` are shared
with cover extraction and SVG sibling resolution. Ingestion stores selected report fields; other callers inspect the
opened file afresh rather than consulting persisted validation evidence.

## Data and state

A `ZipHandle` owns the opened archive, a cloned source handle for repack, admitted names and their bounded index. A
repair plan retains instructions rather than a collection of converted chapters. Pure checks mutate no files;
publication replaces one basename beneath its opened parent. The bounds enforced across the five layers, all named
constants in `mod.rs` or `zip_layer.rs`:

| Constant | Value | What it bounds |
| -------- | ----- | -------------- |
| `MAX_ARCHIVE_BYTES` | 2 GB (same value as `MAX_AGGREGATE_UNCOMPRESSED_BYTES`) | Raw file size, checked by `stat` before the file is read at all |
| `MAX_ZIP_ENTRIES` | 20,000 | Central-directory entry count, checked twice: from the end record's declared count before any header is parsed, and again as a counted backstop during iteration |
| `MAX_ENTRY_UNCOMPRESSED_BYTES` | 500 MB | One entry's declared uncompressed size |
| `MAX_AGGREGATE_UNCOMPRESSED_BYTES` | 2 GB | The running sum of every entry's declared uncompressed size |
| `MAX_SPINE_ITEMS` | 500 | Spine reference count; over the cap skips XHTML validation for the whole spine, not only the excess |
| `MAX_EOCD_SEARCH_SPACE` | 65,557 bytes | How far back from the end of the file the end-of-central-directory locator searches (the fixed 22-byte record plus the format's maximum 65,535-byte comment) |
| `MIMETYPE_CONTENT_PROBE_CAP` | 64 bytes | How much of the `mimetype` entry's content is read to check it against `application/epub+zip` |
| the per-entry extraction probe cap | `min(declared_size + 1, 4096)` bytes | How much of an entry is decompressed during Layer 1's own lying-directory check |
| the entry verification read cap | `declared_size + 1` bytes | How much of each entry the whole-archive verification decompresses |

The per-entry and aggregate caps bound the *declared* size a central-directory record carries, not a verified actual
size. Layer 1's own probe only catches a declared-size lie for an entry small enough that `declared + 1` is at most
4,096 bytes, and it reads through a plain, unverified reader that never checks the CRC-32 the central directory
declares. The whole-archive entry verification closes that gap: after Layer 1, `inspect` streams every admitted entry
except `container.xml` through decompression under a CRC-32 and size verifying reader, reading at most the entry's
declared size plus one byte, and records `Irrecoverable` `CorruptEntry` for the first entry whose bytes differ from what
the directory declares. Because every entry was verified, `read_entry`, which caps a read at
`MAX_ENTRY_UNCOMPRESSED_BYTES + 1` bytes and requires the same verification, cannot fail on CRC for any entry a later
layer reads, apart from `container.xml`. On-demand cover extraction calls Layer 1 alone and does not pay for the
verification pass.

## Runtime behaviour

**A structurally clean EPUB.** Every Layer 1 check passes, the `mimetype` entry meets all four OCF rules, Layer 2 finds
and reads `container.xml`, Layer 3 parses the OPF with every manifest href safe and every spine reference resolved,
Layer 4 finds no encoding or well-formedness problem within the spine cap, and Layer 5 finds a usable cover or none
declared. No issue is recorded; the outcome is `Clean`, and the file is never touched.

**An EPUB whose only problem is a non-conformant `mimetype` entry.** Layer 1 records one or more `Repaired`
`InvalidMimetype` findings (for example, the entry is Deflated and not first) and nothing else. `has_repairable` is
true, so `RepairPlan` runs: no broken spine refs, no encoding fixes, and no missing container mean the only effective
mutation is `repack::with_modifications`'s own unconditional compliant-`mimetype`-first rewrite. The repacked file is
persisted atomically over the source, and a second validation pass over the same path finds no `InvalidMimetype` issue
and an outcome of `Clean`.

**An EPUB with a broken spine reference.** Layer 3 removes the dangling spine reference from `spine_idrefs` and records
a `Repaired` `BrokenSpineRef`. `RepairPlan` rewrites the OPF, removing the matching `<itemref>` element (matched by a
depth counter rather than a boolean, so a malformed OPF with nested `itemref` elements cannot prematurely clear the skip
state), and the repacked archive is persisted atomically. If the same run's Layer 1 also found a non-conformant
`mimetype` entry, both fixes land in the same repack.

**A `container.xml` entry that cannot be read at all, with a discoverable `.opf` file in the archive.** Layer 2 records
a `Repaired` `MissingContainer` with the discovered candidate path. `RepairPlan` regenerates `META-INF/container.xml`
from that candidate (escaping the path for safe XML interpolation) and replaces the existing entry or adds it when
absent; Layer 3 onward already ran against the regenerated path within the same validation pass, since Layer 2 returns
the candidate immediately once it is confirmed safe.

**An archive with no usable package document.** An empty archive, an archive with neither a `container.xml` nor any
`.opf` file, a `container.xml` that names no package document, a named package document that is absent from the archive,
and a package document whose bytes are not valid UTF-8 or well-formed XML each produce an `Irrecoverable` issue and a
`Quarantined` outcome. A readable archive whose package document parses stays importable: missing covers, broken spine
references, encoding mismatches and a non-conformant `mimetype` entry are `Repaired` or `Degraded`.

**An archive with a corrupt entry.** An entry whose decompressed bytes fail the declared CRC-32 or size, anywhere in the
archive, is `Irrecoverable` `CorruptEntry` and the outcome is `Quarantined`, whether or not the entry is structural. The
one exception is `META-INF/container.xml`: the verification pass skips it, so bytes that fail the CRC-32 or size make
the entry unreadable to Layer 2, which regenerates it when a package document is discoverable and otherwise quarantines
the archive. A `container.xml` whose bytes pass the check but do not parse, or that names no package document, is not
regenerated and is `Irrecoverable`.

**An archive shaped to exhaust memory or CPU.** An end-of-central-directory record declaring more entries than
`MAX_ZIP_ENTRIES` is rejected before any header is parsed. A central directory that in fact holds more entries than it
declared is caught by the counted-iteration backstop at the same cap, regardless of the declared count. An entry whose
declared uncompressed size, or whose running aggregate, exceeds the per-entry or aggregate cap is rejected before it is
decompressed. Any of these is `Irrecoverable`; `validate_and_repair` returns `Quarantined` immediately after Layer 1,
without running Layer 2 or any layer after it.

**An EPUB the check rejects outright.** A path-traversal entry name, a duplicate entry name, an encrypted entry, an
unsupported compression method, data preceding the first entry, a corrupt central directory, or an entry failing
whole-archive verification each produce an `Irrecoverable` issue in Layer 1 or the verification pass and an immediate
`Quarantined` outcome. Layer 2 raises `Irrecoverable` issues for an unsafe `OPF` path, for a `container.xml` that is
absent or unreadable with no `.opf` file to regenerate it from, and for a readable `container.xml` that does not parse
or names no package document. `inspect` raises one for a package document that is absent or does not parse, after Layer
3. Each yields the same `Quarantined` result.

## Failure and recovery

Structural findings remain issues inside a successful report; irrecoverable findings produce `Quarantined`. Source I/O,
ZIP writing, required repair and validator errors propagate. A required entry read or conversion cannot be silently
omitted. Before callback acceptance, failures discard the candidate and leave source bytes untouched.

An error returned by the maintained replacement operation after callback acceptance means publication or durability is
uncertain. `PublicationUncertain` carries the accepted hash and underlying error; callers perform no content rollback,
relocation or row-success update on that result. A retry inspects the recorded source afresh. No phase is inferred from
an upstream error string.

An error the validator raises, as opposed to a finding in its report, is classified by `EpubError::is_file_defect`. A
candidate refusal, a failed required repair, an XML rewrite error and a ZIP error that is not an I/O error are verdicts
on the file; the ingestion caller rejects the input as it does a `Quarantined` outcome. An I/O error, a ZIP I/O error
and an uncertain publication are faults in Reverie's own storage; the ingestion caller keeps the file and registers it
with `validation_status = failed`. Writeback sends candidate rejection and errors to its existing failed/retry path. A
missing or unreadable container with a discoverable OPF is repaired once, without duplicate container entries.

## Security and operations

This subject implements compensating controls for EPUB's mandatory use of ZIP archives: the archive must begin at offset
zero (the prelude check); an archive-size cap runs before the file is read; a declared-entry-count cap runs from the
end-of-central-directory record before any header is parsed; the central-directory iteration itself is counted and
refuses at the cap regardless of what the end record declared; and a per-entry and an aggregate uncompressed-size cap
bound decompression. No archive found inside an EPUB entry is ever opened by any layer in this subject, so no
nesting-depth bound is needed. This subject extracts nothing to a generated filename and opens no database connection of
its own; the register's remaining compensating controls belong to the calling pipelines, chiefly the Design "Ingestion
pipeline".

The `mimetype` entry's five OCF container rules are conformance, not a defence against hostile input: a `mimetype` entry
in the wrong position, compressed, carrying an extra field, or holding the wrong content does not let an archive evade
any of this subject's other checks, or do anything a compliant archive could not already do; the rules exist so the
archive opens correctly in every e-reader that enforces them strictly, and every finding is `Repaired`, never
`Irrecoverable`. The size, count, compression-method, encryption, path-safety, duplicate-name, and prelude checks in
Layer 1, by contrast, each bound what an archive can make this subject or a downstream consumer do before extraction,
and are the controls the deviation register's compensating-controls list is about.

## More information

- [CodeGuard deviation register](../../../security/codeguard/README.md), deviation 4: the compensating controls this
  subject implements for processing ZIP archives, and the ones a neighbouring subject implements instead.
