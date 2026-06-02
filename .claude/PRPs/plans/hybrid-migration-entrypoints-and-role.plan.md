# Feature: Hybrid migration entrypoints + dedicated least-privilege migration role (reverie side)

## Summary

Implement the reverie-side of ADR `adr/2026-06-02-hybrid-migration-entrypoints-and-role.md`. Replace the cluster-superuser migration identity with a dedicated non-superuser `reverie_migrator` role, and split migration invocation into (a) a `reverie migrate` CLI subcommand (out-of-band, the shipped default) and (b) an opt-in `REVERIE_AUTO_MIGRATE` in-process startup flag (default **false**). The application process must hold **no** migration credential in the default topology, while still performing a read-only schema-divergence check (fail-closed in both directions — ahead and behind).

Homelab (env template, `REVERIE_MIGRATOR_PASSWORD` secret, ansible) is a **separate** cross-repo pass — NOT in this plan.

**Pre-v0.1.0 operating stance vs. design target.** The migration system is _designed_ for the post-v0.1.0 production model: forward-only, immutable applied migrations, least-privilege role (per the ADR). Pre-v0.1.0, no Reverie database is load-bearing — we _operate_ disposably: edit the initial migration in place and recreate (`down -v` → fresh apply), consolidating into one rolled-up migration before first release. This is an operating stance, not a design constraint; it does not soften any production-targeted choice. The data-preserving cutover machinery (`REASSIGN OWNED`, forward grant-migrations) is operationally N/A until a load-bearing DB exists at v0.1.0 — designed-for, not exercised now.

## User Story

As a self-hosting operator
I want migrations to run as a least-privilege role via a one-shot step (or opt-in on startup)
So that my exposed application process never carries cluster-superuser database credentials, while `docker compose up -d` upgrades stay one command.

## Problem Statement

Today `db::run_migrations` runs unconditionally in-process at startup (`lib.rs:274`) using `DATABASE_URL_MIGRATION`, which is unconditionally required by `Config::from_source` (`config.rs:400`) and in the bundled image resolves to the cluster superuser (`POSTGRES_USER=reverie`). That credential sits in the long-lived web process environ for its whole lifetime. Testable failures this fixes: (1) the app process env must not contain a schema-management DSN when run out-of-band; (2) the migration identity must be a non-superuser; (3) a stale app must still refuse to serve against a divergent schema (newer OR older) without holding the migration DSN.

## Solution Statement

- New Postgres role `reverie_migrator` provisioned in `init-roles.sql` (NOSUPERUSER NOCREATEROLE NOBYPASSRLS; CONNECT + CREATE on DB + USAGE,CREATE on schema public).
- `DATABASE_URL_MIGRATION` → `Option<String>` on `Config`, required only when `REVERIE_AUTO_MIGRATE=true`; never read on the default server path.
- New `REVERIE_AUTO_MIGRATE` bool (default `false`) via existing `parse_bool`.
- `reverie migrate` subcommand: `main.rs` arg dispatch (`std::env::args()`, no new dep) → a `reverie_api::run_migrate()` entrypoint that reads `DATABASE_URL_MIGRATION` directly (NOT full `Config`) and calls `db::run_migrations`, then exits.
- Server startup: gate the in-process `run_migrations` call on `REVERIE_AUTO_MIGRATE`; when off, run a new read-only `db::verify_schema_current(&app_pool)` using the app pool (requires `GRANT SELECT ON _sqlx_migrations TO reverie_app`). **Fail-closed in both directions**: refuse to start on schema-ahead (DB newer than binary) AND schema-behind (binary newer than DB — the common "forgot to run `migrate`" case), so divergence is a legible startup error, never silent runtime SQL failures.
- Migration initial-schema GRANT block: add the `_sqlx_migrations` read grant; object ownership shifts to `reverie_migrator` by connection identity.
- Compose: add a one-shot `reverie-migrate` service + gate the app via `depends_on: condition: service_completed_successfully` — see OPEN QUESTION on which compose file.

