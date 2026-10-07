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
queue with a membership set and one active future. Blocking closures return results and never schedule work themselves.

The copier walks source parents without following symlinks, opens a regular source and streams it into independently
owned staging beneath the library root. EPUB validation operates on that candidate. Publication moves it directly on the
same filesystem; a filesystem boundary requires independently verified staging on the actual destination filesystem.
Final publication uses the delivered contained no-overwrite primitive. Metadata naming and suffix selection consume
shared path exclusion; final registration reserves the manifestation's path claim in its transaction.

## Interfaces and dependencies

`POST /api/v1/ingestion/scan` requires the existing admin scope, role and browser CSRF boundary. It returns HTTP 202
after discovery, with queued, deferred and suppressed counts and `/api/v1/dashboard/activity` as the monitor. The
response describes discovery, without promising a separately attributable batch or completed imports.

`LibraryFiles` supplies immutable ingestion and library directory capabilities. The ingestion database pool reads
current inputs and linked attempts and writes works, manifestations, metadata drafts and claims. Settings use the
existing singleton row, API and monotonic live cache. Tokio supplies cancellation, clock control and the deadline queue;
cap-tempfile owns independent candidates.

Normal application startup requires `DATABASE_URL_INGESTION` for the dedicated `reverie_ingestion` role. A missing,
empty or whitespace-only value fails before database pools, workers or serving, with an error naming the variable. The
application-role DSN is never substituted. One-shot administrative commands retain their own credential requirements.

## Data and state

`ingestion_inputs` stores a byte-preserving ingestion-relative path, full source fingerprint, generation, current
status, reason, optional work link, observation and retry-reset times, completion time and removal cause. A partial
unique index permits one present input per path; a removed input keeps its identity and history while a later arrival
gets a new record. Fingerprints include device, `inode`, size, modification time and change time.

New `ingestion_jobs` link the captured input and generation and carry a typed attempt outcome separately from the shared
job status. Old unlinked history remains readable. Terminal attempt and corresponding input updates share one
transaction, guarded by captured generation and removal state. Imported outcomes share the manifestation, work, metadata
and claim transaction. A stale attempt cannot overwrite a newer generation.

Linked attempts also carry unresolved publication evidence: library identity, exact relative name, candidate device and
`inode`, accepted SHA-256 and non-negative size. These five fields are present together, with optional paired failure
class and reason. An acknowledged evidence transaction precedes visibility. Registration clears evidence in the same
transaction as the imported outcome. A partial keyset index supports 100-row recovery pages; unresolved inputs are
excluded from new attempts.

The accepted-format set permits only EPUB and defaults to EPUB; an empty set accepts no format. Imported cleanup
defaults to enabled and duplicate cleanup to disabled. Other extensions, including sidecars, are independent
not-accepted inputs. Rejection and non-acceptance have no deletion option. Changes to accepted formats are observed from
the existing live settings cache. Before serving or worker startup, the settings service conditionally seeds these three
fields from validated configuration. The internal `ingestion_seeded` marker prevents restart overwrite; supplying any
ingestion setting marks the row as saved, including empty acceptance or default values. Other settings saves leave an
unseeded row eligible at any revision. The marker defaults to false regardless of existing revision. The seed advances
revision and returns the winning row from one transaction; failure stops startup.

## Runtime behaviour

Discovery enumerates through ingestion capabilities and ignores names beginning with a dot and exactly `Thumbs.db`.
Directories and source parents are opened without following symlinks. Startup and admin scans enumerate the full tree;
watcher signals inspect affected names and expand affected directory trees. Finalisation refreshes only its input, and
live acceptance changes reclassify persisted inputs without another filesystem scan. Queries and observation writes use
bounded batches; unchanged observations perform no update. Only confirmed absence marks an input removed; a partial or
unreadable discovery does not infer absence. Rename creates a new input identity and retains the old record and attempt
history. Cancellation, completion and watchdog ticks are serviced ahead of queued watcher traffic.

An input needs ten seconds of observed unchanged size and modification time. Repeated signals coalesce; an admin scan
does not bypass readiness. A changed fingerprint creates a new generation and invalidates suppression. Source changes
during acquisition discard the candidate and schedule another check without waiting for another notification. Unchanged
rejected generations remain suppressed across restart.

