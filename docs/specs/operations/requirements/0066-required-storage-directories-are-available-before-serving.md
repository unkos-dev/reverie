---
type: REQ
profile-version: 1
id: "REV-REQ-0066"
title: "Required storage directories are available before serving"
governed-by:
  - "REV-ADR-0051"
---

# Required storage directories are available before serving

## Statement

When starting the normal server, Reverie MUST establish access to all configured library and ingestion directories
before administrator seeding, worker startup or accepting requests. Empty or relative root configuration, missing
directories and non-directory roots MUST fail startup with an operator-facing error rather than select the process
directory or defer acquisition until a request.

## Rationale

A server that accepts work without its storage can report misleading missing-file results or fail after work has begun.
Provisioned roots make deployment storage readiness a startup precondition.

## Acceptance criteria

- Both defaults are absolute; explicitly empty and relative values are rejected through the configuration boundary.
  Checked by the `library_storage_config_` tests.
- A missing or non-directory value for any required root fails capability acquisition; provisioned roots open
  successfully. Checked by `library_storage_root_all_roots_required` and `library_storage_root_non_directory_refused`.
- Inspection of `run` confirms storage acquisition precedes administrator seeds, the listener and workers.
- A migration-only invocation opens no storage root and remains usable before storage is provisioned. Checked by
  inspection of `run_migrate` and the normal migration command.
