---
severity: medium
surfaces: [developer, server-operator, end-user, ci]
adopted: 2026-09-30
adopted-because: Library ownership and relative locations are delivered before the producer and cover migrations.
lift-when-class: internal-refactor
lift-when: Ingestion, writeback and covers use recorded library-relative locations through directory capabilities, and the full backend suite and scoped preflight pass.
---

# Library location pipelines are incomplete

Manifestations require a library identity and a canonical relative path. OPDS downloads use that recorded location, but
the ingestion, writeback and cover owners still use their previous filesystem interfaces. This checkpoint cannot support
end-to-end library management and must remain draft. Tests and ordinary gates remain active.

## Owners and lift conditions

- `services::ingestion::orchestrator::commit_ingest` omits the required library identity and supplies a full destination
  path. Migrate `process_file`, `scan_once` and `run_watcher` to owned library, ingestion and quarantine capabilities;
  persist the actual relative destination with its library identity and pass that location to cover warming. The
  `scan_once_processes_pdf_end_to_end`, `scan_once_processes_epub_end_to_end`, duplicate-scan and mixed-cleanup tests
  must pass with the new contract, alongside the admin scan-route tests. A failed manifestation insert removes the
  library copy but retains the source and can leave destination directories. Each subsequent scan processes that source
  again and appends another failed ingestion attempt; the existing path-based duplicate check cannot match relative
  records.
- `services::writeback::orchestrator::{load_snapshot, run_once, path_rename_step}` reads recorded paths as ambient paths
  and writes full relocation destinations. Migrate source access, replacement, relocation and compensation to the owning
  library capability. The `run_once_finds_non_default_opf_and_updates_hash`,
  `ingestion_file_hash_immutable_across_writeback_chain`, `run_once_renames_file_to_template_path`, collision and cover
  writeback tests must pass using recorded relative locations. A relative source resolved against the process directory
  can produce a terminal `file_missing` skip. Skipped jobs never retry, including after source resolution is migrated;
  replaying those edits requires an explicit operational action.
- `services::covers::{get_or_create, spawn_warm_thumb, warm_one}` and `services::covers::cache` retain ambient source
  and cache paths. Migrate EPUB source reads and cache publication to capabilities, including the ingestion warming
  handoff. The authenticated OPDS cover tests and the `warm_one_` tests must pass with the same location contract.

Remove this entry when these owners are migrated, ingestion-to-download and restart checks pass, and both the full
backend suite and scoped preflight pass. No compatibility adapter is provided.
