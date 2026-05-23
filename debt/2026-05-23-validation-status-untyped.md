---
status: active
severity: medium
surfaces: [developer, end-user]
adopted: 2026-05-23
adopted-because: 11a-A.3 (PR #308) ships `BookListRow.validation_status` ahead of a Rust `ValidationStatus` enum because the DB enum (`pending|valid|repaired|degraded`) and the frontend API contract sketch (`clean|repaired|degraded|quarantined`) disagree, and reconciling the two vocabularies is a separate piece of work
lift-when-class: internal-refactor
lift-when: UNK-276 (reconcile validation_status vocabularies — pending) lands; the same PR introduces `models::validation_status::ValidationStatus`, retires the raw-string field, and updates the frontend interface to the chosen vocabulary
lifted: ~
superseded-by: ~
---

# `validation_status` ships as raw `String`, not a typed enum

## Constraint

The Postgres enum is `validation_status AS ENUM ('pending', 'valid',
'repaired', 'degraded')` (see migration
`20260416000001_remove_invalid_validation_status.up.sql`). The
frontend API contract in
`.claude/PRPs/plans/library-ui.plan.md` Pattern
`FRONTEND_API_CLIENT_MODULE` documents the wire shape as
`"clean" | "repaired" | "degraded" | "quarantined"`. The two vocabularies
do not line up: `pending` and `valid` (DB) vs `clean` and `quarantined`
(plan) are unmapped.

Two things follow from that mismatch:

- A typed Rust enum cannot be introduced cleanly without first
  picking one of the vocabularies. Either the DB enum gets renamed
  (`pending` → `clean`, add `quarantined`) or the frontend
  contract is rewritten to reflect the DB vocabulary.
- The decision is not load-bearing for 11a-A.3 (the JSON list/detail
  read path). Deferring it into a follow-up keeps the slice scoped.

## Workaround

`BookListRow::validation_status`, `BookDetail::validation_status`,
and `WorkManifestation::validation_status` are typed as plain
`String` rather than a `sqlx::Type` Rust enum
([`backend/src/models/library.rs:66-69, 110-113, 178-180`](../backend/src/models/library.rs)).
The route handler at
[`backend/src/routes/library/mod.rs`](../backend/src/routes/library/mod.rs)
casts the column to `::text` and surfaces the raw DB-enum lexical
form on the wire. A docstring on the model field flags the
mismatch for future readers.

Consequences:

- A new value added to the DB enum is silently accepted on the
  wire — there is no compile-time signal that the Rust side fell
  out of sync, only an opaque string change that frontend code may
  or may not handle.
- The frontend `BookListItem` interface as currently sketched will
  type-error against the wire shape (`pending` / `valid` are not in
  the union); the frontend slice owner has to either widen the
  union or rewrite the field type.

The trade-off was accepted because reconciliation is a vocabulary
decision spanning two stacks plus a migration, not a `models/`
refactor that fits inside an 11a-A.3-shaped slice.

## Lift trigger

UNK-276 lands the reconciliation PR: picks the canonical
vocabulary, ships a DB migration if the DB enum is renamed,
introduces `models::validation_status::ValidationStatus` with
`sqlx::Type` impls following the `IngestionStatus` /
`EnrichmentStatus` patterns, retires the raw-string field on the
three DTOs, and updates the frontend type union to match.

The lift PR flips this entry's frontmatter (`status: lifted`,
`lifted`, `superseded-by`) and moves the index entry from "Active"
to "Lifted" in `debt/README.md`.
