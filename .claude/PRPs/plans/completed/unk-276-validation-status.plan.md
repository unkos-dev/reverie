# Feature: Typed `ValidationStatus` enum + `valid`→`clean` vocabulary reconciliation (UNK-276)

## Summary

Rename the Postgres `validation_status` enum value `valid` → `clean`
(canonical set `pending | clean | repaired | degraded`), introduce a
Rust `ValidationStatus` `sqlx::Type` enum mirroring the
`IngestionStatus`/`EnrichmentStatus` pattern, and retire the raw-`String`
field across the API DTOs and the series route. Closes the last
raw-`String` DB enum in the sqlx::Type migration series. Decision and
rationale are settled in
[`adr/2026-05-28-validation-status-vocabulary.md`](../../../adr/2026-05-28-validation-status-vocabulary.md).

## User Story

As a Reverie maintainer / external integrator
I want `validation_status` to be a typed, correctly-named closed enum on
both stacks
So that unknown DB variants fail loudly at the boundary and the wire
value (`clean`) no longer falsely implies repaired/degraded files are
invalid.

## Problem Statement

`validation_status` is the only Postgres enum still decoded into Rust as
a raw `String`. Three vocabularies disagree (domain `ValidationOutcome`
= `Clean/Repaired/Degraded/Quarantined`; DB = `pending/valid/repaired/
degraded`; frontend sketch = `clean/repaired/degraded/quarantined`).
`quarantined` never persists (quarantine deletes the file, no row);
`pending` is real (column default). The drift is the `Clean => "valid"`
orchestrator translation. Testable: after the change, no `String`-typed
`validation_status` field exists, the DB value is `clean`, and the wire
union is closed.

## Solution Statement

Option A from the ADR: rename the DB value, add a typed enum, drop the
`::text` casts, mirror the closed set on the frontend Zod schema. No new
dependency — reuses the `sqlx::Type` derive already in the tree.

## Metadata

| Field            | Value                                                                                                                                |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Type             | REFACTOR                                                                                                                             |
| Complexity       | MEDIUM                                                                                                                               |
| Systems Affected | backend models, backend routes (library, series), ingestion orchestrator, migrations, `.sqlx` cache, frontend api client, docs, debt |
| Dependencies     | `sqlx` (in tree), `serde` (in tree), `zod` (frontend, in tree) — no new deps                                                         |
| Estimated Tasks  | 12                                                                                                                                   |

---

## UX Design

Backend data-shape refactor. The JSON **wire shape is unchanged**:
`validation_status` remains a JSON string — the value changes from
`"valid"` → `"clean"` and the type tightens from open string to a closed
union. No UI/route/screen change; the Step 11 frontend slice that renders
the field has not landed yet.

### Interaction Changes

| Location                                                      | Before                                                  | After                                                          | User Impact                                                 |
| ------------------------------------------------------------- | ------------------------------------------------------- | -------------------------------------------------------------- | ----------------------------------------------------------- |
| `GET /api/books[/{id}]`, `GET /api/works/{id}`, series detail | `validation_status: "valid"` (open string)              | `validation_status: "clean"` (closed enum)                     | Correct label; clients can exhaustively switch on the union |
| Backend decode                                                | raw `String`, unknown DB variant flows through silently | `ValidationStatus` sqlx decode, unknown variant = decode error | Drift fails loudly at the boundary                          |

---

## Mandatory Reading

**Implementation agent MUST read these before starting:**