## Metadata

| Field            | Value                                                                |
| ---------------- | -------------------------------------------------------------------- |
| Type             | REFACTOR + NEW_CAPABILITY                                            |
| Complexity       | MEDIUM-HIGH                                                          |
| Systems Affected | db roles, Rust config + entrypoint, migrations SQL, compose, docs    |
| Dependencies     | sqlx (existing), thiserror (existing); NO new crate (std::env::args) |
| Estimated Tasks  | 12                                                                   |

---

## OPEN QUESTIONS (resolve before/at implementation — do NOT guess)

1. **Confirm the shipped compose target + migration-DSN env placement.** The shipped operator compose is `docker/compose.staging.yml` — it already has a `reverie:` app service (`ghcr.io/unkos-dev/reverie`) gated on `reverie-postgres: condition: service_healthy`, fed by `env_file:`. (Repo `docker-compose.yml` is **dev-only**, postgres service only; dev migrate = `cargo run -- migrate`, no compose service.) Task 9 retargets to `compose.staging.yml`. Two points still need human confirmation: (a) confirm `compose.staging.yml` is the intended target (vs. a not-yet-created public example); (b) `DATABASE_URL_MIGRATION` must reach ONLY the new `reverie-migrate` service, NOT the app `reverie` service's shared `env_file:` — so it lives in a migrate-scoped env file or inline `environment:` on the migrate service. **Confirm before Task 9.**
2. **Staging cutover — recreate (disposable dev infra).** Staging is a throwaway pre-v0.1.0 DB (see the operating-stance note above), so the cutover is simply: `docker compose down -v` → fresh apply with `reverie_migrator` running the migration. Objects are then `reverie_migrator`-owned natively — no `REASSIGN OWNED`, no checksum reconciliation. The data-preserving path (diagnose ownership via `pg_class.relowner`, `REASSIGN OWNED`) is post-v0.1.0 machinery and is NOT exercised here; it stays a design consideration for the production upgrade story, not a step in this plan. Homelab-side execution; this plan only records the branch.

---

## Mandatory Reading

| Priority | File                                                      | Lines              | Why                                                                                                                                            |
| -------- | --------------------------------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| P0       | `backend/src/lib.rs`                                      | 219-298            | `run()` startup order; migration call site `:274`; where to gate on the flag + insert read-only check                                          |
| P0       | `backend/src/config.rs`                                   | 379-405            | `from_source`; `DATABASE_URL_MIGRATION` required pattern `:400`; ingestion-DSN optional pattern `:404` to mirror for making migration optional |
| P0       | `backend/src/config.rs`                                   | 736-749            | `parse_bool` — mirror for `REVERIE_AUTO_MIGRATE`                                                                                               |
| P0       | `backend/src/db.rs`                                       | 247-257            | `run_migrations` signature + ephemeral pool; reuse verbatim from subcommand                                                                    |
| P0       | `docker/init-roles.sql`                                   | 31-64              | `\set/\getenv/\gset` + `CREATE ROLE` + `GRANT CONNECT` DO-block — mirror for `reverie_migrator`                                                |
| P0       | `backend/migrations/20260526000000_initial_schema.up.sql` | 12-13, 839-937     | extensions + GRANT block — add `_sqlx_migrations` SELECT grant; ownership context                                                              |
| P1       | `backend/src/main.rs`                                     | 1-4                | entrypoint — add arg dispatch here                                                                                                             |
| P1       | `backend/src/db.rs`                                       | 139-232            | `MigrationError` (`#[non_exhaustive]`) — extend if a verify error is needed                                                                    |
| P1       | `backend/src/config.rs`                                   | 778-829, 1012-1043 | test helpers `env_for`/`BASE_VARS`/`with_overrides`/`without_keys`; the 3 migration-url tests to rewrite                                       |
| P1       | `backend/src/test_support.rs`                             | 12-83, 261-336     | `test_config()` (`migration_database_url` field) + `app_pool_for`/`pool_as_role` (mirror for a `migrator_pool_for` + role-attr test)           |
| P1       | `backend/src/db.rs`                                       | 601-772            | `#[sqlx::test]` runner tests — pattern for new tests                                                                                           |

