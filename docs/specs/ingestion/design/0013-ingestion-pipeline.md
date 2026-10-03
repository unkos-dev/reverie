---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0013"
title: "Ingestion pipeline"
satisfies:
  - "REV-REQ-0043"
governed-by:
  - "REV-ADR-0020"
  - "REV-ADR-0052"
---

# Ingestion pipeline

## Purpose and boundaries

Ingestion turns accepted files beneath opened ingestion authority into manifestations beneath an opened library. One
coordinator owns observation, readiness, an attempt, retry scheduling and source cleanup. EPUB validation owns archive
inspection and repair; metadata services own drafts and work matching; the shared path-claim primitive owns destination
exclusion. Input-management and retention interfaces are outside this pipeline.

## Structure

`CoordinatorHandle` in `AppState` sends discovery commands to `run_watcher`. Startup, watcher notifications, manual
scans and deadlines feed the same owner. The owner maintains current input snapshots, a `DelayQueue`, its keys, a ready
queue and one active future. Blocking closures return results and never schedule work themselves.

The copier walks source parents without following symlinks, opens a regular source and streams it into independently
owned destination-parent staging. EPUB validation operates on that candidate. Final publication uses the delivered
contained no-overwrite primitive. Metadata naming and suffix selection consume shared path exclusion; final registration
reserves the manifestation's path claim in its transaction.

## Interfaces and dependencies

`POST /api/v1/ingestion/scan` requires the existing admin scope, role and browser CSRF boundary. It returns HTTP 202
after discovery, with queued, deferred and suppressed counts and `/api/v1/dashboard/activity` as the monitor. The
response describes discovery, without promising a separately attributable batch or completed imports.

`LibraryFiles` supplies immutable ingestion and library directory capabilities. The ingestion database pool reads
current inputs and linked attempts and writes works, manifestations, metadata drafts and claims. Settings use the
existing singleton row, API and monotonic live cache. Tokio supplies cancellation, clock control and the deadline queue;
cap-tempfile owns independent candidates.

## Data and state

`ingestion_inputs` stores a byte-preserving ingestion-relative path, full source fingerprint, generation, current
status, reason, optional work link, observation and retry-reset times, completion time and removal cause. A partial
unique index permits one present input per path; a removed input keeps its identity and history while a later arrival
gets a new record. Fingerprints include device, `inode`, size, modification time and change time.

New `ingestion_jobs` link the captured input and generation and carry a typed attempt outcome separately from the shared
job status. Old unlinked history remains readable. Terminal attempt and corresponding input updates share one
transaction, guarded by captured generation and removal state. Imported outcomes share the manifestation, work, metadata
and claim transaction. A stale attempt cannot overwrite a newer generation.

The accepted-format set permits only EPUB and defaults to EPUB; an empty set accepts no format. Imported cleanup
defaults to enabled and duplicate cleanup to disabled. Other extensions, including sidecars, are independent
not-accepted inputs. Rejection and non-acceptance have no deletion option. Changes to accepted formats are observed from
the existing live settings cache.

## Runtime behaviour

Discovery enumerates through ingestion capabilities and ignores names beginning with a dot and exactly `Thumbs.db`.
Directories and source parents are opened without following symlinks. Full discovery reconciles current records in
bounded pages and set-based updates. Only confirmed absence marks an input removed; a partial or unreadable discovery
does not infer absence. Rename creates a new input identity and retains the old record and attempt history.

An input needs ten seconds of observed unchanged size and modification time. Repeated signals coalesce; an admin scan
does not bypass readiness. A changed fingerprint creates a new generation and invalidates suppression. Source changes
during acquisition discard the candidate and schedule another check without waiting for another notification. Unchanged
rejected generations remain suppressed across restart.