| Priority | File                                             | Lines                       | Why                                                          |
| -------- | ------------------------------------------------ | --------------------------- | ------------------------------------------------------------ |
| P0       | `backend/src/models/ingestion_status.rs`         | all                         | Enum to MIRROR (lowercase variant, derives, test block)      |
| P0       | `backend/src/models/enrichment_status.rs`        | 23-96                       | `Display` impl + test that asserts `format!()` == wire       |
| P0       | `backend/src/models/manifestation_format.rs`     | 139-164                     | Enum-drift `#[sqlx::test]` probe + UNK-167 carve-out comment |
| P1       | `backend/src/models/library.rs`                  | 70-77, 121-127, 280-288     | DTO fields to retype                                         |
| P1       | `backend/src/routes/library/mod.rs`              | 152-228, 689-734, 903-920   | Read sites (QueryBuilder + macros)                           |
| P1       | `backend/src/routes/series/mod.rs`               | 72-129                      | Shares `WorkManifestation` DTO                               |
| P1       | `backend/src/services/ingestion/orchestrator.rs` | 381-439, 612-633, 1063-1072 | Map + INSERT bind + test assert                              |
| P2       | `frontend/src/api/books.ts`                      | 27-33, 46-65, 121-160       | z.enum pattern + 3 schemas to tighten                        |
| P2       | `backend/CLAUDE.md`                              | sqlx section                | `.sqlx` cache regen + runtime carve-out allowlist            |

**External documentation:** none required — no new library; established
internal pattern.

---

## Patterns to Mirror

**ENUM (lowercase, with Display):**

```rust
// SOURCE: backend/src/models/ingestion_status.rs:21-39 (shape)
//         + enrichment_status.rs:58-62 (Display impl)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash,
    serde::Serialize, serde::Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "validation_status", rename_all = "lowercase")]
pub enum ValidationStatus { Pending, Clean, Repaired, Degraded }
// + as_str() match, + impl Display calling as_str()
```

**ENUM-DRIFT PROBE:**

```rust
// SOURCE: backend/src/models/manifestation_format.rs:139-164
// runtime sqlx::query (UNK-167 carve-out comment REQUIRED):
sqlx::query("ALTER TYPE validation_status ADD VALUE 'probe_unknown'")...;
let result: Result<ValidationStatus, _> =
    sqlx::query_scalar("SELECT 'probe_unknown'::validation_status").fetch_one(&pool).await;
assert!(result.is_err(), ...);
```

**MACRO READ with column override (replaces `::text` cast):**

```rust
// PATTERN per backend/CLAUDE.md: column-type override forces OID check
validation_status AS "validation_status: ValidationStatus"
```

**RUNTIME QueryBuilder decode (list path):**

```rust
// SOURCE: routes/library/mod.rs:213 (currently String)
// AFTER: select `m.validation_status` (no ::text), then:
let validation: ValidationStatus = r.get("validation_status");
```

**FRONTEND z.enum:**

```typescript
// SOURCE: frontend/src/api/books.ts:27-31
const ValidationStatusSchema = z.enum(["pending", "clean", "repaired", "degraded"]);
export type ValidationStatus = z.infer<typeof ValidationStatusSchema>;
```

---

## Files to Change

