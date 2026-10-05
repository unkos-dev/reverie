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

A library may reside on a NAS separate from the container host. Destination probing cannot reserve a name, and mounted
storage differs in its support for no-replace rename. The storage decision needs refusal at the relocation commit
without restricting libraries to local disks.

The decision covers persistent library ownership, authority over managed files and how complete replacements reach their
destination. Authentication, row-level security and archive resource limits retain their existing contracts.

## Decision drivers

- Download metadata and bytes describe one opened file, even across directory-entry replacement.
- Recorded locations identify their owning library; links resolving inside that library remain usable.
- Reverie owns managed-library writes and reorganisation; external tools coordinate their changes with it.
- EPUB candidates can be rejected while the original bytes remain untouched.
- Maintained filesystem primitives reduce the replacement and recovery code Reverie owns.
- Durable EPUBs and rebuildable cover caches have different synchronisation needs.
- NAS library roots need relocation through operations the mounted filesystem supports.

## Considered options

- Opened directory capabilities with validated candidate publication through maintained crates.
- Opened capabilities with no-replace rename as the sole relocation primitive.
- Retain canonical path guards and separate path-based filesystem operations.
- Retain raw `tempfile` life cycles and maintain publication, synchronisation and recovery in Reverie.
- Manifestation-local forward relocation recovery under the existing job claim.
- Filesystem move-back compensation after failed location bookkeeping.

## Decision outcome

Chosen option: **opened directory capabilities with validated candidate publication through maintained crates**, because
it assigns filesystem authority to a workflow and separates building a replacement from exposing it to readers.

Use persistent library identities and record each actual file location relative to its owning library. Deployment
configuration supplies absolute roots independently of those identities. Metadata and naming policies propose
destinations; they do not reinterpret recorded locations. This permits independent libraries without committing to an
administration interface or organisation syntax.

Use a concrete `LibraryFiles` boundary with cap-std. Open provisioned library and ingestion roots before serving or
starting workers, then keep the identity-to-root bindings immutable. Classification supports internal symlink targets;
only the selected opened directory grants authority for the file operation. Opened roots identify directory objects, so
external relocation or replacement requires a coordinated restart. Directory existence cannot establish that the
intended filesystem is mounted; deployment ordering remains an operator responsibility.

Choose independent candidate acquisition and retained-source rejection for ingestion. Source handles supply
fingerprints, immutable ingestion hashes and streamed bytes; validation and repair act only on owned candidate bytes.
Irrecoverable content preserves its original and rejection reason without a manifestation or a separate quarantine copy.
Imported and duplicate cleanup are independent outcome options, with deletion restricted to an unchanged source. Moving
originals to quarantine remains a viable separation mechanism, but changes externally shared paths and requires another
growing byte store. Retained originals keep correction with the operator and need explicit retry eligibility.

Record possible ingestion publication on its existing linked attempt before making the final name visible. Persist the
exact library/name, candidate identity, accepted hash and size, and acknowledge that transaction under shared path
exclusion. Clear evidence with the imported outcome and manifestation claim in one transaction. Resolve lost
acknowledgement from that exact attempt before inspecting a name writeback may already have changed. Startup reconciles
evidence before reclaiming interrupted attempts; unavailable read or removal suspends its input while unrelated healthy
inputs continue. Preserve foreign files and committed owners; remove only verified unregistered owned bytes.

Reuse acquisition evidence when validation leaves bytes unchanged and finalised repair evidence when it rewrites them.
Move the owned candidate directly on the same filesystem; stage and independently verify on the actual destination
filesystem for EXDEV. Track only destination parents created by the attempt, removing them on disposal only while their
identity matches, they are empty and no committed owner requires them. Preserve pre-existing folders.

Use streaming, validation and publication phases within the existing progress owner. Only streaming accepts idle
cancellation; protected phases finish without an idle timeout invalidating their result. Earlier shutdown requests stay
effective between phases. Keep the existing drain budget and kernel-read limitation.