An attempt hashes and streams the same opened source object in 64 KiB chunks, verifies the captured fingerprint and
streamed digest, and validates independent candidate bytes. Content duplication uses the immutable ingestion hash and
links the existing work. A path collision chooses a suffix and never establishes content equality. Validation precedes
final publication. The recorded final library identity and relative path, accepted hash and length describe the bytes
actually published; the ingestion hash remains immutable. Cover warming opens that recorded location.

Cleanup reads the current settings snapshot and rechecks the current generation and source fingerprint. A successful
source deletion records automatic cleanup and preserves attempt history. Upward pruning starts at that deletion's parent
and stops at the root. Every remaining entry must be a regular `.DS_Store` or `Thumbs.db` file before pruning; other
hidden files, sidecars, symlinks and directories preserve the directory. Ordinary empty-directory removal preserves
entries arriving during pruning. Unrelated empty directories are untouched.

## Failure and recovery

An irrecoverable EPUB discards the candidate, preserves the original and records rejection without a manifestation or
quarantine copy. A validator execution error retains the separate failed-validation contract: accepted bytes can be
registered with validation status `failed`.

Each attempt has a child of the worker's shutdown token and a progress counter. Source hashing and streaming check
cancellation between chunks and increment progress after each one. There is no total-duration deadline. After 120
seconds without progress the owner requests cancellation; validation and no-overwrite publication finish their current
operation. The candidate remains owned until its blocking closure returns, then cancellation preserves the source and
disposes of candidate bytes. Stall warnings begin after five minutes without progress and repeat every five minutes
until return. A kernel-blocked read cannot observe cancellation.

Operational failures are classified before retry:

- Shared dependency means database unavailability or a failed opened-root probe, including EIO, ENOTCONN, ESTALE,
  EHOSTDOWN and destination-root ENOSPC. New attempts pause. Probes run after 30 seconds, one minute, two minutes and
  then every five minutes until healthy. Observation and readiness continue where ingestion authority permits. Completed
  results survive failed outcome commits and are recommitted on that schedule. Pause and resume each log once; shared
  failures consume no input budget.
- With healthy roots, input EIO, idle cancellation, panic, unchanged-source hash mismatch and otherwise unlisted I/O
  failures are transient. Hash mismatch first rechecks the source fingerprint; source change takes the scheduled-check
  path without a failure. Unlisted errors record their kind. Five retries after the initial attempt wait five minutes,
  30 minutes, two hours, eight hours and 24 hours, without `jitter`. The sixth transient failure exhausts the generation
  without a deadline.
- Source EACCES or EPERM, ENAMETOOLONG, non-regular files, ELOOP and unrepresentable paths need change and have no
  automatic retry.

Retry counts use linked history since the persisted reset marker, excluding shared and interrupted outcomes. New
generation, startup reconstruction and admin scans reset exhausted and needs-change inputs, subject to readiness;
unchanged rejection suppression is preserved. These timings and budgets are internal constants.

Startup reclaims interrupted attempts under the session advisory lock. Shutdown cancellation records no terminal
outcome. The shared 30-second drain budget does not impose an attempt duration deadline or cancel a kernel read. Failed
and ambiguous registration retain accepted evidence until ownership is known: committed manifestations are adopted, and
only a verified unregistered owned name can be removed before another publication. Foreign files and committed owners
are preserved.

## Security and operations

Library and ingestion capabilities are acquired before serving; no quarantine authority exists. Source acquisition
refuses symlink parents and non-regular files. Independent staging prevents repair from mutating a source shared through
a hardlink. Shared path exclusion protects publication, registration and cleanup across ingestion and writeback.

The coordinator logs pause, resume, failures and stalled attempts. The activity endpoint exposes attempt history.
Operators manage retained originals on disk and request another scan after correction. An outage can strand eligible
inputs until probes succeed; a blocked read can delay process shutdown. Abrupt termination can leave temporary
directories, and no scavenging is supplied. Local tests do not establish NAS server durability.

## More information

[Ingestion readiness and retries](../../../../backend/README.md#ingestion-readiness-and-retries) documents the operator
contract.
