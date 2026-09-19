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
well-formedness, and cover-image decodability. It covers the shared `Issue` vocabulary those layers append to, the
severity tiers that decide whether a finding is automatically repaired, tolerated, or fatal, and the repair pass that
rewrites and atomically replaces an archive carrying only repairable findings.

## Purpose and boundaries

This subject owns: the entry point `validate_and_repair` and its `ValidationReport`/`ValidationOutcome` result types
(`backend/src/services/epub/mod.rs`); the `Issue`/`IssueKind`/`Severity`/`Layer` vocabulary every layer appends findings
to, and the shared `is_safe_path` traversal check guarding every archive-relative path before it is used, whether a
layer applies it directly to the path it resolves or, as Layer 5 does for its primary cover path, inherits it from an
earlier layer's check on that same href (Layer 3's, made at manifest insertion), applying the check itself only to an
SVG cover's sibling-image path; Layer 1's ZIP integrity read and its `ZipHandle` backing store, built on `rawzip`
(`zip_layer.rs`); Layer 2's `container.xml` parse and OPF-path discovery, including regeneration when `container.xml`
cannot be read (`container_layer.rs`); Layer 3's OPF package-document parse into `OpfData`, the manifest, spine, Dublin
Core fields, W3C accessibility metadata, and series metadata (`opf_layer.rs`); Layer 4's XHTML spine-document encoding
and well-formedness checks (`xhtml_layer.rs`); Layer 5's cover-image decodability check (`cover_layer.rs`); the repair
orchestrator that applies every `Repaired`-severity finding and atomically replaces the source file (`repair.rs`); and
the low-level repack helper that rebuilds a ZIP archive with the `mimetype` entry first and stored, copying every
untouched entry verbatim (`repack.rs`).

It does not own what a caller does with a `Quarantined` outcome, and the three production callers do three different
things with it. The Design "Ingestion pipeline" removes the just-copied library file, moves the drop-zone original to
quarantine with a sidecar, and never commits a manifestation row for it. The Writeback pipeline subject never sees a
fresh `Quarantined` outcome from a file it is writing back to (the file already carried a manifestation row and a
non-quarantined outcome before the job started); it treats a `Quarantined` result from its own post-write validation
call as a regression and atomically restores the pre-write bytes, leaving the existing row's `file_path` and
`current_file_hash` exactly as they were before that run. On-demand cover extraction in the Design "Covers"
(`backend/src/services/covers/extract.rs`) does not call this subject's entry point at all; it re-runs Layer 1 alone and
turns any `Irrecoverable` finding into `CoverError::ArchiveRejected`. This subject owns only the production of the
`Quarantined` value and the issues behind it, not any of those three dispositions.

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

It does not own the OPF and cover-image bytes a metadata rewrite produces. The Writeback pipeline subject computes those
bytes and hands them to this subject's own repack function to fold into a fresh archive; the OPF rewrite logic and
cover-embed planning that produce those bytes belong to that subject, not this one.

Depends on: `rawzip` for Layer 1's lazy, allocation-bounded central-directory read; `flate2` for on-the-fly Deflate
decompression during Layer 1's probes and full entry reads; the `zip` crate, built with only the
`deflate-flate2-zlib-rs` and `time` features, for the repack write path; `quick_xml` for `container.xml`, OPF, and XHTML
parsing; `encoding_rs` for XHTML transcoding; `image`, plus `rasterize_svg`, `looks_like_svg`, and `parses_as_svg`,
owned by the Design "Covers", for cover decodability; and `tempfile` for the repack's atomic rename. This subject opens
no database connection of its own; every check and repair runs against a byte buffer read from, and (on repair) written
back to, the filesystem path the caller supplies.

Depended on by: the Design "Ingestion pipeline", which calls `validate_and_repair` once per freshly copied EPUB file and
branches on the returned `ValidationOutcome`; the Writeback pipeline subject, which calls `validate_and_repair` twice
per job (once before its own rewrite, to record a baseline outcome, and once after, to detect a regression), and which
also calls this subject's own `repack::with_modifications` and `zip_layer::read_entry_from_bytes` directly to build and
read its own rewritten archive, independently of `validate_and_repair`; and the Design "Covers", whose on-demand cover
extraction calls `zip_layer::validate`, `container_layer::validate`, and `opf_layer::validate` directly, and calls
`cover_layer::find_cover_href` to locate the same cover Layer 5 would check, without ever calling Layer 5's own
`validate` function. Layer 5 in turn depends on SVG helpers owned by the Design "Covers" to judge whether an
SVG-declared cover renders anything visible, the one place this subject's own validation logic calls into a neighbouring
subject's code rather than the reverse.

