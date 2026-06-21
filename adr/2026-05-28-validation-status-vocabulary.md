---
status: accepted
date: 2026-05-28
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Reconcile `validation_status` vocabulary and introduce a typed `ValidationStatus` enum

## Context and Problem Statement

`validation_status` is the last Postgres enum still read into Rust as a
raw `String`. Every sibling enum (`user_role`, `ingestion_status`,
`enrichment_status`, `manifestation_format`, `api_cache_kind`) was
migrated to a `sqlx::Type` Rust enum under the user role and ingestion status enum tasks. The validation status vocabulary task closes the
series.

The blocker is not the type machinery; it is a vocabulary collision
surfaced during the adversarial review of PR #308 (Step 11a-A.3, the
`GET /api/books` read path) and tracked in
[`debt/2026-05-23-validation-status-untyped.md`](../debt/2026-05-23-validation-status-untyped.md).
**Three** vocabularies describe the same concept and disagree:

| Source                      | Vocabulary                                        | Where                                                        |
| --------------------------- | ------------------------------------------------- | ------------------------------------------------------------ |
| Domain enum (authoritative) | `Clean` / `Repaired` / `Degraded` / `Quarantined` | `backend/src/services/epub/mod.rs:177` (`ValidationOutcome`) |
| Postgres enum (storage)     | `pending` / `valid` / `repaired` / `degraded`     | `validation_status` type                                     |
| Frontend sketch (wire)      | `clean` / `repaired` / `degraded` / `quarantined` | `.claude/PRPs/plans/library-ui.plan.md`                      |

Two facts settle which terms are real:

1. **`quarantined` never persists.** When EPUB validation returns
   `ValidationOutcome::Quarantined`, the orchestrator deletes the
   library file and returns `ProcessResult::Failed`, no manifestation
   row is ever created (`backend/src/services/ingestion/orchestrator.rs:404-425`).
   `quarantined` is therefore a dead variant on the storage/wire surface.
2. **`pending` is real.** It is the column default (`DEFAULT 'pending'`)
   and a legitimate lifecycle state (row exists, validation not yet
   run). Any vocabulary that drops it is wrong.

The remaining disagreement is purely the `Clean` ↔ `valid` rename. The
orchestrator currently translates `ValidationOutcome::Clean => "valid"`
(`orchestrator.rs:427`). That translation step **is** the drift: the
storage string diverges from the domain enum that defines it. Resolving
the vocabulary is a prerequisite for the typed enum, a `sqlx::Type`
cannot be introduced cleanly on a vocabulary that contradicts its own
domain source. The choice spans DB schema + Rust DTO + frontend
interface, so it did not fit inside the 11a-A.3 read-path slice and was
deferred here.

## Decision Drivers

- Close the `sqlx::Type` enum series, remove the last raw-`String` DB
  enum so unknown DB variants fail decode loudly instead of reaching the
  wire as opaque strings.
- Eliminate the storage↔domain vocabulary drift at its root rather than
  codify it.
- Name the value set correctly: the stored label should not imply a
  false semantic (see Decision Outcome).
- Simplicity: no speculative schema for unbuilt product surfaces.
- Pre-release schema is freely mutable
  ([`project_schema_evolution`](../.claude/projects/-home-coder-reverie/memory/project_schema_evolution.md)),
  so a rename costs no production data backfill.

## Considered Options

- **Option A**: rename the Postgres value `valid` → `clean`; canonical
  vocabulary `pending | clean | repaired | degraded`. _(chosen)_
- **Option B**: adopt the DB vocabulary as-is (`valid`) and rewrite the
  frontend union to match.
- **Option C**: keep `validation_status` untouched; add a separate
  `availability_status` (`clean | quarantined`) column for a curation
  surface.
- **Issue-as-written variant**: rename `pending` → `clean` and add
  `quarantined`, per the validation status vocabulary's original Option-A text.

## Decision Outcome

Chosen: **Option A**: rename the Postgres enum value `valid` → `clean`
(`ALTER TYPE ... RENAME VALUE`), keep `pending`/`repaired`/`degraded`,
do not add `quarantined`. Canonical vocabulary across DB, Rust DTO, and
wire becomes `pending | clean | repaired | degraded`, surfaced through a
new `ValidationStatus` `sqlx::Type` enum following the `IngestionStatus`
pattern.

### Why `clean`, not `valid` (load-bearing rationale)