An attempt hashes and streams the same opened source object in 64 KiB chunks, verifies the captured fingerprint and
streamed digest, and validates independent candidate bytes. Content duplication uses the immutable ingestion hash and
links the existing work. A path collision chooses a suffix and never establishes content equality. Validation precedes
final publication. The recorded final library identity and relative path, accepted hash and length describe the bytes
actually published; the ingestion hash remains immutable. Unchanged validation reuses acquisition evidence; repair
reuses its finalised hash and size. Uncertain repair, independent destination copies and recovery verify actual bytes.
Cover warming opens that recorded location and passes the accepted handle, recorded library identity and shared
`LibraryFiles` into the detached thumbnail task. Cache authority follows that identity. Final-name probes use the opened
actual parent before evidence persistence.

Cleanup reads the current settings snapshot and rechecks the current generation and source fingerprint. A successful
source deletion records automatic cleanup and preserves attempt history. Local deletion failure preserves the completed
outcome and releases ordinary attempt ownership. A successful deletion whose database update fails retains its live
receipt for recommit. Absence without that receipt records `unattributed_disappearance`, including after restart;
absence alone cannot identify who removed a source. Historical causes remain readable. Upward pruning starts at that
deletion's parent and stops at the root. Every remaining entry must be a regular `.DS_Store` or `Thumbs.db` file before
pruning; other hidden files, sidecars, symlinks and directories preserve the directory. Ordinary empty-directory removal
preserves entries arriving during pruning. Unrelated empty directories are untouched.

## Failure and recovery

An irrecoverable EPUB discards the candidate, preserves the original and records rejection without a manifestation or
quarantine copy. A validator execution error retains the separate failed-validation contract: accepted bytes can be
registered with validation status `failed`.

Each attempt has a child of the worker's shutdown token, a progress counter and typed streaming, validation and
publication phases. Source hashing and streaming check cancellation between chunks and increment progress after each
one. Phase transitions reset idle observation. There is no total-duration deadline. After 120 seconds without progress
in streaming, the owner requests cancellation. Validation and publication are protected from idle cancellation; neither
clears a previous cancellation request. Shutdown remains effective between phases. The candidate remains owned until its
blocking closure returns, then cancellation preserves the source and disposes of candidate bytes. Stall warnings begin
after five minutes without progress and repeat every five minutes until return. A kernel-blocked read cannot observe
cancellation.

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
  automatic retry. Exhausted suffix selection and confirmed database constraint or protocol errors also need change;
  serialization and deadlock errors are transient.

Retry counts use linked history since the persisted reset marker, excluding shared and interrupted outcomes. New
generation, startup reconstruction and admin scans reset exhausted and needs-change inputs, subject to readiness;
unchanged rejection suppression is preserved. These timings and budgets are internal constants.

Startup reconciles unresolved publication evidence under the session advisory lock before reclaiming interrupted
attempts. It first checks this exact attempt's imported outcome, preserving committed files even after writeback changes
their location or bytes. Otherwise, under path exclusion, confirmed absence clears evidence; an unclaimed name matching
identity, hash and size is removed and its parent synced before evidence clears. Foreign content or another owner is
preserved and records needs-change. Unavailable ownership, read, removal or required sync retains evidence and its
diagnostic without a failed terminal outcome. Healthy unrelated inputs continue; the affected input is reconsidered on
startup, admin scan, dependency resumption or a relevant signal. Shared failures use the global probe schedule, and a
fresh shared failure during recommit keeps the pause.

Shutdown records no new terminal outcome. The shared 30-second drain budget does not impose an attempt duration deadline
or cancel a kernel read. Acknowledged evidence survives close/sync errors, cancellation, panic and ambiguous
registration. Live recovery may register retained accepted metadata; startup disposes of a verified unregistered file
and reclaims its attempt without consuming transient budget. Evidence is resolved before a replacement name is selected.

Disposal removes destination parents created by the attempt only while their identity matches, they remain empty and no
committed owner requires them. Opened-parent removal stops at retained entries, filesystem boundaries and the library
root. Pre-existing directories and library metadata files are preserved.

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
