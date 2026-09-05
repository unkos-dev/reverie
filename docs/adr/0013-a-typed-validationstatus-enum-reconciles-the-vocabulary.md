---
type: ADR
profile-version: 1
id: "REV-ADR-0013"
title: "A typed ValidationStatus enum reconciles the vocabulary"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-05-28"
decision-makers:
  - "John Unkovich"
---

# A typed ValidationStatus enum reconciles the vocabulary

## Context and problem statement

`validation_status` is the last Postgres enum still read into Rust as a raw `String`. Every sibling enum
(`user_role`, `ingestion_status`, `enrichment_status`, `manifestation_format`, `api_cache_kind`) was already migrated
to a `sqlx::Type` Rust enum; this decision closes that series.

The blocker is not the type machinery; it is a vocabulary collision surfaced during review of the `GET /api/books`
read path. Three vocabularies describe the same concept and disagree, and the domain enum
(`backend/src/services/epub/mod.rs`) is authoritative over the other two:

| Source          | Vocabulary                                        | Where                            |
| --------------- | ------------------------------------------------- | -------------------------------- |
| Domain enum     | `Clean` / `Repaired` / `Degraded` / `Quarantined` | `ValidationOutcome`              |
| Postgres enum   | `pending` / `valid` / `repaired` / `degraded`     | `validation_status` column       |
| Frontend sketch | `clean` / `repaired` / `degraded` / `quarantined` | pre-implementation design sketch |

Two facts settle which terms are real:

1. `quarantined` never persists. When EPUB validation returns `ValidationOutcome::Quarantined`, the ingestion
   orchestrator (`backend/src/services/ingestion/orchestrator.rs`) deletes the library file and returns a failed
   result; no manifestation row is ever created. `quarantined` is therefore a dead variant on the storage and wire
   surface.
2. `pending` is real. It is the column default and a legitimate lifecycle state (row exists, validation not yet
   run). Any vocabulary that drops it is wrong.

The remaining disagreement is purely the `Clean` to `valid` rename. The orchestrator currently translates
`ValidationOutcome::Clean` to the stored string `"valid"`, and that translation is the drift: the storage string
diverges from the domain enum that defines it. Resolving the vocabulary is a prerequisite for the typed enum, since a
`sqlx::Type` cannot be introduced cleanly on a vocabulary that contradicts its own domain source. The choice spans
the database schema, the Rust DTO layer, and the frontend interface.

## Decision drivers

- Close the `sqlx::Type` enum series and remove the last raw-`String` DB enum, so an unknown DB variant fails decode
  loudly instead of reaching the wire as an opaque string.
- Eliminate the storage-to-domain vocabulary drift at its root rather than codify it.
- Name the value set correctly: the stored label should not imply a false semantic.
- Simplicity: no speculative schema for unbuilt product surfaces.
- Pre-release schema is freely mutable, so a rename costs no production data backfill.

## Considered options

- Rename the Postgres value `valid` to `clean`; canonical vocabulary `pending | clean | repaired | degraded`
- Adopt the DB vocabulary as-is (`valid`) and rewrite the frontend union to match
- Keep `validation_status` untouched; add a separate `availability_status` (`clean | quarantined`) column for a
  curation surface
- Rename `pending` to `clean` and add `quarantined`

## Decision outcome

Chosen option: **rename the Postgres value `valid` to `clean`**, because it eliminates the storage-to-domain drift at
its root, names the value set correctly, and costs a one-time pre-release migration rather than a permanent mislabel.

The Postgres enum value is renamed in place (`ALTER TYPE ... RENAME VALUE`), keeping `pending`, `repaired`, and
`degraded`, and `quarantined` is not added. The canonical vocabulary across the database, the Rust DTO layer, and the
wire becomes `pending | clean | repaired | degraded`, surfaced through a new `ValidationStatus` `sqlx::Type` enum
(`backend/src/models/validation_status.rs`) following the existing enum pattern.

`valid`, `repaired`, and `degraded` are all stored-and-usable outcomes. Labelling one of them `valid` implies the
other two are invalid; they are not, a repaired or degraded file is still ingested, stored, and served. `clean` names
the actual distinction: no issues found, as opposed to had issues and auto-repaired (`repaired`) or has issues and
tolerated (`degraded`). The three are points on one quality tier, not one valid state plus two error states.

Renaming to `clean` also makes the orchestrator mapping an identity (`Clean` maps to `"clean"`), eliminating the
translation seam that produced the drift, and realigns the stored string with `ValidationOutcome`, the only place
validation semantics are decided.

The operator-facing explanation of these states is deferred to documentation work rather than built ad hoc; the
dev-facing `docs/schema.md` reference is corrected at the same time, because the rename makes its current listing
wrong. Operator-facing documentation carries the rationale, not just the values.

### Consequences

- Positive: closes the `sqlx::Type` enum series; the storage string, the domain enum, and the wire union all agree;
  the `Clean` to `"valid"` translation seam is gone.
- Positive: the frontend union tightens from an open string type to a closed set, so an unaccounted-for backend enum
  change surfaces as a validation error at the boundary rather than silent UI drift.
- Positive: pre-release, so the rename needs no production data backfill (`ALTER TYPE RENAME VALUE` rewrites the
  label in place).
- Negative: touches around a dozen seed and test SQL call sites writing `'valid'::validation_status`; mechanical but
  spread across several test files.

## Pros and cons of the options

### Rename the Postgres value `valid` to `clean`

- Positive: kills the storage-to-domain drift at its root, gives the orchestrator an identity mapping, names the
  value set correctly, and tightens the wire contract via a typed enum.
- Negative: one migration plus around a dozen seed and test call-site edits.
- Neutral: pre-release, so no production backfill.

### Adopt the DB vocabulary as-is (`valid`)

- Positive: smallest delta, no migration and no `'valid'` call-site churn.
- Negative: codifies the `valid` mislabel permanently and leaves the `Clean` to `"valid"` translation seam in place,
  the exact drift this decision exists to remove. Cheaper now, but the mislabel cost is permanent and the migration
  cost is one-time and pre-release.

### Separate `availability_status` column

- Positive: cleanly separates ingestion validity from a curation surface if quarantine ever becomes a retained
  state.
- Negative: speculative, since no quarantine-as-state product surface exists today (quarantine means the file is
  deleted, with no row). Adds schema, DTO, and frontend surface for an unbuilt concept, and would need its own
  decision record if the feature becomes real.

### Rename `pending` to `clean` and add `quarantined`

- Negative: factually wrong as stated. `pending` is a real lifecycle state (the column default) that must stay, and
  `quarantined` has no write path. Rejected; the accepted decision renames `valid` instead and keeps `pending`.

## More information

The value set was later extended with `failed`, for the case where the validator itself could not run; that outcome
previously borrowed `degraded`, hiding validator crashes from operators. The four original values and their
semantics are unchanged, and `quarantined` remains absent.

Domain source enum: `backend/src/services/epub/mod.rs` (`ValidationOutcome`).