## Structure

**Outcome derivation runs in three stages, and only the first two can quarantine.** `validate_and_repair` runs Layer 1,
checks for an `Irrecoverable` issue, and returns `Quarantined` immediately if one exists; runs Layer 2 and repeats the
same check; then runs Layers 3, 4, and 5 together before a final severity sweep. Every `Severity::Irrecoverable` push
site in the module lives in Layer 1 (`zip_layer.rs`) or Layer 2 (`container_layer.rs`); no push site in Layer 3, 4, or 5
ever constructs an `Irrecoverable` issue. The third `has_irrecoverable` check inside `validate_and_repair`, after Layers
3 to 5 have run, therefore has nothing to find under today's issue vocabulary; a maintainer adding a Layer 3, 4, or 5
refusal is the first code to make that check reachable. Below the `Irrecoverable` tier, the outcome is `Repaired` if any
issue carries `Severity::Repaired` (repair runs and, on success, replaces the outcome's severity gate), else `Degraded`
if any issue carries `Severity::Degraded`, else `Clean`.

**Layer 1: ZIP integrity (`zip_layer.rs`).** Checks the file's size against `MAX_ARCHIVE_BYTES` with a `stat` call
before reading any bytes. Reads the whole file, locates the archive with `rawzip::ZipArchive::with_max_search_space`
bounded to the fixed 22-byte end-of-central-directory record plus the ZIP format's maximum 65,535-byte comment, and
rejects the file outright if the locator fails, if trailing bytes follow the located end (a short comment or garbage
after the archive), or if the end record's declared entry count exceeds `MAX_ZIP_ENTRIES` before any header is parsed. A
module-level comment states the rationale directly: every ambiguity the locator tolerates instead of rejecting, such as
a comment ending early or a directory-offset error the locator defers to iteration time, is treated as an outright
rejection here, because the entry-count guard is sound only if this layer never opens an archive interpretation the
writer side would not itself have produced. The central directory is then walked as a counted iteration, refusing the
moment the count exceeds `MAX_ZIP_ENTRIES` regardless of what the end record declared (the backstop for an end record
that understates the true count). Per entry, in order: the name must decode as UTF-8 and pass `is_safe_path` before
anything else runs against it; the name must not repeat a name already seen (case-sensitive, exact string); the entry
must not be encrypted and must declare Stored or Deflate compression; its declared uncompressed size must not exceed
`MAX_ENTRY_UNCOMPRESSED_BYTES`, and the running sum of declared sizes must not exceed
`MAX_AGGREGATE_UNCOMPRESSED_BYTES`; the entry must be locatable and, when small enough, a bounded decompression probe
confirms its actual size is consistent with what it declared. After the loop, the smallest recorded local-header offset
across every entry must be zero (rejecting any prelude before the true first entry), and the counted total must equal
the declared count in both directions (catching a central directory that is either longer or shorter than the end record
claims).

**The `mimetype` entry's OCF container rules are checked last, and only once the archive has already passed every check
above.** A compliant archive is guaranteed to have at least one entry at offset zero by that point; the mimetype check
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
present but failing Layer 1's own CRC-and-size-verified `read_entry`, which is a stricter check than Layer 1's own
per-entry probe; see Failure and recovery), the layer scans the already-validated entry list for a `.opf` file and, if
one is found, regenerates the container path from it, recording a `Repaired` `MissingContainer` issue in either case. A
found candidate is still checked with `is_safe_path`; an unsafe candidate is `Irrecoverable` `UnsafeOpfPath`. If the
entry can be read, its `<rootfile full-path="...">` attribute is extracted and, if unsafe, likewise rejected as
`Irrecoverable`. A `container.xml` entry that reads successfully but whose bytes are not valid UTF-8, fail XML parsing,
or carry no `rootfile` element with a `full-path` attribute takes neither of those paths: the extraction function
returns `None` at each of those points without pushing any issue, and Layer 2 as a whole reports no OPF path found and
no problem recorded, exactly as if the layer had never run. The `MissingContainer` regeneration only fires when the
entry itself cannot be read; a readable-but-unparsable `container.xml` is not repaired and is not flagged.

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
UTF-8, or the XML fails to parse, the function returns `None` silently: there is no issue kind for a malformed or
unreadable OPF, and no code path in this layer reports one. Layer 3 has no error-signal for its own parse failure at
all, unlike Layer 4 below, which does report a malformed spine document as an issue.