`valid | repaired | degraded` are **all** stored-and-usable outcomes.
Labelling one of them `valid` implies the other two are _invalid_, they
are not; a repaired or degraded file is still ingested, stored, and
served. `clean` names the actual distinction: _no issues found_, as
opposed to _had issues, auto-repaired_ (`repaired`) or _has issues,
tolerated_ (`degraded`). The three are points on one quality tier, not
one valid state plus two error states.

Renaming to `clean` additionally makes the orchestrator mapping an
identity (`Clean => "clean"`), eliminating the translation seam that
produced the drift, and realigns the stored string with
`ValidationOutcome`: the only place validation semantics are
decided.

The operator-facing explanation of these states is deferred to a
release-docs backlog (`docs/RELEASE_DOCS_BACKLOG.md`) rather than a
half-built Starlight page; the dev-facing `docs/schema.md` reference is
corrected now because the rename makes its current listing wrong. See
[`feedback_rationale_in_user_docs`](../.claude/projects/-home-coder-reverie/memory/feedback_rationale_in_user_docs.md).

### Consequences

- **Good**: closes the `sqlx::Type` enum series; storage string, domain
  enum, and wire union all agree; the `Clean => "valid"` translation
  seam is gone.
- **Good**: the frontend union tightens from `z.string()` to a closed
  set, so an unaccounted-for backend enum change surfaces as a
  `ZodError` at the boundary, not silent UI drift.
- **Bad**: touches ~12 seed/test SQL call sites writing
  `'valid'::validation_status`; mechanical but spread across several
  test files.
- **Neutral**: pre-release, so the rename needs no production data
  backfill (`ALTER TYPE RENAME VALUE` rewrites the label in place).

### Confirmation

Implementation tasks, the migration mechanics, the affected file:line
sites, and the verification checklist live in the committed plan
`.claude/PRPs/plans/unk-276-validation-status.plan.md`. Compliance with
this decision is confirmed by that plan's verification gate plus CI:
`cargo sqlx prepare --check -- --tests`, `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`,
and the frontend lint/test suite. The closing PR flips
`debt/2026-05-23-validation-status-untyped.md` to `status: lifted`.

## Pros and Cons of the Options

### Option A: rename `valid` → `clean` (chosen)

- Good: kills the storage↔domain drift at its root; identity
  orchestrator mapping; correct naming of the value set; tightens the
  wire contract via a typed enum.
- Bad: one migration plus ~12 seed/test call-site edits.
- Neutral: pre-release, so no production backfill.

### Option B: adopt DB vocabulary as-is (`valid`)

- Good: smallest delta: no migration, no `'valid'` call-site churn.
- Bad: codifies the `valid` mislabel permanently and leaves the
  `Clean => "valid"` translation seam in place (the exact drift this
  ticket exists to remove). Cheaper now, but the mislabel cost is
  forever and the migration cost is one-time and pre-release.

### Option C: separate `availability_status` column

- Good: cleanly separates ingestion validity from a curation surface
  if quarantine ever becomes a retained state.
- Bad: speculative: no quarantine-as-state product surface exists today
  (quarantine = file deleted, no row). Adds schema + DTO + frontend for
  an unbuilt concept; violates simplicity-first. Gets its own ADR if the
  feature becomes real.

### Issue-as-written variant (rename `pending` → `clean`, add `quarantined`)

- Bad: factually wrong as stated: `pending` is a real lifecycle state
  (column default) that must stay, and `quarantined` has no write path.
  Rejected; the accepted decision renames `valid` instead and keeps
  `pending`.

## More Information

- Value-set extension (2026-06-11,
  the validation failure states task): `failed` added for
  "the validator itself could not run": previously that outcome borrowed
  `degraded`, hiding validator crashes from operators. The four original
  values and their semantics are unchanged; `quarantined` remains absent.
- Implementation plan (committed):
  `.claude/PRPs/plans/unk-276-validation-status.plan.md`.
- Tracked debt:
  [`debt/2026-05-23-validation-status-untyped.md`](../debt/2026-05-23-validation-status-untyped.md).
- Enum-series precedent: the user role and ingestion status enum tasks.
- Step 11 umbrella: the API conventions work; this issue is the validation status vocabulary task.
- Domain source enum: `backend/src/services/epub/mod.rs:177`
  (`ValidationOutcome`).
- Operator-rationale-in-docs principle:
  [`feedback_rationale_in_user_docs.md`](../.claude/projects/-home-coder-reverie/memory/feedback_rationale_in_user_docs.md).