**External docs:**
| Source | Why |
|--------|-----|
| [PG18 ddl-priv](https://www.postgresql.org/docs/current/ddl-priv.html) + [PG15 release notes](https://www.postgresql.org/docs/15/release-15.html) | public-schema CREATE removed since PG15 → migrator needs `CREATE ON SCHEMA public` not just on DB |
| [PG18 CREATE EXTENSION](https://www.postgresql.org/docs/current/sql-createextension.html) | pg_trgm/pgcrypto trusted; contained objects owned by bootstrap superuser, extension record by caller |
| [PG18 REASSIGN OWNED](https://www.postgresql.org/docs/current/sql-reassign-owned.html) | run as superuser (member of all roles), connected to target DB |
| [Compose depends_on](https://docs.docker.com/reference/compose-file/services/#depends_on) | `service_completed_successfully` blocks on non-zero exit; migrate service `restart: "no"`; depends_on db `service_healthy` |

---

## Patterns to Mirror

**OPTIONAL ENV DSN (mirror for making migration DSN optional):**

```rust
// SOURCE: backend/src/config.rs:404-405
let ingestion_database_url =
    get("DATABASE_URL_INGESTION").unwrap_or_else(|| database_url.clone());
```

**BOOL FLAG:**

```rust
// SOURCE: backend/src/config.rs:736-749 + call site :581
let enabled = parse_bool(get, "REVERIE_OPDS_ENABLED", true)?;
// NEW: let auto_migrate = parse_bool(get, "REVERIE_AUTO_MIGRATE", false)?;
```

**MIGRATION RUNNER (reuse verbatim from subcommand):**

```rust
// SOURCE: backend/src/db.rs:247-257
pub async fn run_migrations(url: &str) -> Result<MigrationReport, MigrationError> {
    let pool = PgPoolOptions::new().max_connections(1).connect(url).await
        .map_err(MigrationError::Connection)?;
    let result = run_migrations_inner(&pool).await;
    pool.close().await;
    result
}
```

**ROLE PROVISION (mirror for reverie_migrator):**

```sql
-- SOURCE: docker/init-roles.sql:31-51
\set app_password ''
\getenv app_password REVERIE_APP_PASSWORD
SELECT COALESCE(NULLIF(:'app_password',''),'reverie_app') AS app_pw \gset
CREATE ROLE reverie_app WITH LOGIN PASSWORD :'app_pw';
```

**ROLE-SCOPED TEST POOL (mirror for migrator_pool_for + role-attribute assertions):**

```rust
// SOURCE: backend/src/test_support.rs:261-336 (pool_as_role)
```

**#[sqlx::test] forms:**

```rust
// SOURCE: backend/src/db.rs:603-615
#[sqlx::test(migrations = false)]      // test the runner from clean slate
#[sqlx::test(migrations = "./migrations")] // test against migrated schema
```

---

## Files to Change

| File                                                      | Action | Justification                                                                                                                                                                                                                       |
| --------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docker/init-roles.sql`                                   | UPDATE | add `reverie_migrator` role + grants (CONNECT, CREATE ON DB, USAGE+CREATE ON SCHEMA public)                                                                                                                                         |
| `backend/src/config.rs`                                   | UPDATE | `migration_database_url: Option<String>`; `auto_migrate: bool`; conditional-required logic; rewrite 3 migration-url tests + add flag tests                                                                                          |
| `backend/src/db.rs`                                       | UPDATE | add `verify_schema_current(pool)` read-only check (fail-closed: refuses schema-ahead AND schema-behind); add `MigrationError::SchemaBehind` variant alongside the existing ahead path                                               |
| `backend/src/lib.rs`                                      | UPDATE | gate `run_migrations` on `auto_migrate`; else call `verify_schema_current` with app pool; add `pub async fn run_migrate()`                                                                                                          |
| `backend/src/main.rs`                                     | UPDATE | `std::env::args()` dispatch: `migrate` → `run_migrate()`, else `run()`                                                                                                                                                              |
| `backend/migrations/20260526000000_initial_schema.up.sql` | UPDATE | add `GRANT SELECT ON _sqlx_migrations TO reverie_app`                                                                                                                                                                               |
| `backend/src/test_support.rs`                             | UPDATE | `test_config()` migration field → `None`; add `migrator_pool_for` if needed                                                                                                                                                         |
| `docker/staging.env.runtime.example`                      | UPDATE | do NOT add `DATABASE_URL_MIGRATION` — this is the app `reverie` service's `env_file:` (`compose.staging.yml:78`); keeping the migration DSN out of it is the whole point (Task 9). Only fix the "schema owner bypasses RLS" wording |
| `docker/staging.env.migrate.example`                      | CREATE | migrate-scoped env template: `DATABASE_URL_MIGRATION` (reverie_migrator DSN) ONLY; sourced by the `reverie-migrate` one-shot service, never the app                                                                                 |
| `.env.example`                                            | UPDATE | dev `DATABASE_URL_MIGRATION` → reverie_migrator role; note hybrid invocation                                                                                                                                                        |
| `backend/CLAUDE.md`                                       | UPDATE | document hybrid invocation + reverie_migrator (path ref already updated)                                                                                                                                                            |
| `docker/compose.staging.yml`                              | UPDATE | add `reverie-migrate` one-shot + app `service_completed_successfully` gating — see OPEN QUESTION 1 (env-DSN placement)                                                                                                              |
| `docs/` (Starlight)                                       | UPDATE | operator-facing: migrate step, REVERIE_AUTO_MIGRATE, bare-docker two-step (per memory: rationale in user docs)                                                                                                                      |

---

## NOT Building

- Homelab env template / `REVERIE_MIGRATOR_PASSWORD` Infisical secret / ansible — separate cross-repo pass.
- Changing the migration runner internals (batch tx, schema-ahead, advisory lock, logging) — carried forward unchanged from the superseded ADR.
- `lock_timeout` strategy changes (UNK-296) / logging-convention changes (UNK-297).
- Staging REASSIGN/recreate execution — diagnostic recorded here; execution is homelab-side.

---

## Step-by-Step Tasks (dependency order)

### Task 1: UPDATE `docker/init-roles.sql` — add `reverie_migrator`

- ACTION: add migrator password resolution + role + grants, mirroring the app-role block (`:31-64`).
- IMPLEMENT: `\getenv migrator_password REVERIE_MIGRATOR_PASSWORD`; `COALESCE(NULLIF(...),'reverie_migrator')`; `CREATE ROLE reverie_migrator WITH LOGIN PASSWORD :'mig_pw' NOSUPERUSER NOCREATEROLE NOBYPASSRLS;` then in the DO-block: `GRANT CONNECT ON DATABASE %I TO reverie_migrator`, `GRANT CREATE ON DATABASE %I TO reverie_migrator`, and (outside, schema-level) `GRANT USAGE, CREATE ON SCHEMA public TO reverie_migrator;`.
- GOTCHA: PG15+ — `CREATE ON SCHEMA public` is REQUIRED (database CREATE alone won't allow `CREATE EXTENSION ... WITH SCHEMA public` or `CREATE TABLE` in public). `public` schema exists at init time.
- GOTCHA: `GRANT CREATE ON DATABASE` is ALSO required and is NOT redundant — the initial migration runs `CREATE SCHEMA IF NOT EXISTS tower_sessions` (`...up.sql:108`). Creating a _schema_ needs database-level CREATE; `CREATE ON SCHEMA public` covers only objects _within_ public. Annotate this in the SQL so a later least-privilege audit doesn't strip it.
- GOTCHA: keep `reverie` (POSTGRES_USER) bootstrap-only; do NOT grant migrator membership in superuser.
- VALIDATE: fresh `docker compose down -v && up`; `\du reverie_migrator` shows no Superuser/Createrole/Bypass RLS.

### Task 2 (TDD): config tests FIRST — rewrite required→conditional

- ACTION: rewrite `from_env_missing_migration_url` / `from_env_empty_migration_url_rejected` / `from_env_custom_migration_url` (`config.rs:1012-1043`) for the new contract; add `auto_migrate` tests.
- IMPLEMENT cases: (a) AUTO_MIGRATE unset + no migration url → Ok, `migration_database_url == None`, `auto_migrate == false`; (b) `REVERIE_AUTO_MIGRATE=true` + no migration url → `MissingVar`; (c) `=true` + url present → `Some(url)`; (d) invalid flag value → `Invalid`; (e) custom url stored.
- MIRROR: `with_overrides`/`without_keys` helpers (`:801-829`).
- GOTCHA: `BASE_VARS` includes `DATABASE_URL_MIGRATION`; tests asserting "not required by default" must `without_keys` it AND not set the flag.
- VALIDATE: `cargo test -p reverie-api config::` → new tests FAIL (red).

### Task 3: UPDATE `backend/src/config.rs` — make migration DSN optional + add flag

- ACTION: `migration_database_url: Option<String>`; `auto_migrate: bool`.
- IMPLEMENT: `let auto_migrate = parse_bool(get, "REVERIE_AUTO_MIGRATE", false)?;` then `let migration_database_url = get("DATABASE_URL_MIGRATION").filter(|s| !s.trim().is_empty());` and `if auto_migrate && migration_database_url.is_none() { return Err(ConfigError::MissingVar("DATABASE_URL_MIGRATION".into())); }`.
- MIRROR: optional pattern `:404-405`; required pattern `:400-402`; `parse_bool` `:736-749`.
- GOTCHA: field type change ripples to every `Config` constructor — `test_support.rs:25` and any struct-literal builds.
- VALIDATE: Task 2 tests pass; `cargo build`.

### Task 4: UPDATE `backend/src/test_support.rs`

- ACTION: `test_config()` `migration_database_url: None` (was `String::new()`); add `auto_migrate: false`.
- VALIDATE: `cargo build --tests`.

### Task 5 (TDD): `db::verify_schema_current` test FIRST

- ACTION: `#[sqlx::test(migrations = "./migrations")]` — connect a non-owner (reverie_app) pool via `app_pool_for`, assert `verify_schema_current(&app_pool)` returns Ok on an up-to-date schema; inject a fake-ahead `_sqlx_migrations` row and assert it errors `SchemaAhead`; delete a known `_sqlx_migrations` row (simulate DB behind the binary) and assert it errors `SchemaBehind` (fail-closed, both directions).
- MIRROR: `db.rs:601-772` test style; `test_support.rs:261-336` for the app-role pool.
- GOTCHA: requires Task 6's `GRANT SELECT ON _sqlx_migrations TO reverie_app` to be in the migration, else the app pool can't read it. Ordering: although Task 5 is numbered before Task 6, the test cannot run until Task 7 adds the function (it won't compile before then); by Task 7 the Task 6 grant is already in place, so there is no two-reason failure — the red state through Tasks 5–6 is purely "function missing". (If you prefer strict single-reason TDD, execute Task 6 before writing this test.)
- VALIDATE: fails red (function missing).

### Task 6: UPDATE initial-schema migration — grant app read of `_sqlx_migrations`

- ACTION: in the GRANT block (`...up.sql:839-937`) add `GRANT SELECT ON _sqlx_migrations TO reverie_app;`.
- GOTCHA: `_sqlx_migrations` is created by the runner before user SQL executes, so the grant is valid inside the migration. Migrator owns it → can grant.
- GOTCHA: pre-release schema is mutable — edit the existing initial migration in place (no new migration file) per project convention; re-run `cargo sqlx prepare`.
- GOTCHA (pre-v0.1.0 only): in-place edit changes the file's checksum, so any DB where the initial migration already ran would fail sqlx's checksum check. Pre-release this never bites — every Reverie DB is disposable and recreated (`down -v` → fresh apply), so the checksum is computed fresh. In-place edit is the correct pre-release workflow (see the operating-stance note); the forward-migration alternative is post-v0.1.0 discipline and out of this plan.
- VALIDATE: fresh DB migrate; `\dp _sqlx_migrations` shows reverie_app SELECT.

### Task 7: UPDATE `backend/src/db.rs` — implement `verify_schema_current`

- ACTION: `pub async fn verify_schema_current(pool: &PgPool) -> Result<(), MigrationError>` — read `_sqlx_migrations` versions and compare against the embedded `Migrator` list **both ways** (fail-closed): `SchemaAhead { version }` if the DB has versions the binary doesn't know (DB newer); `SchemaBehind { version }` if the binary has versions the DB hasn't applied (DB older — the forgot-to-migrate case). Read-only; no advisory lock, no writes. Reuse/extract the version-set comparison already in `run_migrations_inner`.
- GOTCHA: do NOT require checksum write access; SELECT only. Extract the comparison so both `run_migrations_inner` and `verify_schema_current` share it (no duplication).
- DESIGN NOTE: refusing on schema-behind is deliberate fail-closed (security stance: multi-user exposed instance). The migrate-then-app design has no legitimate window where the app should run newer than the DB; the bare-docker path has no compose gating, so this check is the only backstop. Reflect this bidirectional invariant in the ADR's Confirmation section.
- VALIDATE: Task 5 tests pass.

### Task 8 (TDD then impl): entrypoint split — `run_migrate()` + flag gating

- ACTION (tests first where feasible): `pub async fn run_migrate() -> anyhow::Result<()>` in `lib.rs`. FIRST install a best-effort subscriber — `tracing_subscriber::fmt().try_init().ok();` — because `run_migrate()` bypasses `run()` (subscriber installed at `lib.rs:255-258`); without it every `tracing::info!`/`error!` in `run_migrate` AND inside `db::run_migrations` drops silently and the operator sees only an exit code. THEN read the DSN directly (NOT `Config::from_env`) WITH an empty-string guard: `std::env::var("DATABASE_URL_MIGRATION").ok().filter(|s| !s.trim().is_empty()).ok_or_else(|| anyhow::anyhow!("DATABASE_URL_MIGRATION is required for `reverie migrate`"))?`. `std::env::var` alone returns `Ok("")` for an exported-empty var, which would pass through to `db::run_migrations("")` as a cryptic `Connection` parse error — mirror the Config path's `.filter()` at `config.rs:400`. Then call `db::run_migrations`, log the report, return. In `run()`, replace the unconditional `:274` call: `if config.auto_migrate { db::run_migrations(config.migration_database_url.as_deref()...) } else { db::verify_schema_current(&pool) }` — note ordering: `verify_schema_current` needs the app `pool` (created at `:287`), so the read-only check moves AFTER pool init; the auto-migrate path stays BEFORE pool init.
- ACTION `main.rs`: `match std::env::args().nth(1).as_deref() { Some("migrate") => reverie_api::run_migrate().await, None => reverie_api::run().await, Some(unknown) => Err(anyhow::anyhow!("unknown subcommand: {unknown:?}; valid: migrate")) }`. Do NOT use a `_ => run()` wildcard — it silently boots the long-running server on any typo (`reverie migration`, `reverie --help`); in a compose `service_completed_successfully` slot a typo in `command:` would make the one-shot migrate container run as a server. Use `anyhow::Err` (non-zero exit via `#[tokio::main]`), NOT `eprintln!`/`println!` — `backend/CLAUDE.md:153` forbids them.
- GOTCHA: migrate subcommand must NOT build full `Config` (no OIDC/app DSN in the migrate container). Keep it to the migration DSN + reuse `db::run_migrations`.
- GOTCHA: `run()` has `#[allow(clippy::too_many_lines)]` already; keep the branch tidy.
- VALIDATE: `cargo run -- migrate` against dev compose applies/no-ops; `cargo run` (no arg, flag unset) starts and runs the read-only check (no migration DSN needed).

### Task 9: compose — migrate service + app gating ⚠ blocked on OPEN QUESTION 1

- ACTION: in `docker/compose.staging.yml` (the shipped operator compose — see OPEN Q1), add `reverie-migrate` (same image, `command: ["migrate"]`, `restart: "no"`, sourcing a migrate-scoped `env_file: .env.migrate` (spec'd by Task 10's `staging.env.migrate.example`) carrying `DATABASE_URL_MIGRATION` ONLY — NOT the app's `.env.runtime`, `depends_on: reverie-postgres: condition: service_healthy`) and gate the app `reverie` service with `depends_on: reverie-migrate: condition: service_completed_successfully` (keeping its existing `reverie-postgres: service_healthy`). The app `reverie` service keeps `env_file: .env.runtime` and that file must NOT carry `DATABASE_URL_MIGRATION`.
- GOTCHA: dev `docker-compose.yml` is postgres-only; dev migrate is `cargo run -- migrate`, NOT a compose service. Do not bolt an app service onto the dev compose without confirming intent.
- GOTCHA: postgres:18 PGDATA volume already at `/var/lib/postgresql` in dev compose (correct for PG18) — don't regress it.
- VALIDATE: `docker compose config` parses; one-shot exits 0 then app starts; on forced migrate failure, app does not start.

### Task 10: docs + examples

- ACTION: create `docker/staging.env.migrate.example` (migrate-scoped, `DATABASE_URL_MIGRATION` only); reconcile `.env.example` + `backend/CLAUDE.md` wording; Starlight operator docs for the migrate step, `REVERIE_AUTO_MIGRATE`, bare-docker two-step.
- GOTCHA: `DATABASE_URL_MIGRATION` is REMOVED from / never added to `staging.env.runtime.example` (the app's `env_file:`) — it lives ONLY in the migrate-scoped example. Adding it to the runtime template would re-inject the migration credential into the app container, defeating the plan's core objective.
- MEMORY: rationale belongs in user-facing docs, not only ADR.
- VALIDATE: markdownlint (lint-staged), doc-lint.

### Task 11: role-attribute + posture tests

- ACTION: `#[sqlx::test(migrations="./migrations")]` asserting `reverie_migrator` is `rolsuper=false, rolcreaterole=false, rolbypassrls=false` (first role-attribute test in the repo); a test/assert that the migration set contains no superuser-only op (no non-trusted `CREATE EXTENSION`, `CREATE ROLE`, `ALTER SYSTEM`, event trigger) — ADR Confirmation invariant.
- VALIDATE: `cargo test`.

### Task 12: full validation + sqlx cache

- ACTION: `cargo sqlx prepare --workspace` (migration SQL changed); fmt/clippy/nextest.
- VALIDATE: see Validation Commands.

---

## Testing Strategy

| Test                              | Cases                                                                             | Validates                                      |
| --------------------------------- | --------------------------------------------------------------------------------- | ---------------------------------------------- |
| config tests (`config.rs`)        | migration url optional unless flag; flag parse                                    | conditional-required contract                  |
| `verify_schema_current` (`db.rs`) | up-to-date Ok; fake-ahead → SchemaAhead; fake-behind → SchemaBehind; via app pool | bidirectional fail-closed schema check + grant |
| entrypoint                        | `migrate` subcommand applies; server flag on/off                                  | hybrid invocation                              |
| role attrs (`db.rs`/new)          | reverie_migrator not super/createrole/bypassrls                                   | least-privilege identity                       |
| migration-set audit               | no superuser-only op                                                              | ADR Confirmation invariant                     |

Edge cases: `DATABASE_URL_MIGRATION` empty string with flag on → MissingVar; flag invalid value → Invalid; migrate subcommand with absent DSN → clear error; migrate subcommand with **exported-empty** DSN (`Ok("")`) → clear error, not cryptic Connection error; app start with flag off and unmigrated DB → SchemaAhead refusal (not cryptic SQL).

---

## Validation Commands

### Level 1: STATIC

```bash
cd backend && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
```

EXPECT: exit 0 (memory: run fmt --check before commit).

### Level 2: UNIT/INTEGRATION

```bash
cd backend && cargo nextest run -p reverie-api
```

EXPECT: all pass. (Memory: no parallel cargo test across worktrees — single session OK; needs live dev DB.)

### Level 3: SQLX ROUND-TRIP + BUILD

```bash
cd backend && cargo sqlx prepare --workspace --check && cargo build --workspace
```

EXPECT: no .sqlx drift, build OK.

### Level 4: DB VALIDATION (live PG)

```bash
docker compose down -v && docker compose up -d postgres
# then run migrate as migrator, then:
# \du reverie_migrator  → no super/createrole/bypassrls
# \dp _sqlx_migrations  → reverie_app has SELECT
# objects in public owned by reverie_migrator
```

### Level 5: COMPOSE (after OPEN QUESTION 1)

```bash
docker compose config   # parses
# migrate one-shot exits 0 → app starts; forced failure → app blocked
```

---

## Acceptance Criteria

- [ ] `reverie_migrator` exists, non-superuser, owns objects, can install trusted extensions.
- [ ] App process env carries no `DATABASE_URL_MIGRATION` in the out-of-band/default path; server refuses on ANY schema divergence (ahead OR behind), fail-closed.
- [ ] `reverie migrate` subcommand runs migrations without building full Config.
- [ ] `REVERIE_AUTO_MIGRATE` default false; true requires the DSN.
- [ ] Levels 1-3 pass; new tests cover the contract + role attributes + migration-set audit.
- [ ] Docs reconciled; no superuser creds in app container.

---

## Risks and Mitigations

| Risk                                                                               | L   | I    | Mitigation                                                                                                                                             |
| ---------------------------------------------------------------------------------- | --- | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `migration_database_url` Option ripples to many `Config` builds                    | MED | MED  | grep all constructors; `test_support.rs:25` known; compiler enforces                                                                                   |
| Staging objects superuser-owned (pre-existing dev DB)                              | LOW | LOW  | recreate (`down -v` → fresh apply); `reverie_migrator` owns objects natively. No `REASSIGN` pre-v0.1.0 — see OPEN QUESTION 2                           |
| migrator missing `CREATE ON SCHEMA public` → extension/table create fails on PG15+ | MED | HIGH | Task 1 grants both DB + schema CREATE; Level 4 fresh-DB test                                                                                           |
| schema divergence check needs app read of `_sqlx_migrations`                       | —   | —    | Task 6 GRANT + Task 5 test gate it                                                                                                                     |
| migration DSN re-injected into app via `staging.env.runtime.example`               | MED | HIGH | Task 10: DSN lives ONLY in `staging.env.migrate.example`; runtime template stays migration-cred-free                                                   |
| default-false flag → bare-docker app starts unmigrated (DB behind binary)          | MED | MED  | documented two-step / flag; **schema-behind** refusal (Task 7, fail-closed) makes the unmigrated case a legible startup error, not silent runtime 500s |

---

## Notes

- No new crate: arg dispatch via `std::env::args()` (no `clap`) — repo has zero CLI parser; one subcommand doesn't justify a dep (simplicity).
- Migration runner internals unchanged; `verify_schema_current` shares the version-set comparison with `run_migrations_inner` (extract, don't duplicate), computing both the ahead and behind set-differences (fail-closed both ways).
- Sequencing vs ADR PR #404: this plan implements the accepted ADR; land after #404 merges (or stack on it).
- Cross-repo: homelab env template + `REVERIE_MIGRATOR_PASSWORD` secret + ansible is a separate cold-audited pass (memory: cross-repo plans need other-side audit).