| File                                                                                   | Action | Justification                                                                                                                         |
| -------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| `backend/migrations/{ts}_rename_validation_status_valid_to_clean.up.sql` + `.down.sql` | CREATE | `ALTER TYPE ... RENAME VALUE 'valid' TO 'clean'`                                                                                      |
| `backend/src/models/validation_status.rs`                                              | CREATE | Typed `ValidationStatus` enum + tests + drift probe                                                                                   |
| `backend/src/models/mod.rs`                                                            | UPDATE | Register `pub mod validation_status;` (alpha order, after `series`/`theme_preference`, before `user`)                                 |
| `backend/src/models/library.rs`                                                        | UPDATE | Retype field on `BookListRow:75`, `BookDetail:125`, `WorkManifestation:283`; drop deferred-reconciliation docstrings; import the enum |
| `backend/src/routes/library/mod.rs`                                                    | UPDATE | Drop `::text` at :155/:703/:910; retype inline `DetailRow:662`; decode via enum; remove `validation_raw` plumbing                     |
| `backend/src/routes/series/mod.rs`                                                     | UPDATE | Drop `::text` at :80; field assign :125 (shared DTO covers type)                                                                      |
| `backend/src/services/ingestion/orchestrator.rs`                                       | UPDATE | `Clean => "clean"` (:427); fix test assert :1072 (`"valid"`→`"clean"`)                                                                |
| `backend/src/models/work.rs`                                                           | UPDATE | Seed `'valid'`→`'clean'` (:519, :567)                                                                                                 |
| `backend/src/test_support.rs`                                                          | UPDATE | Seed `'valid'`→`'clean'` (:456)                                                                                                       |
| `backend/src/routes/library/tests.rs`                                                  | UPDATE | Seeds (:95,:597,:883,:1434) + tighten asserts (:136,:728)                                                                             |
| `backend/src/routes/opds/tests.rs`                                                     | UPDATE | Seeds (:57,:462,:594,:815)                                                                                                            |
| `backend/src/services/enrichment/queue.rs`                                             | UPDATE | Seed (:379)                                                                                                                           |
| `backend/src/services/enrichment/orchestrator.rs`                                      | UPDATE | Seed (:1111)                                                                                                                          |
| `backend/src/services/enrichment/field_lock.rs`                                        | UPDATE | Seed (:153)                                                                                                                           |
| `backend/src/services/metadata/draft.rs`                                               | UPDATE | Seed (:226)                                                                                                                           |
| `backend/src/services/writeback/queue.rs`                                              | UPDATE | Seed (:476)                                                                                                                           |
| `backend/src/services/writeback/orchestrator.rs`                                       | UPDATE | Seed (:850)                                                                                                                           |
| `backend/.sqlx/*`                                                                      | UPDATE | Regenerate (macro column override + renamed literal)                                                                                  |
| `frontend/src/api/books.ts`                                                            | UPDATE | `z.string()`→`z.enum(...)` ×3 (:58,:136,:155) + export type                                                                           |
| `docs/schema.md`                                                                       | UPDATE | Row :89 → `pending, clean, repaired, degraded`                                                                                        |
| `docs/RELEASE_DOCS_BACKLOG.md`                                                         | CREATE | Operator-rationale backlog item                                                                                                       |
| `debt/2026-05-23-validation-status-untyped.md`                                         | UPDATE | Flip to `status: lifted`                                                                                                              |
| `debt/README.md`                                                                       | UPDATE | Move index entry Active→Lifted                                                                                                        |

> Note: the agent investigation found **two extra seed sites** beyond the
> ADR's enumeration — `services/writeback/queue.rs:476` and
> `services/writeback/orchestrator.rs:850`. Total seed sites: 19 (not 17).

---

## NOT Building (Scope Limits)

- **No `availability_status` column** — quarantine-as-retained-state is
  not a product surface today (ADR Option C rejected as speculative).
- **No `quarantined` enum value** — never persists.
- **No refactor of `ingestion_status`/`enrichment_status` decode** — they
  keep `::text` + `parse_*`; out of scope.
- **No Starlight operator docs page** — deferred to release-docs backlog.
- **No production data backfill** — pre-release; `RENAME VALUE` rewrites
  the label in place.

---

## Step-by-Step Tasks

### Task 1: CREATE migration pair

- **ACTION**: new timestamped `*_rename_validation_status_valid_to_clean.{up,down}.sql` in `backend/migrations/`.
- **IMPLEMENT**: up `ALTER TYPE public.validation_status RENAME VALUE 'valid' TO 'clean';` / down reverse.
- **GOTCHA**: `RENAME VALUE` is txn-safe on PG12+ (runs in the batch migration runner). Leaves the `'pending'` column default intact — no `DROP/SET DEFAULT`.
- **VALIDATE**: `cargo run` boots and auto-applies; `\dT+ validation_status` shows `{pending,clean,repaired,degraded}`.

### Task 2: CREATE `backend/src/models/validation_status.rs` (TDD — tests first)

