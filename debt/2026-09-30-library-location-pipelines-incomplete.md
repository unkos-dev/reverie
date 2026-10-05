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
reads use owning-library capabilities. Ingestion persists explicit library-relative locations and claims; source
acquisition, quarantine and cover-cache operations remain incomplete, so the checkpoint cannot support end-to-end
library management and stays draft. Tests and ordinary gates remain active.

## Owners and lift conditions

- `process_file`, `scan_once` and `run_watcher` retain ambient source acquisition and quarantine operations. Migrate
  those operations to ingestion and quarantine capabilities. Library publication, recorded-location persistence and the
  opened cover-warming source already use the owning library. The `scan_once_processes_pdf_end_to_end`,
  `scan_once_processes_epub_end_to_end`, duplicate-scan and mixed-cleanup tests must pass with the new contract,
  alongside the admin scan-route tests.
- `services::covers::cache` and cached response-file opening retain ambient cache paths. Source extraction consumes
  opened library files, and ingestion warming opens its known final copy. Migrate cache publication and response opening
  to capabilities. Authenticated OPDS cover tests and `warm_one_` tests must retain their existing behaviour.

Remove this entry when these owners are migrated, ingestion-to-download and restart checks pass, and both the full
backend suite and scoped preflight pass. No compatibility adapter is provided.
