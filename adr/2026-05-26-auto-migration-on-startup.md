---
status: proposed
date: 2026-05-26
decision-makers: junkovich
consulted: []
informed: []
---

# Auto-migrate database on startup with all-or-nothing batch transactions

## Context and Problem Statement

Reverie's database migrations are manual-only. Operators must run `sqlx migrate run` against the schema-owner DSN before (re)starting the application. Nothing in the startup path enforces or automates this.

On 2026-05-26, the staging instance (`reverie-dev.unkos.net`) entered a restart loop after the `:main` image was rebuilt with code from PR #330 (persisted settings, 11f). The new image expected a `settings` table that the 8-day-old staging database did not have. The container logged `relation "settings" does not exist` on every restart attempt and never reached a healthy state.

Self-hosted applications in the same category (Gitea, Immich, Kavita, Paperless-ngx, Jellyfin) auto-migrate on startup. Their users pull a new image, restart the container, and the schema updates transparently. Reverie's manual-migration requirement is a deviation from this convention that creates operator friction and outage risk.

How should Reverie apply database migrations at startup, and what transaction semantics should protect the operator from partial-migration failures?

Related: [persisted settings ADR](2026-05-26-persisted-settings.md) (assumes "migrations auto-run on startup" in its consequences), [tower-sessions ADR](2026-05-08-tower-sessions-sqlx-store.md) (schema-owner migration pattern).

## Decision Drivers