- **ACTION**: write the enum + unit tests + drift probe.
- **MIRROR**: `ingestion_status.rs` (lowercase) + `enrichment_status.rs` Display + `manifestation_format.rs:139-164` probe.
- **IMPLEMENT**: `ValidationStatus {Pending,Clean,Repaired,Degraded}`, derives, `#[sqlx(type_name="validation_status", rename_all="lowercase")]`, `#[serde(rename_all="lowercase")]`, `as_str`, `Display`. Tests: `as_str`↔serde, JSON round-trip, reject-unknown, drift `#[sqlx::test]`.
- **GOTCHA**: drift-probe runtime `sqlx::query` needs the UNK-167 carve-out comment AND a `.github/sqlx-runtime-allowlist.txt` entry.
- **VALIDATE**: `cargo test -p reverie_api validation_status`.

### Task 3: UPDATE `backend/src/models/mod.rs`

- **ACTION**: add `pub mod validation_status;` in alpha order.
- **VALIDATE**: `cargo build`.

### Task 4: UPDATE `backend/src/models/library.rs`

- **ACTION**: retype `validation_status: String` → `ValidationStatus` on the 3 DTOs; import `use crate::models::validation_status::ValidationStatus;`; drop the deferred-reconciliation docstrings.
- **VALIDATE**: `cargo build` (will fail at route sites until Task 5/6 — expected).

### Task 5: UPDATE `backend/src/routes/library/mod.rs`

- **ACTION**: drop `::text` at :155 (select `m.validation_status`), :703, :910; column override `AS "validation_status: ValidationStatus"` on the two macros; retype inline `DetailRow.validation_status:662`; in the list loop, `let validation: ValidationStatus = r.get("validation_status")`; remove dead `validation_raw`.
- **GOTCHA (decode hazard)**: the list handler is a runtime `QueryBuilder`, NOT macro-checked — this is the first custom-enum `PgRow::get` decode in the codebase. Must be exercised by a test (Task 8). Leave `parse_ingestion`/`parse_enrichment` untouched.
- **VALIDATE**: `cargo build`.

### Task 6: UPDATE `backend/src/routes/series/mod.rs`

- **ACTION**: drop `::text` at :80 (column override); field assign :125 unchanged (shared `WorkManifestation` type now enum).
- **VALIDATE**: `cargo build`.

### Task 7: UPDATE orchestrator map + assert

- **ACTION**: `ValidationOutcome::Clean => ("clean", ...)` (:427); non-EPUB pass-through `("clean", None, None)` (:439); test assert `"valid"`→`"clean"` (:1072). INSERT bind (:621) unchanged (binds `"clean"`).
- **VALIDATE**: `cargo test -p reverie_api ingestion`.

### Task 8: UPDATE seeds + tighten list-path tests (adversarial C1/C2)

- **ACTION**: replace all 19 `'valid'::validation_status` → `'clean'::validation_status` (see Files table). Change `tests.rs:136` and `:728` from `is_string()` to `== "clean"`; ensure ≥1 `GET /api/books` row is decoded + value-asserted so the runtime enum decode (Task 5 hazard) is exercised.
- **VALIDATE**: `cargo test -p reverie_api`.

### Task 9: REGENERATE `.sqlx` cache

- **ACTION**: `DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo sqlx prepare -- --tests` from `backend/`.
- **VALIDATE**: `cargo sqlx prepare --check -- --tests` exits 0.

### Task 10: UPDATE frontend `books.ts`

- **ACTION**: add `ValidationStatusSchema = z.enum(["pending","clean","repaired","degraded"])` + exported type; replace the 3 `validation_status: z.string()` (:58,:136,:155); doc-comment → backend module.
- **VALIDATE**: `npm run -s lint && npx tsc --noEmit` (frontend).

### Task 11: UPDATE docs + debt

- **ACTION**: `docs/schema.md:89` → `pending, clean, repaired, degraded`; CREATE `docs/RELEASE_DOCS_BACKLOG.md` with the operator-rationale item; flip `debt/2026-05-23-validation-status-untyped.md` to `lifted` (+`lifted:` date, `superseded-by: UNK-276`); move `debt/README.md` index Active→Lifted.
- **VALIDATE**: `rg "'valid'::validation_status" backend` → no matches; `rg "validation_status: String" backend/src` → no matches.