**Layer 4: `XHTML` (`xhtml_layer.rs`).** If the spine holds more than `MAX_SPINE_ITEMS` idrefs, the layer emits a single
`Degraded` `SpineCapExceeded` issue and validates no spine document at all, not just the entries past the cap.
Otherwise, each spine document is read and checked under a three-condition encoding rule: a non-UTF-8 encoding must be
declared (XML declaration or byte-order mark), the raw bytes must fail UTF-8 parsing, and decoding under the declared
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
to the same archive. A missing entry or an undecodable one is `Degraded`; a cover that parses as SVG but renders nothing
visible (empty, or referencing an unresolved sibling) is treated the same as no cover declared, with no issue at all.
The layer returns a plain boolean, `true` only when a cover both exists and renders at serve time, which the calling
pipeline persists directly instead of re-deriving it on each subsequent read.

**Repair and repack (`repair.rs`, `repack.rs`).** `repair::repackage` collects every `Repaired`-severity issue by kind
(broken spine idrefs, non-OPF encoding fixes, the missing-container flag and its candidate) and builds the inputs
`repack::with_modifications` needs: an optional OPF replacement (spine refs removed, an encoding fix applied, or both,
chained in that order when both apply to the same entry), a map of non-OPF binary replacements for their own encoding
fixes, and a regenerated `META-INF/container.xml` addition when the container was missing and a safe candidate was
found. `repack::with_modifications` always writes a compliant `mimetype` entry first and Stored, regardless of which
mutation was requested, then copies every other source entry through unchanged with `raw_copy_file` (compressed bytes,
compression method, and timestamp preserved) except an entry named in the OPF replacement or the binary-replacement map,
then appends any additions. The whole result is written to a fresh `tempfile::NamedTempFile` in the source file's own
directory and persisted over the source path with an atomic rename; any failure before that final persist leaves the
source path completely untouched.

## Interfaces and dependencies

The public entry point is `validate_and_repair(path: &Path) -> Result<ValidationReport, EpubError>`, synchronous, and
documented as intended for a caller to run inside `tokio::task::spawn_blocking`. `ValidationReport` carries `issues`
(every finding, in discovery order), `outcome`, `accessibility_metadata`, `opf_data`, and `has_usable_embedded_cover`
(forced `false` alongside `None` accessibility metadata and OPF data on a `Quarantined` outcome, whether or not Layer 5
ran). This function is not read-only: any call that finds a `Repaired`-severity issue mutates the file at `path` in
place before returning, including the Writeback pipeline subject's second, post-write call, whose nominal purpose is
only to check for a regression against the pre-write outcome; that call can itself trigger a repack before the caller
hashes the file, so the hash the caller ultimately records reflects whatever this subject's own repair pass produced,
not necessarily the exact bytes the caller's own rewrite wrote.

Beyond the entry point, three functions are reused directly by callers outside this subject's own internal call graph:
`zip_layer::read_entry_from_bytes` and `repack::with_modifications` are both called by the Writeback pipeline subject on
bytes and paths it holds itself, independently of `validate_and_repair`; and `cover_layer::find_cover_href` is exported
specifically so that cover extraction owned by the Design "Covers" locates the same cover this subject's Layer 5 checks,
since any divergence between the two is a silent correctness hazard. `is_safe_path` is likewise called directly by
sibling-path resolution owned by the Design "Covers", re-validating a joined path built from attacker-controlled SVG
content rather than trusting that the path it was joined from was already safe.

No `ValidationReport` or `Issue` list is ever persisted as a whole. The Design "Ingestion pipeline" extracts four fields
from the report it receives: `outcome` (mapped to the `validation_status` column), `accessibility_metadata`,
`has_usable_embedded_cover` (mapped to `has_embedded_cover`), and `opf_data`, which it hands to the metadata extractor
and, from there, to the metadata-based file rename, rather than storing it raw; every other caller, including on-demand
cover extraction in the Design "Covers", re-runs the relevant layers fresh from the file on disk rather than consulting
a stored result.

## Data and state

This subject holds no state beyond a single validation call's own local buffers, and writes nothing but the one file at
the `path` it was given, in place, only on a `Repaired` outcome. The bounds enforced across the five layers, all named
constants in `mod.rs` or `zip_layer.rs`:

| Constant | Value | What it bounds |
| -------- | ----- | -------------- |
| `MAX_ARCHIVE_BYTES` | 2 GB (same value as `MAX_AGGREGATE_UNCOMPRESSED_BYTES`) | Raw file size, checked by `stat` before the file is read at all |
| `MAX_ZIP_ENTRIES` | 20,000 | Central-directory entry count, checked twice: from the end record's declared count before any header is parsed, and again as a counted backstop during iteration |
| `MAX_ENTRY_UNCOMPRESSED_BYTES` | 500 MB | One entry's declared uncompressed size |
| `MAX_AGGREGATE_UNCOMPRESSED_BYTES` | 2 GB | The running sum of every entry's declared uncompressed size |
| `MAX_SPINE_ITEMS` | 500 | Spine idref count; over the cap skips XHTML validation for the whole spine, not only the excess |
| `MAX_EOCD_SEARCH_SPACE` | 65,557 bytes | How far back from the end of the file the end-of-central-directory locator searches (the fixed 22-byte record plus the format's maximum 65,535-byte comment) |
| `MIMETYPE_CONTENT_PROBE_CAP` | 64 bytes | How much of the `mimetype` entry's content is read to check it against `application/epub+zip` |
| the per-entry extractability probe cap | `min(declared_size + 1, 4096)` bytes | How much of an entry is decompressed during Layer 1's own lying-directory check |

The per-entry and aggregate caps bound the *declared* size a central-directory record carries, not a verified actual
size. Layer 1's own probe only catches a declared-size lie for an entry small enough that `declared + 1` is at most
4,096 bytes; above that, the probe cap itself is the limiting factor and a larger lie is not distinguished from a
truthful declaration at this layer. The bound that verifies an entry's actual size against its true content is
`read_entry`/`read_entry_from_bytes`, used by every layer past Layer 1 to fetch entry bytes: it caps the read at
`MAX_ENTRY_UNCOMPRESSED_BYTES + 1` bytes and requires the wrapped CRC-32 verification over that capped read to succeed,
returning nothing on any mismatch. Layer 1's own per-entry probe reads through a plain, unverified reader; it never
checks the CRC-32 the central directory declares. Consequently, an entry that decompresses without an I/O error and
whose true size (up to the probe cap) matches its declared size, but whose content does not match its declared CRC-32,
passes Layer 1 entirely. When Layers 2 to 5 subsequently need that entry's bytes through `read_entry`, the CRC check
there fails and the entry is treated as unreadable, silently, by whichever of the gaps described above applies: absent
for `container.xml` (repaired via regeneration) or a `None` `OpfData` for the OPF itself (not repaired, not flagged, and
potentially still an overall `Clean` outcome if nothing else fired).

## Runtime behaviour

**A structurally clean EPUB.** Every Layer 1 check passes, the `mimetype` entry meets all four OCF rules, Layer 2 finds
and reads `container.xml`, Layer 3 parses the OPF with every manifest href safe and every spine idref resolved, Layer 4
finds no encoding or well-formedness problem within the spine cap, and Layer 5 finds a usable cover or none declared. No
issue is recorded; the outcome is `Clean`, and the file is never touched.

**An EPUB whose only problem is a non-conformant `mimetype` entry.** Layer 1 records one or more `Repaired`
`InvalidMimetype` findings (for example, the entry is Deflated and not first) and nothing else. `has_repairable` is
true, so `repair::repackage` runs: no broken spine refs, no encoding fixes, and no missing container mean the only
effective mutation is `repack::with_modifications`'s own unconditional compliant-`mimetype`-first rewrite. The repacked
file is persisted atomically over the source, and a second validation pass over the same path finds no `InvalidMimetype`
issue and an outcome of `Clean`.

**An EPUB with a broken spine reference.** Layer 3 removes the dangling idref from `spine_idrefs` and records a
`Repaired` `BrokenSpineRef`. `repair::repackage` rewrites the OPF, removing the matching `<itemref>` element (matched by
a depth counter rather than a boolean, so a malformed OPF with nested `itemref` elements cannot prematurely clear the
skip state), and the repacked archive is persisted atomically. If the same run's Layer 1 also found a non-conformant
`mimetype` entry, both fixes land in the same repack.

**A `container.xml` entry that cannot be read at all, with a discoverable `.opf` file in the archive.** Layer 2 records
a `Repaired` `MissingContainer` with the discovered candidate path. `repair::repackage` regenerates
`META-INF/container.xml` from that candidate (escaping the path for safe XML interpolation) and adds it to the repacked
archive; Layer 3 onward already ran against the regenerated path within the same validation pass, since Layer 2 returns
the candidate immediately once it is confirmed safe. If no `.opf` file exists anywhere in the archive, the same
`Repaired` `MissingContainer` issue is still recorded (its severity does not depend on whether a candidate was found),
the outcome is still `Repaired`, but `repackage` adds no container addition (there is no candidate to regenerate from),
and the only effective change to the archive is again the unconditional `mimetype` rewrite.

**An archive shaped to exhaust memory or CPU.** An end-of-central-directory record declaring more entries than
`MAX_ZIP_ENTRIES` is rejected before any header is parsed. A central directory that in fact holds more entries than it
declared is caught by the counted-iteration backstop at the same cap, regardless of the declared count. An entry whose
declared uncompressed size, or whose running aggregate, exceeds the per-entry or aggregate cap is rejected before it is
decompressed. Any of these is `Irrecoverable`; `validate_and_repair` returns `Quarantined` immediately after Layer 1,
without running Layer 2 or any layer after it.

**An EPUB the check rejects outright.** A path-traversal entry name, a duplicate entry name, an encrypted entry, an
unsupported compression method, data preceding the first entry, or a corrupt central directory each produce an
`Irrecoverable` issue in Layer 1 and an immediate `Quarantined` outcome. An unsafe `OPF` path, whether extracted from
`container.xml` or discovered by the regeneration scan, produces the only `Irrecoverable` issue Layer 2 can raise, with
the same immediate `Quarantined` result.

## Failure and recovery

`EpubError` has four variants. `Io` covers a filesystem read failure on the source path itself. `TempFile` covers a
failed atomic persist during repack, reachable from `repair.rs`'s own `temp.persist(path)` call. `Xml` wraps a
`quick_xml::Error` by `#[from]`, but no code path in the module constructs one: `repair.rs`'s own OPF rewriter
(`rewrite_opf_remove_broken_spine`) discards every `write_event` failure with a warning instead of propagating it, and
falls back to the original, unmodified bytes on a read failure; the variant exists in the type but nothing in the code
produces it. `Zip` is documented as a `zip`-crate error such as a corrupt central directory, but Layer 1's own
structural findings, corrupt central directory included, are always represented as `Irrecoverable` issues inside a
successful `Ok(ValidationReport)`, never as this error variant; in the code, `Zip` surfaces from
`repack::with_modifications`'s own use of the `zip` crate, both re-opening the source archive and from `start_file`,
`raw_copy_file`, and `finish` on the writer it builds, or from a `zip::result::ZipError::FileNotFound` value `repair.rs`
constructs by hand as a generic not-found sentinel on a path that in fact used `rawzip`, not the `zip` crate, to read
the entry that could not be found.

A `Repaired`-severity issue that repair cannot actually apply is not always visible in the outcome. If
`repair::repackage` itself fails partway (an unreadable OPF entry needed for a spine rewrite, a `ZipWriter` failure, a
failed atomic persist), the error propagates out of `validate_and_repair` as `Err(EpubError)` rather than as a
`ValidationReport`; the file at `path` is left completely untouched, since the failure happens before the atomic
persist. The Design "Ingestion pipeline" treats this the same as any other validator crash: it stores
`validation_status = failed` and still ingests the file with its original, unrepaired bytes, rather than quarantining it
or leaving it unhandled. Separately, an `EncodingMismatch` fix on a non-OPF entry can be recorded as `Repaired` at
validation time yet silently not applied at repair time: `repackage` re-reads and re-transcodes the entry independently
of the check that decided the fix was safe, and if that re-derivation fails for any reason, the entry is simply left out
of the repack's binary replacements and copied through unchanged, while the outcome the caller sees is still `Repaired`.

A repacked archive can end up with two entries sharing the same name in at least one path: a `MissingContainer` repair
adds a regenerated `META-INF/container.xml` as a new entry, and the raw-copy loop that carries every other source entry
through unchanged has no case that skips an entry sharing an addition's name, so it also copies the original entry
through when that original `container.xml` entry exists in the archive but fails Layer 1's CRC-and-size-verified read (a
genuinely absent entry leaves nothing for the raw-copy loop to carry through).

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