- **Self-hosted audience**: operators manage Reverie via Docker Compose, not CI pipelines. Pull-and-run must work without manual DB ops.
- **4-role Postgres architecture**: schema owner (`reverie`) runs migrations; runtime roles (`reverie_app`, `reverie_ingestion`, `reverie_readonly`) have minimum privilege. Migration connection must use schema-owner credentials and must not leak into runtime.
- **Failure mode matters more than failure rate**: migration failures are inevitable over the lifecycle of the project. The operator's recovery path must be simple: pin the previous image tag and restart. This requires the database to be untouched when a migration fails.
- **Security best practice**: dedicated migration role with elevated privileges, separate from the application role ([industry convention](https://github.com/launchbadge/sqlx/discussions/3770), consistent with Reverie's existing role separation).
- **Seamless UX for average user, details for those who look**: migrations should be invisible in the happy path. Operators who care can read structured logs.

## Considered Options

### Migration lifecycle

1. **Auto-migrate on startup, always-on** — no operator action needed, no opt-out
2. **Auto-migrate with opt-out** (`REVERIE_AUTO_MIGRATE=false`) — operator can disable for manual control
3. **Manual-only** (status quo) — operator runs `sqlx migrate run` before starting the app

### Transaction semantics

1. **All-or-nothing batch transaction** — wrap all pending migrations in one `BEGIN`/`COMMIT`; failure rolls back everything
2. **Per-migration transactions** (sqlx default) — each migration in its own transaction; failure leaves partial state
3. **Dry-run preflight** — run in a transaction, verify, rollback, then run for real

### Schema-version safety

1. **Schema-ahead detection** — refuse startup if DB has migrations unknown to the binary
2. **No version check** — let the app start and fail with SQL errors if schema is incompatible

## Decision Outcome

### Migration lifecycle: Always-on auto-migrate (option 1)

No opt-out knob. Reverie's audience is Docker Compose self-hosters, not Kubernetes fleet operators with Flyway CI pipelines. No operator in the target audience has a reason to disable auto-migration. The opt-out adds code, documentation, and testing surface for a scenario that does not exist.

Manual-only (status quo) rejected: caused the staging outage that motivated this ADR and deviates from self-hosted OSS conventions.

Opt-out rejected: YAGNI. Adds complexity for a fictional user profile.

### Transaction semantics: All-or-nothing batch transaction (option 1)

A custom migration runner wraps sqlx's embedded `Migrator` struct. All pending migrations execute within a single `BEGIN`/`COMMIT`. If any migration fails, the entire batch rolls back and the database is untouched at the pre-migration state.

This is the critical operator-experience decision. With per-migration transactions, a failure at migration 3 of 5 leaves migrations 1–2 committed and 3–5 unapplied. The database is in a state where neither the old image nor the new image works. The operator must manually intervene with SQL. With all-or-nothing, the operator pins the previous image tag, restarts, and the app works — the database was never mutated.

PostgreSQL supports transactional DDL (`CREATE TABLE`, `ALTER TABLE`, `DROP` all roll back cleanly), making this possible. MySQL cannot do this.

Migrations marked with `-- no-transaction` (required for `CREATE INDEX CONCURRENTLY` and some `ALTER TYPE ... ADD VALUE` operations) run outside the batch individually, after the batch commits. These are rare and flagged at review time.

Per-migration transactions (sqlx default) rejected: partial-state failure mode is unacceptable for self-hosted operators without DB expertise.

Dry-run preflight rejected: doubles migration time, doesn't catch all issues (non-idempotent operations), abandoned by tools that tried it (early Flyway).

### Schema-ahead detection (option 1)

On startup, the runner compares the binary's embedded migration list against the `_sqlx_migrations` table. If the database contains migration rows unknown to the binary, startup fails with:

> database schema (migration 20260527...) is newer than this application version — upgrade the image or roll back the database manually

This prevents the confusing failure mode where a stale image hits tables/columns it doesn't understand and throws cryptic SQL errors.

### Connection architecture

A new required environment variable `DATABASE_URL_MIGRATION` provides the schema-owner DSN. The runner opens an ephemeral connection pool (max 1 connection), runs migrations, then drops the pool before runtime pools are initialised. The schema-owner connection never exists during request serving.

This follows the existing `DATABASE_URL` / `DATABASE_URL_INGESTION` pattern and aligns with the security best practice of dedicated migration roles with elevated privileges.

### Concurrency safety

sqlx's migrator acquires a PostgreSQL advisory lock before running migrations. The custom runner replicates this. Multiple containers starting simultaneously (e.g., Docker restart race) are serialised by the advisory lock — only one runs migrations, others wait, then proceed.

### Lock timeout

The ephemeral migration connection sets `lock_timeout=30s` to prevent a migration blocked on a lock from hanging startup indefinitely. This is an interim hardcoded default pending a project-wide database lock and timeout strategy ([UNK-296](https://linear.app/unkos/issue/UNK-296)).

### Logging

Logging levels follow the project-wide conventions being formalised under [UNK-297](https://linear.app/unkos/issue/UNK-297). Interim levels:

| Scenario                     | Level | Message                                                                      |
| ---------------------------- | ----- | ---------------------------------------------------------------------------- |
| No pending migrations        | DEBUG | `database schema is up to date`                                              |
| Migrations applied           | INFO  | `applied {n} pending migrations ({elapsed}ms)`                               |
| Individual migration applied | DEBUG | `applied migration {version} ({name})`                                       |
| Schema ahead of binary       | ERROR | `database schema is newer than this application version` + recovery guidance |
| Migration failure            | ERROR | `migration failed: {error}` + recovery guidance                              |

Recovery guidance in ERROR messages: `pin the previous image tag to restore service, then fix forward with a new release`.

### Consequences

- Good, because operators pull a new image and restart — schema updates are invisible in the happy path
- Good, because all-or-nothing transactions mean a failed migration leaves the database untouched — pin the old image to recover
- Good, because schema-ahead detection catches stale-image deployments with a clear message instead of cryptic SQL errors
- Good, because ephemeral migration connection maintains the RLS security boundary — schema-owner privileges never exist during request serving
- Good, because advisory lock makes concurrent container starts safe by default
- Bad, because custom migration runner (~60–80 lines) couples to sqlx's `_sqlx_migrations` table schema — must be verified on sqlx version bumps
- Bad, because `-- no-transaction` migrations (enum ADD VALUE, CONCURRENTLY indexes) run outside the batch and cannot be rolled back atomically with the rest
- Neutral, because adds one required env var (`DATABASE_URL_MIGRATION`) — consistent with existing multi-DSN pattern

### Semver and release-notes implications

Pre-v1.0: schema is freely mutable (existing convention). Migrations are transparent — users auto-update and the app handles it.

Post-v1.0: migrations that are purely additive (new tables, new columns with defaults, new indexes) are MINOR bumps. Destructive migrations (column drops, type changes, data reshaping) are MAJOR bumps. The migration itself runs transparently in both cases — the semver signal and release notes communicate the impact, not the runtime.

Auto-update tools (Watchtower, Renovate) surface version changes. Changelogs (generated by `release-please`) document migration impact. The app itself does not alarm the user — it just works.

## Implementation Plan

### Affected paths

- `backend/src/config.rs` — add `migration_database_url: String` (required, from `DATABASE_URL_MIGRATION`)
- `backend/src/db.rs` — add `run_migrations(url: &str) -> Result<MigrationReport, MigrationError>` with batch transaction runner, schema-ahead detection, advisory lock, and `lock_timeout` configuration
- `backend/src/lib.rs` — insert migration call between `Config::from_env()` (line 221) and `db::init_pool()` (line 274)
- `docker-compose.yml` — no change needed (dev DSN already uses schema owner `reverie`)
- `backend/CLAUDE.md` — update "Run migrations as schema owner" section, document `DATABASE_URL_MIGRATION`

### Dependencies

No new crates. `sqlx` (already present) provides `Migrator`, `Migration`, and `PgPool`. The custom runner uses these types directly.

Future: [UNK-299](https://linear.app/unkos/issue/UNK-299) tracks potential extraction as a standalone crate once battle-tested.

### Patterns to follow

- `sqlx::migrate!()` macro for compile-time migration embedding (already used by `#[sqlx::test]`)
- Advisory lock acquisition matching sqlx's internal lock ID for interop
- `_sqlx_migrations` table schema matching sqlx's format (version, description, installed_on, success, checksum, execution_time)
- Ephemeral pool pattern: `PgPoolOptions::new().max_connections(1).connect(url)` → use → drop
- Error types via `thiserror` per backend conventions

### Patterns to avoid

- Do NOT use `sqlx::Migrator::run()` — it uses per-migration transactions, not batch
- Do NOT keep the migration pool alive after migrations complete — drop it before runtime pool init
- Do NOT add an opt-out env var — always-on is the deliberate design
- Do NOT log migration SQL content — may contain sensitive DDL comments or role names
- Do NOT fall back to `DATABASE_URL` when `DATABASE_URL_MIGRATION` is unset — the 4-role architecture has no single-role scenario

### Configuration

| Env var                  | Required | Default | Purpose                         |
| ------------------------ | -------- | ------- | ------------------------------- |
| `DATABASE_URL_MIGRATION` | Yes      | —       | Schema-owner DSN for migrations |

No other new env vars. `lock_timeout` is hardcoded at 30s (interim, pending [UNK-296](https://linear.app/unkos/issue/UNK-296)).

### Migration steps

This ADR ships after migration rollup PR #333 merges. The rollup consolidates 27 migrations into one initial schema. Staging's `_sqlx_migrations` table must be reset to match the consolidated migration as part of the rollup — otherwise schema-ahead detection would false-positive on the 27 historical rows unknown to the new binary.

Deployment sequence:

1. PR #333 (migration rollup) merges — includes `_sqlx_migrations` reset guidance for staging
2. This ADR's implementation PR merges
3. Staging adds `DATABASE_URL_MIGRATION` to its compose env
4. Next image pull auto-migrates — no manual `sqlx migrate run` ever again

### Verification

- [ ] `cargo test` passes — all existing tests unaffected (`#[sqlx::test]` still uses sqlx's built-in migrator)
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo sqlx prepare --workspace --check` reports no drift
- [ ] Fresh database: startup applies all migrations, logs `applied N pending migrations`, app serves requests
- [ ] Up-to-date database: startup logs `database schema is up to date` at DEBUG, app serves requests
- [ ] Migration failure (e.g., invalid SQL injected into a test migration): startup fails with ERROR, database has no partial state (all-or-nothing rollback verified by inspecting `_sqlx_migrations` row count)
- [ ] Schema-ahead detection: insert a fake future row into `_sqlx_migrations`, verify startup refuses with clear error message
- [ ] `DATABASE_URL_MIGRATION` unset: startup fails with `missing required environment variable: DATABASE_URL_MIGRATION`
- [ ] `lock_timeout` effective: verify migration connection has `lock_timeout = '30s'` via `SHOW lock_timeout` in test
- [ ] Concurrent starts: two processes starting simultaneously — advisory lock serialises, both succeed, no duplicate migration rows
- [ ] Ephemeral pool: migration pool is dropped before runtime pools are created (verify by connection count or pool lifecycle logging at DEBUG)
- [ ] `-- no-transaction` migration: a migration with the marker runs outside the batch, after the batch commits

## Pros and Cons of the Options

### Always-on auto-migrate

- Good, because pull-and-run works without manual DB ops
- Good, because matches self-hosted OSS conventions (Gitea, Immich, Kavita, Paperless-ngx)
- Good, because eliminates the class of outage that motivated this ADR
- Neutral, because operators who want pre-migration review can check changelogs before pulling the new image
- Bad, because no escape hatch for operators who want manual control — but no such operator exists in the target audience

### Auto-migrate with opt-out

- Good, because flexibility for advanced operators
- Bad, because adds code, docs, and test surface for a fictional user profile
- Bad, because the opt-out itself becomes a support surface ("I set AUTO_MIGRATE=false and now the app won't start")

### Manual-only (status quo)

- Good, because explicit control
- Bad, because caused the staging outage
- Bad, because deviates from self-hosted conventions
- Bad, because requires operators to know about `sqlx migrate run` — leaks implementation detail

### All-or-nothing batch transaction

- Good, because failure leaves DB untouched — pin old image to recover
- Good, because PostgreSQL transactional DDL makes this reliable
- Good, because fills a gap in the sqlx ecosystem (no existing solution, [sqlx #3770](https://github.com/launchbadge/sqlx/discussions/3770))
- Neutral, because ~60–80 lines of custom runner code
- Bad, because couples to `_sqlx_migrations` schema (stable across sqlx versions, but must verify on bumps)
- Bad, because `-- no-transaction` migrations cannot participate in the batch

### Per-migration transactions (sqlx default)

- Good, because zero custom code — `Migrator::run()` just works
- Bad, because partial-state failure mode — DB left in limbo where neither old nor new image works
- Bad, because recovery requires manual SQL intervention by operators who may not have DB expertise

### Dry-run preflight

- Good, because validates before committing
- Bad, because doubles migration time
- Bad, because non-idempotent operations fail on the real run after dry-run side effects
- Bad, because abandoned by tools that tried it (early Flyway)

## More Information

**Restart-loop behaviour**: with `restart: unless-stopped` (Docker) or `restartPolicy: Always` (Kubernetes), a persistent migration failure causes the container to hammer Postgres in a restart loop. The ERROR log message includes recovery guidance to break the loop by pinning the previous image tag. Future enhancement: exponential backoff on repeated migration failure (not in scope for this ADR).

**PostgreSQL extension privileges**: migrations `CREATE EXTENSION pg_trgm`. In the bundled-postgres scenario, `POSTGRES_USER=reverie` makes `reverie` the cluster superuser — works. For any future "bring your own Postgres" path (RDS, Supabase, Crunchy), trusted extensions need DB-owner; non-trusted need SUPERUSER. Not in scope today; noted for future operator documentation.

**`start_period` consideration**: while migrations run, the container is "starting." Pre-v1.0 schema evolution is freely mutable; a future data-backfill migration could exceed Docker's default `start_period` (30s). Migrations that include data backfill should note the expected duration and recommend bumping `start_period` in the release notes.

**Revisit conditions:**

- If sqlx merges [#3770](https://github.com/launchbadge/sqlx/discussions/3770) (batch transaction mode), evaluate replacing the custom runner with the upstream feature
- If [UNK-299](https://linear.app/unkos/issue/UNK-299) evaluation is positive, extract the runner as a standalone crate
- If [UNK-296](https://linear.app/unkos/issue/UNK-296) (lock strategy ADR) changes the recommended `lock_timeout`, update the hardcoded 30s default
- If [UNK-297](https://linear.app/unkos/issue/UNK-297) (logging conventions ADR) changes level semantics, update the logging table

**Linear issues:**

- [UNK-296](https://linear.app/unkos/issue/UNK-296) — ADR: database lock and timeout strategy
- [UNK-297](https://linear.app/unkos/issue/UNK-297) — ADR: project-wide logging conventions
- [UNK-298](https://linear.app/unkos/issue/UNK-298) — review ADR cluster collectively
- [UNK-299](https://linear.app/unkos/issue/UNK-299) — evaluate extracting batch migration runner as public crate