The development catalogue is disposable and can be rebuilt and re-ingested. No conversion, legacy location reader or
temporary writer adapter is part of this decision. Startup never resets the database.

For writer adoption, prefer cap-std-ext for durable replacement and cap-tempfile for temporary ownership and cache
publication, subject to qualification of permission preservation and errors after publication. Build, repair, finish,
flush and validate an independent candidate before publishing it once; hash the finalised candidate rather than write
calls that a random-access archive writer may later revise. Durable EPUB replacement syncs the file and parent
directory; rebuildable caches use atomic publication without forced sync.

Keep blocking filesystem and archive work off request threads, within the workflow's concurrency bounds. Preserve
existing queue and database ownership: filesystem publication and a Postgres transaction are separate operations. A
failure after rename can leave published bytes with unconfirmed durability, requiring an explicit error policy rather
than a blind rollback. Atomic replacement does not supply no-overwrite destination selection.

Choose `rustix` no-replace rename on opened parents and base names for relocation, with cap-std hard links only when
rename reports EINVAL or ENOSYS. This preserves refusal on occupied names while supporting storage whose rename flags
are unavailable. The link path syncs the destination parent before source removal and syncs the source parent
afterwards. Permission, collision and ambiguous network errors retain their failure meaning rather than selecting a
different commit operation.

For EXDEV, choose cap-tempfile ownership on the actual destination filesystem, with replacing materialisation confined
to an exclusively owned staging directory and no-overwrite publication to the final name. Preserve bounded copying and
independent destination verification before removing the original source. These maintained primitives avoid another
direct `procfs` dependency or a storage-provider abstraction.

Linux container storage needs contained file access, atomic content replacement, useful sync/error semantics and either
no-replace rename or hard links for relocation. Unsupported operations preserve the source and report failure. This
operations contract does not promise compatibility with every NFS or SMB server. Mount ordering remains the operator's
responsibility. A successful sync after a reported failure does not prove that failed writes became durable.

Choose manifestation-local forward relocation recovery because a recorded pair survives interruption without depending
on the originating job's remaining attempts. Accepted content evidence and exact relocation names share one existing
UPDATE before movement, with destination ownership reserved in the same transaction. Final location, intent clearing and
obsolete-claim release share one transaction afterwards. A visible move with uncertain sync records its destination
while retaining intent. Move-back cannot restore a single atomic filesystem/database outcome and can itself fail or
collide, so failure retains evidence for forward reconciliation instead.

Recovery verifies the exact in-progress job, intent and both names' path claims in a short transaction, then releases
the connection before filesystem work. A fresh short transaction locks the exact claimed job and guards the intent
update when recording the outcome. Holding row locks during hashing, copying or sync would block canonical metadata
edits and occupy a writeback connection without adding exclusion within the supported deployment.

Exclusion relies on the
[restart-bounded single-instance claim](./0018-durable-job-queue-postgres-backed-skip-locked-crash-only.md). The worker
stops claiming on shutdown; aborting it leaves unfinished rows in progress. Runtime shutdown waits for started blocking
work to finish before the process exits, so startup reclaim follows the old mutation's completion or process death. A
multi-instance deployment or runtime shutdown that abandons running blocking work must revisit this protocol. Recovery
adopts verified destination bytes, removes only a verified source and preserves foreign or externally changed files.
Confirmed permanent loss or change clears the corresponding intent with terminal job bookkeeping in one transaction
owned by queue::finish; otherwise clearing after a separate terminal write would release exclusion too early. Unreadable
evidence or unfinished filesystem/SQL recovery retains intent. Ordinary jobs reload and continue; bounded
relocation-only carriers recover intents whose original jobs exhausted their attempts without publishing content again.

Ordinary recovery claims preserve the edit budget. Successful destination finalisation debits one edit in its
transaction before snapshot reload; failed recovery, source restoration and permanent evidence outcomes do not debit an
edit. Relocation carriers retain bounded attempts. Finish reads the durable count, including after interruption or
database failure. Transient reset, sweep and claim failures remain inside the existing worker, retaining its tracked
jobs and retry timer. [Library path ownership](./0052-library-path-ownership-during-publication-and-relocation.md)
records the shared claim constraints and publisher authority.