### Task 12: FULL VALIDATION

- **VALIDATE**: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test`, frontend `npm test` + lint — all green.

---

## Testing Strategy

### Unit / integration tests

| Test                                                      | Cases                                         | Validates                                |
| --------------------------------------------------------- | --------------------------------------------- | ---------------------------------------- |
| `models/validation_status.rs` tests                       | as_str↔serde, JSON round-trip, reject-unknown | enum wire-format invariant               |
| `models/validation_status.rs` `#[sqlx::test]` drift probe | `ADD VALUE` + cast → decode error             | loud-failure on schema drift             |
| `routes/library/tests.rs:136,728` (tightened)             | value `== "clean"` + list-path decode         | runtime QueryBuilder enum decode (C1/C2) |
| `services/ingestion/orchestrator.rs:1072`                 | `== "clean"`                                  | Clean→"clean" map                        |

### Edge cases checklist

- [ ] Unknown DB variant → sqlx decode error (drift probe)
- [ ] List path (runtime QueryBuilder) decodes the enum, not just "a string"
- [ ] `pending` default still decodes (row pre-validation)
- [ ] Frontend `z.enum` rejects a non-member string (ZodError)

---

## Validation Commands

### Level 1: STATIC

```bash
cd backend && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings
cd frontend && npm run -s lint && npx tsc --noEmit
```

### Level 2: UNIT/INTEGRATION

```bash
cd backend && cargo test
cd frontend && npm test
```

### Level 3: SQLX CACHE

```bash
cd backend && cargo sqlx prepare --check -- --tests
```

### Level 4: DB

- [ ] `\dT+ validation_status` → `{pending, clean, repaired, degraded}`

---

## Acceptance Criteria

- [ ] `rg "validation_status: String" backend/src` → no matches
- [ ] `rg "::text AS .*validation_status|validation_status::text" backend/src` → no matches
- [ ] `rg "'valid'::validation_status" backend` → no matches
- [ ] `ValidationStatus` enum + drift probe present and passing
- [ ] `tests.rs:136/728` assert `"clean"`; list-path runtime decode exercised
- [ ] Frontend 3 schemas use `z.enum`; tsc/lint clean
- [ ] `docs/schema.md` corrected; `RELEASE_DOCS_BACKLOG.md` has the item
- [ ] debt entry `lifted`; README index moved
- [ ] Level 1-3 green

---

## Risks and Mitigations

| Risk                                                                           | Likelihood | Impact | Mitigation                                                             |
| ------------------------------------------------------------------------------ | ---------- | ------ | ---------------------------------------------------------------------- |
| Runtime QueryBuilder enum decode fails (not compile-checked)                   | MED        | MED    | Task 8 list-path value-assertion test exercises it before merge        |
| Forgot a seed site → test INSERT fails on missing `'valid'` label after rename | MED        | LOW    | `rg "'valid'::validation_status"` acceptance gate; 19 sites enumerated |
| Stale `.sqlx` cache → CI fails                                                 | MED        | LOW    | Task 9 regen + `--check` gate                                          |
| Drift probe runtime query not allowlisted → CI grep-guard fails                | LOW        | LOW    | Add `.github/sqlx-runtime-allowlist.txt` entry in Task 2               |

## Notes

- Provenance: tasks/sites confirmed by `prp-core:codebase-explorer` +
  `prp-core:codebase-analyst` (2026-05-28). Two seed sites
  (`writeback/queue.rs:476`, `writeback/orchestrator.rs:850`) found
  beyond the ADR's list — folded in above.
- Decode-strategy note: after this change, `validation_status` decodes as
  a typed enum while `ingestion_status`/`enrichment_status` stay
  `::text`+`parse_*` in the same list query. Intentional — those are
  out of scope (their own follow-up if ever).
