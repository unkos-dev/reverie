---
severity: medium
surfaces: [developer, server-operator, end-user, ci]
adopted: 2026-09-30
adopted-because: Library ownership and relative locations are delivered before the producer and cover migrations.
lift-when-class: internal-refactor
lift-when: Ingestion, writeback and covers use recorded library-relative locations through directory capabilities, and the full backend suite and scoped preflight pass.
---

# Library location pipelines are incomplete

Manifestations require a library identity and a canonical relative path. Downloads, ingestion, writeback and request
cover-source reads use owning-library capabilities. Cover-cache publication and response opening remain incomplete.
Tests and ordinary gates remain active.

## Owners and lift conditions

- `services::covers::cache` and cached response-file opening retain ambient cache paths. Source extraction consumes
  opened library files, and ingestion warming opens its known final copy. Migrate cache publication and response opening
  to capabilities. Authenticated OPDS cover tests and `warm_one_` tests must retain their existing behaviour.

Remove this entry when these owners are migrated, ingestion-to-download and restart checks pass, and both the full
backend suite and scoped preflight pass. No compatibility adapter is provided.
