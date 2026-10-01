---
severity: medium
surfaces: [developer, server-operator, end-user, ci]
adopted: 2026-09-30
adopted-because: Library ownership and relative locations are delivered before the producer and cover migrations.
lift-when-class: internal-refactor
lift-when: Ingestion, writeback and covers use recorded library-relative locations through directory capabilities, and the full backend suite and scoped preflight pass.
---

# Library location pipelines are incomplete

Manifestations require a library identity and a canonical relative path. Downloads, writeback and request cover-source
reads use owning-library capabilities. Ingestion persistence and cover-cache operations remain incomplete, so the
checkpoint cannot support end-to-end library management and stays draft. Tests and ordinary gates remain active.

## Owners and lift conditions

- `services::ingestion::orchestrator::commit_ingest` omits the required library identity and supplies a full destination
  path. Migrate `process_file`, `scan_once` and `run_watcher` to owned library, ingestion and quarantine capabilities;
  persist the actual relative destination with its library identity and pass that location to cover warming. The
  `scan_once_processes_pdf_end_to_end`, `scan_once_processes_epub_end_to_end`, duplicate-scan and mixed-cleanup tests
  must pass with the new contract, alongside the admin scan-route tests. A failed manifestation insert removes the
  library copy but retains the source and can leave destination directories. Each subsequent scan processes that source
  again and appends another failed ingestion attempt; the existing path-based duplicate check cannot match relative
  records.
- `services::covers::cache` and cached response-file opening retain ambient cache paths. Source extraction consumes
  opened library files; ingestion warming opens its known final copy, while recorded-location producer adoption remains
  with ingestion. Migrate cache publication and response opening to capabilities and complete that warming handoff.
  Authenticated OPDS cover tests and `warm_one_` tests must retain their existing behaviour.

Remove this entry when these owners are migrated, ingestion-to-download and restart checks pass, and both the full
backend suite and scoped preflight pass. No compatibility adapter is provided.