A startup and five-minute bounded sweep inside the existing enabled worker supplies carriers; five-minute intent retry
spacing limits scheduling opportunities, not operation duration. This choice claims relocation replay safety only.
Cover-sidecar replay, temporary-file scavenging and complete writeback replay remain outside it. Successful re-sync
permits finalisation after a reported sync failure under a weaker guarantee, without proving that failed writes reached
durable storage or adding a third persisted error state.

### Consequences

- Positive: a download's handle supplies both metadata and bytes, and contained opening closes the path lookup gap.
- Positive: candidate validation can reject a rewrite or ingestion without changing externally shared original bytes.
- Positive: maintained crates own contained resolution and replacement mechanics.
- Negative: absolute-link compatibility still needs ambient classification before contained opening.
- Negative: provisioned roots and mount ordering are startup prerequisites; pinned roots require coordinated shutdown
  when storage is unmounted, relocated or replaced.
- Negative: a disposable catalogue rebuild loses development rows; partial pipeline delivery cannot be released.
- Negative: additional dependencies and separate filesystem/database failure handling remain maintenance costs.
- Negative: hard-link relocation can leave two names after interruption or removal failure; recovery requires readable
  size/hash evidence and successful filesystem and SQL finalisation.
- Negative: reported sync errors retain a weaker durability guarantee even after successful re-sync; a bounded sweep and
  queue backlog can delay recovery beyond five minutes.
- Negative: abrupt exit can leave bare UUID staging directories visible on a NAS share; no scavenging is provided.
- Negative: rejected and operationally failed originals remain on disk until corrected or removed by the operator;
  dedicated management and retention controls are separate work.
- Negative: a kernel-blocked read cannot observe cooperative cancellation and can delay shutdown; outages pause new
  ingestion while completed results await successful outcome commits.

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

### Opened capabilities with no-replace rename as the sole relocation primitive

- Positive: one atomic relocation operation avoids an interrupted two-name state.
- Negative: storage lacking support for the rename flag cannot relocate files even when it supports contained hard
  links.

### Manifestation-local forward relocation recovery

- Positive: exact names and existing content evidence survive interruption, under the existing database claim.
- Negative: unreadable storage keeps intent open; verification and periodic carrier scheduling remain application work.
  An unresolved storage failure blocks later writeback for that book and continues creating carrier rows, roughly 28 per
  day with the default ten attempts and five-minute retry spacing; queue and operation delays can reduce that rate.

### Filesystem move-back compensation

- Positive: a successful move-back restores the location the row already names.
- Negative: interruption can skip compensation, and a failed or colliding move-back leaves no durable reconciliation
  owner. It cannot make filesystem and SQL operations atomic.

## More information

OPDS downloads, writeback relocation and initial ingestion publication apply the capability boundary. Ingestion
validates independently owned staged bytes before publication and reconciles input, attempt and manifestation outcomes.
Cover-cache publication remains incomplete. No representative NAS behaviour or server flush guarantee is established by
source inspection or injected error-code tests.

- [cap-std capability model](https://github.com/bytecodealliance/cap-std/blob/v4.0.3/README.md).
- [cap-std-ext replacement implementation](https://github.com/coreos/cap-std-ext/blob/v5.1.2/src/dirext.rs).
- [cap-tempfile ownership and replacement](https://github.com/bytecodealliance/cap-std/blob/v4.0.3/cap-tempfile/src/tempfile.rs).
- [NamedTempFile persistence](https://docs.rs/tempfile/3.27.0/tempfile/struct.NamedTempFile.html).
- [No-replace rename API](https://docs.rs/rustix/1.1.5/rustix/fs/fn.renameat_with.html).
- [Linux rename semantics](https://man7.org/linux/man-pages/man2/rename.2.html).
- [Linux hard-link semantics](https://man7.org/linux/man-pages/man2/link.2.html).
