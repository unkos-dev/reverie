---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0023"
title: "Database migration model"
satisfies:
  - "REV-REQ-0062"
  - "REV-REQ-0063"
governed-by:
  - "REV-ADR-0014"
---

# Database migration model

This Design covers how Reverie's schema is applied and verified: the least-privilege `reverie_migrator` identity, the
four code paths that apply or check migrations against it (the `reverie migrate` subcommand, the opt-in in-process
auto-migrate step, the `#[sqlx::test]` harness the backend test suite runs against, and the `just db-migrate-raw` recipe
used in local development), the advisory-lock-guarded all-or-nothing batch transaction the runner wraps pending
migrations in, the read-only `verify_schema_current` startup gate, every `MigrationError` variant and the operator
action each names, and the decode-range `CHECK` constraint convention every first-party `TIMESTAMPTZ` column follows.

## Purpose and boundaries

This subject owns `backend/src/db.rs`'s migration half: `run_migrations`, `verify_schema_current`, `MigrationError`, the
advisory-lock retry loop, and the batch-transaction runner (`run_migrations_inner`, `run_locked`, `release_lock`). It
owns the two production call sites in `backend/src/lib.rs` — `run_migrate` (the `reverie migrate` subcommand) and
`apply_or_verify_schema` (the startup branch `run()` calls, selecting between auto-migrate and verify by
`Config::auto_migrate`). CLI dispatch itself, including the `Command::Migrate` arm that reaches `run_migrate`, belongs
to "Application runtime: startup, workers, and shutdown". It owns the `reverie_migrator` role's definition in
`docker/init-roles.sql` (creation flags, its `CONNECT`, database-level `CREATE`, and `USAGE, CREATE ON SCHEMA public`
grants), though that file also provisions the three RLS-scoped runtime roles, which belong to "Row-level security and
database context". It owns the deployment wiring that drives the two out-of-band entrypoints:
`docker/compose.staging.yml`'s one-shot `reverie-migrate` service and its `service_completed_successfully` gate on the
`reverie` service, `docker/staging.env.migrate.example`, and the `db-migrate` recipe in the justfiles for local
development. It owns the sibling `db-migrate-raw` recipe, which applies pending migrations directly through sqlx-cli's
`cargo sqlx migrate run` against the same `reverie_migrator`-authenticated DSN, one migration at a time rather than
inside this subject's batch transaction; the recipe's own doc comment states it "is not: the deployment path", naming
`reverie migrate` (run via `db-migrate`) as the path real instances use. It owns the
`#[sqlx::test(migrations = "./migrations")]` harness as a fourth, test-only way migrations reach a database, and the
secondary-role pool helpers those tests build against the per-test database (`crate::test_support::db::app_pool_for` and
its siblings). It owns the decode-range `CHECK` constraint convention and its coverage test,
`backend/src/db.rs::tests::every_timestamptz_column_has_decode_range_check`.

It does not own the SQL the migrations contain: the tables, columns, row-level-security policies and grants they create
are schema-design content, owned by "Row-level security and database context" for the RLS-relevant objects and by the
individual feature Designs for the rest. This subject describes only how that SQL is applied and verified, not what it
says. It does not own database roles and grants beyond `reverie_migrator` itself — the RLS-scoped roles (`reverie_app`,
`reverie_ingestion`, `reverie_readonly`), every row-level-security policy, and the pools that connect as those roles are
"Row-level security and database context". It does not own startup order: where `apply_or_verify_schema` falls in
`run()`'s sequence relative to the other startup steps, worker spawn, and the listening socket is "Application runtime".
It does not own how `REVERIE_AUTO_MIGRATE` and `DATABASE_URL_MIGRATION` are parsed into `Config::auto_migrate` and
`Config::migration_database_url` — that gate is "Configuration loading"'s Gate 1 (`backend/src/config/mod.rs`), named
here only as the call `apply_or_verify_schema` and `run_migrate` make against its output.

Depends on: `sqlx::migrate!("./migrations")`, which embeds `backend/migrations/*.up.sql` (and `.down.sql`, unused at
runtime) at compile time — this subject never reads migration files from disk; "Configuration loading" for
`auto_migrate` and `migration_database_url`; `docker/init-roles.sql` for the `reverie_migrator` role's existence.

Depended on by: `run()` in `backend/src/lib.rs`, which cannot proceed past `apply_or_verify_schema` to registering any
route or spawning any worker; "Row-level security and database context", which relies on this subject for the schema
objects, policies and grants the migrations create as content, and for the least-privilege proof that `reverie_migrator`
holds no cluster-wide authority; and every `#[sqlx::test]`-based module across the backend test suite, each of which
provisions its database through the harness path this subject documents.

## Structure

### The `reverie_migrator` identity

`docker/init-roles.sql` creates `reverie_migrator WITH LOGIN PASSWORD :'mig_pw' NOSUPERUSER NOCREATEROLE NOBYPASSRLS`,
alongside the three RLS-scoped roles the neighbouring Design owns. The script grants it `CONNECT` on the database and,
separately, `USAGE, CREATE ON SCHEMA public` — both are load-bearing: database-level `CREATE` alone lets it run
`CREATE SCHEMA tower_sessions` (the initial migration's own statement), but PostgreSQL 15 removed the implicit `CREATE`
on schema `public` from `PUBLIC`, so the schema-level grant is what lets it create tables and trusted extensions inside
`public`. Every schema object the migrations create is owned by whichever role ran the migration that created it: on a
fresh database that is always `reverie_migrator`, because nothing else ever connects with a DDL credential before the
first migration runs. "Row-level security and database context" carries the same ownership fact in its database-role
table, and covers the consequence (an owner is exempt from its own tables' RLS policies unless
`FORCE ROW LEVEL SECURITY` is set, which no migration sets).

### The runner: advisory lock and batch transaction

`run_migrations_inner` (`backend/src/db.rs`) is the one function both production entrypoints share. On its single
acquired connection it runs `SET lock_timeout = '30s'`, reads `current_database()`, and computes an advisory lock id
with `generate_lock_id`: a CRC-32 (ISO-HDLC) checksum of the database name multiplied by a fixed magic constant, the
same formula sqlx's own internal migration lock uses. It then loops `pg_try_advisory_lock` up to ten times, sleeping
three seconds between attempts (nine intervals, about twenty-seven seconds of total wait), rather than blocking on
`pg_advisory_lock`; failing to acquire the lock in that budget returns `MigrationError::LockTimeout` without ever having
touched the schema.

Holding the lock, `run_locked` runs `CREATE TABLE IF NOT EXISTS _sqlx_migrations (...)` (mirroring sqlx's own tracking
schema exactly, so the table sqlx's compile-time `query!` macros and the read-only verifier expect always exists once
any migration has applied), loads the applied `(version, checksum)` pairs, and diffs them against the versions
`sqlx::migrate!` embedded via the shared `diverged_versions` set comparison. An applied version absent from the embedded
set fails closed with `MigrationError::SchemaAhead` before anything else runs. For every embedded migration already
recorded as applied, it compares the stored checksum byte-for-byte against the embedded file's SHA-384 hash and fails
with `MigrationError::ChecksumMismatch` on the first difference — this comparison runs only here, inside the runner; see
Failure and recovery for what that means for the read-only startup path.

Pending migrations then partition on each embedded `Migration`'s `no_tx` field (set by sqlx's own migration-file
resolver from a `-- no-transaction` header comment; no migration in `backend/migrations/` carries one). Transactional
migrations run first, in version order, inside one manual `BEGIN` … `COMMIT` on the locked connection: each migration's
SQL runs via `sqlx::raw_sql`, then its tracking row is inserted, and any failure in that loop issues `ROLLBACK` and
returns `MigrationError::BatchFailed` — because PostgreSQL's DDL is itself transactional, the rollback undoes both the
failed migration's schema changes and every successful one earlier in the same batch. Any pending no-transaction
migrations then run individually, after the batch commits, each with its own tracking insert; a SQL failure here returns
`MigrationError::NoTxFailed` (the migration did not apply), while a tracking-insert failure after successful SQL returns
the distinct `MigrationError::NoTxTrackingFailed` (the migration did apply and must not be reverted). `release_lock`
runs on every exit path, successful or not, so a failed run never leaves the advisory lock held for the session's
lifetime.

### `verify_schema_current`: the default startup gate

`verify_schema_current` runs on the ordinary `reverie_app` pool and holds no migration credential. It probes
`to_regclass('public._sqlx_migrations') IS NOT NULL` first, so a never-migrated database returns
`MigrationError::NotInitialized` instead of a raw missing-relation failure from the version query that would otherwise
run next. It then reads `version FROM _sqlx_migrations WHERE success = true`, diffs that set against the embedded
versions with the same `diverged_versions` comparison the runner uses, and fails closed on either direction: an
applied-but-unembedded version is `MigrationError::SchemaAhead`, an embedded-but-unapplied version is
`MigrationError::SchemaBehind`. It performs no checksum comparison at all — see Failure and recovery.

### The four entrypoints

- **`reverie migrate`** (`run_migrate` in `backend/src/lib.rs`, dispatched from `Command::Migrate` in `main.rs`) reads
  only `DATABASE_URL_MIGRATION` via `resolve_migration_dsn`, builds no `Config`, and calls `db::run_migrations`
  directly. It installs its own best-effort `tracing_subscriber` because it never reaches `run()`, where the subscriber
  is normally installed. This is the path `docker/compose.staging.yml`'s one-shot `reverie-migrate` service runs
  (`command: ["migrate"]`, appended to the image's `ENTRYPOINT ["reverie-api"]`), scoped to `.env.migrate` so the
  credential never reaches the `.env.runtime`-scoped `reverie` service, and the path the `db-migrate` recipe in the
  justfiles runs locally against the dev socket DSN.
- **Auto-migrate at startup** (`apply_or_verify_schema` in `backend/src/lib.rs`, called from `run()` once the
  `reverie_app` pool exists) takes this branch only when `Config::auto_migrate` is true, in which case it requires
  `Config::migration_database_url` to be `Some` and calls the same `db::run_migrations`. When `auto_migrate` is false —
  the default path — it calls `verify_schema_current` on the `reverie_app` pool instead, never touching
  `run_migrations`. Where this step falls in `run()`'s full startup sequence, relative to worker spawn and the listening
  socket, belongs to "Application runtime".
- **The `#[sqlx::test]` harness** applies migrations through neither of the above. The attribute macro (from the pinned
  `sqlx` crate) expands to `sqlx::testing::TestFn::run_test`, which creates a fresh `_sqlx_test_<hash>` database through
  a master pool connected with the ambient `DATABASE_URL` — in local development and CI that is the bootstrap role
  `reverie` (`test_support.rs`'s own doc comment: "owned by the schema owner (`reverie` — bypasses RLS)") — and then
  calls the embedded `Migrator`'s own `run_direct` method on a fresh connection to that database. `run_direct` is sqlx's
  built-in migration applier: it does not call `db::run_migrations`, `run_locked`, or anything in this crate, so a test
  provisioned this way exercises none of the advisory lock, the batch-vs-no-tx split, or any `MigrationError` variant. A
  handful of tests in `backend/src/db.rs` instead prove properties of the real `reverie_migrator` identity directly:
  `reverie_migrator_can_apply_full_migration_set` grants a fresh per-test database's `CREATE` privileges to
  `reverie_migrator` (since a `#[sqlx::test]` database starts with none of `init-roles.sql`'s grants) and then calls
  `db::run_migrations` against a `reverie_migrator`-authenticated DSN built from the test pool's own connect options.
- **`just db-migrate-raw`** applies pending migrations directly through sqlx-cli's `cargo sqlx migrate run` against the
  same `reverie_migrator`-authenticated DSN `db-migrate` uses, bypassing `db::run_migrations` entirely: sqlx-cli commits
  each migration individually rather than inside one batch transaction. The recipe exists so a branch authoring a new
  migration can regenerate the sqlx offline cache before the backend binary can compile; its own doc comment states it
  "is not: the deployment path", naming `reverie migrate` (via `db-migrate`) as the path real instances use.

### The `_sqlx_migrations` writers

| Writer                            | Path                                      | Role connected as               |
| --------------------------------- | ----------------------------------------- | ------------------------------- |
| Batch-transaction tracking insert | `run_locked`, inside the `BEGIN`/`COMMIT` | `reverie_migrator`              |
| No-transaction tracking insert    | `run_locked`, after the batch commits     | `reverie_migrator`              |
| The `#[sqlx::test]` harness       | sqlx's own `Migrator::run_direct`         | the ambient `DATABASE_URL` role |
| `just db-migrate-raw`             | sqlx-cli's own `cargo sqlx migrate run`   | `reverie_migrator`              |

On a deployed topology, `run_locked` is the only writer that inserts into `_sqlx_migrations`; the table's other two rows
are the `#[sqlx::test]` harness, which only ever runs against a test-only per-test database, and `db-migrate-raw`,
documented as a local-development recipe rather than restricted by anything in tooling to a non-deployed target.
`reverie_app` holds only `SELECT` on the table, granted by the initial migration itself
(`GRANT SELECT ON TABLE public._sqlx_migrations TO reverie_app`), which is what lets `verify_schema_current` read it
without a migration credential.

### The `TIMESTAMPTZ` decode-range convention

Every first-party `TIMESTAMPTZ` column in `backend/migrations/20260810000000_initial_schema.up.sql` carries a `CHECK`
constraint named `<table>_<column>_ts_decode_range`, bounding the column between the year `0001` and the year `10000`,
across both the `public` and `tower_sessions` schemas. PostgreSQL's `TIMESTAMPTZ` range extends to year 294276 and
includes the `-infinity`/`infinity` specials, none of which `chrono::DateTime<Utc>` can represent; an out-of-range value
read through `sqlx`'s decode path panics rather than returning an error. The constraint rejects such a value at write
time instead, so the panicking decode path is unreachable for any column the constraint covers as long as every
first-party column carries one. `every_timestamptz_column_has_decode_range_check` (`backend/src/db.rs`) enforces that
going forward: it scans `information_schema.columns` for every `timestamp with time zone` column in `public` and
`tower_sessions` (excluding `_sqlx_migrations`, which sqlx itself manages and only ever writes via `now()`), anti-joins
against `pg_constraint` for a `CHECK` whose definition contains both bound literals, and fails the migration this landed
in — since it is a `#[sqlx::test(migrations = "./migrations")]` test, it re-runs against every migration set the tree
ever ships — if a new `TIMESTAMPTZ` column has no matching constraint.

## Interfaces and dependencies

- `db::run_migrations` and `db::verify_schema_current`, each returning a `Result` around `MigrationError`, are the two
  public entry points `backend/src/lib.rs` calls; the success value of the former, `MigrationReport`, carries
  `applied: usize` and `elapsed_ms: u128`.
- The CLI surface (`parse_command` in `backend/src/lib.rs`) is not binary: besides no arguments (`Command::Serve`) and
  exactly `migrate` (`Command::Migrate`, the arm that reaches this subject), four further subcommands
  (`print-config-schema`, `bootstrap`, `reset-password <email>`, `unlock-account <email>`) dispatch to their own
  `Command` variants outside this subject's ownership. Only a token outside that set of six, or a wrong argument count,
  is a parse error rather than a silent fall-through to `Command::Serve`.
- Environment: `DATABASE_URL_MIGRATION` (the `reverie_migrator` DSN, required by `run_migrate` unconditionally and by
  `apply_or_verify_schema` only when `auto_migrate` is true) and `REVERIE_AUTO_MIGRATE` (parsed into
  `Config::auto_migrate`, default `false`) — both owned by "Configuration loading", named here as the contract this
  subject consumes.
- `sqlx::migrate!("./migrations")` is the compile-time embedding boundary: the migration SQL, its filename-derived
  version and description, its SHA-384 checksum, and its `no_tx` flag are all fixed at build time, not read from the
  filesystem at runtime.
- `docker/compose.staging.yml`'s `reverie-migrate` service waits on `reverie-postgres` reporting `service_healthy`, and
  the `reverie` service in turn waits on `reverie-migrate` reporting `service_completed_successfully`; that pair of
  `depends_on` conditions is the compose-level contract that makes `docker compose pull && up -d` a correct one-command
  upgrade in the repository-provided Compose topology.

## Data and state

- **`_sqlx_migrations`** (schema `public`): one row per applied migration, carrying its version, description, install
  timestamp, success flag, SHA-384 checksum, and execution time. Durable, forward-only: no code path deletes or updates
  a row (the tests that do so mutate it directly to construct a failure scenario). Its writers are listed above; every
  read this subject performs filters `WHERE success = true`, mirroring the fact that the runner only ever inserts rows
  with `success = true` (a failed migration's row is never written, because the whole batch rolls back).
- **The advisory lock**: session-scoped PostgreSQL server state, not a row in any table. Its id is a pure function of
  the database name, so a rerun against the same database always computes the same id, and two different databases (for
  example a production database and a `#[sqlx::test]` database) cannot collide on it. It is released explicitly on every
  exit path and, because it is session-scoped rather than transaction-scoped, is also released automatically if the
  holding connection closes for any other reason.
- **`lock_timeout`**: a session GUC set fresh to `'30s'` at the start of every run; not read from configuration, not
  persisted, and not shared with any other subject's lock-timeout setting.
- **`Config::auto_migrate`** (`bool`, default `false`) and **`Config::migration_database_url`** (`Option<String>`):
  owned by "Configuration loading", consumed here. When `auto_migrate` is `false`, the loader forces
  `migration_database_url` to `None` regardless of what `DATABASE_URL_MIGRATION` holds in the process environment, so a
  value left in the runtime environment by mistake cannot reach `apply_or_verify_schema`'s auto-migrate branch unless
  the operator also sets `REVERIE_AUTO_MIGRATE=true`.

## Runtime behaviour

**The default path: `reverie migrate` then `verify_schema_current`.**

1. `docker compose pull && docker compose up -d` starts `reverie-postgres`, waits for it to report healthy, then starts
   `reverie-migrate` with `DATABASE_URL_MIGRATION` from `.env.migrate` alone.
2. `run_migrate` resolves the DSN, calls `db::run_migrations`, which opens a one-connection pool bounded by
   `MIGRATION_CONNECT_TIMEOUT` (thirty seconds — the same budget sqlx's pool would apply by default, stated explicitly
   so shortening it reads as a deliberate change rather than an inherited default; this is also the effective
   startup-race tolerance for a database still running `initdb` on a fresh volume, since the health check in this
   topology is the only readiness gate).
3. `run_migrations_inner` sets `lock_timeout`, acquires the advisory lock, and `run_locked` finds every embedded
   migration unapplied on a fresh database, runs them all inside the batch transaction, and returns a report whose
   `applied` count equals the full embedded set.
4. The migrate container exits zero; the `service_completed_successfully` condition in compose releases the `reverie`
   service to start.
5. `run()` builds the `reverie_app` pool, then `apply_or_verify_schema` sees `auto_migrate = false` and calls
   `verify_schema_current` on that pool: the catalog probe finds `_sqlx_migrations`, the version-set diff is empty in
   both directions, and startup continues.

**Concurrent runners racing the advisory lock**, as `concurrent_starts_serialize` exercises it: two calls to
`run_migrations_inner` join on clones of the same pool against a fresh database. One acquires `pg_try_advisory_lock` on
its first attempt and proceeds through `run_locked`; the other retries in the bounded loop and, once the lock frees,
itself calls `run_locked` — but by then every embedded migration is already recorded as applied, so its
`tx_pending`/`no_tx_pending` partitions are both empty and it commits nothing. The `CREATE TABLE IF NOT EXISTS` each one
issues at the top of `run_locked` never races the other, because the advisory lock already serialises the two
`run_locked` calls: by the time either reaches that statement, the other is not concurrently inside `run_locked` at all.
Applied counts across both runs sum to the full embedded set, and `_sqlx_migrations` holds exactly that many rows — no
duplicate, no partial batch from the loser.

**Auto-migrate startup**, for an operator running bare `docker run` with `REVERIE_AUTO_MIGRATE=true`: `run()` builds the
`reverie_app` pool exactly as above, then `apply_or_verify_schema` takes the `auto_migrate = true` branch, requires
`config.migration_database_url` (a startup failure names `DATABASE_URL_MIGRATION` explicitly if it is absent, since the
configuration loader's own gate refuses to build a `Config` in that state), and calls `db::run_migrations` — the same
function and the same advisory-lock/batch-transaction path the out-of-band invocation uses. The long-lived process holds
`DATABASE_URL_MIGRATION` in its own environment for as long as it runs, which is the trade-off this branch exists to let
an operator with no orchestration accept deliberately.

**The `#[sqlx::test]` harness**, for an ordinary backend integration test: the macro expansion creates
`_sqlx_test_<hash>` as the ambient `DATABASE_URL` role, connects to it, and calls the embedded `Migrator`'s own
`run_direct` method — sqlx's own applier, running the same embedded SQL this subject's runner would, but through sqlx's
internal apply logic rather than `run_locked`. The test then receives a `PgPool` connected as that same ambient role,
which owns every object just created and so is exempt from the RLS policies those objects carry; a test that needs
RLS-enforced behaviour reconnects as `reverie_app` (or another runtime role) via `crate::test_support::db::app_pool_for`
and its siblings against the same per-test database.

## Failure and recovery

- **`MigrationError::Connection`**: the ephemeral migration pool could not connect. Nothing was attempted; the message
  never includes DSN credentials (`resolve_migration_dsn` and the connect path pass only the parsed error through).
- **`MigrationError::SessionSetup`**: the post-connect `lock_timeout` or advisory-lock query itself failed. The database
  is untouched.
- **`MigrationError::LockTimeout`**: the advisory lock was not acquired within ten attempts. The message reports that
  another instance may be migrating; nothing was attempted, and no lock is held to release.
- **`MigrationError::BatchFailed`**: a transactional migration's SQL, a tracking insert inside the batch, or the
  `COMMIT` itself failed. The first two roll the batch back, leaving the database exactly as it was before the run
  started. The third does not settle the question: a `COMMIT` that reaches the server and succeeds but whose
  acknowledgement never arrives returns this same variant, and the batch is applied. Recovery therefore begins by
  reconnecting and reading `_sqlx_migrations`. Pinning the previous image tag is right only once the batch's versions
  are absent from it, because a batch that did commit meets that older image as `SchemaAhead` and refuses to start.
- **`MigrationError::NoTxFailed`**: a `-- no-transaction` migration's SQL failed after the batch already committed. Any
  transactional migrations in this run are already applied and cannot be un-applied by pinning the old tag; the operator
  must fix the failing SQL and re-deploy.
- **`MigrationError::NoTxTrackingFailed`**: a `-- no-transaction` migration's SQL succeeded but its tracking insert
  failed. The schema change is live, so reverting it is not the fix. The message says so explicitly and directs the
  operator to insert the tracking row by hand.
- **`MigrationError::SchemaAhead`**: the database has a migration version this binary does not know. Both `run_locked`
  and `verify_schema_current` refuse in this direction; recovery is upgrading the image or manually rolling back the
  database.
- **`MigrationError::SchemaBehind`**: only `verify_schema_current` returns this — `run_locked` treats every
  embedded-but-unapplied version as ordinary pending work and applies it instead of erroring. It is the "forgot to run
  `reverie migrate`" case: on the default topology this is the fail-closed refusal that stands in for what would
  otherwise be scattered runtime failures against missing columns.
- **`MigrationError::NotInitialized`**: `_sqlx_migrations` does not exist yet. Both the catalog probe in
  `verify_schema_current` and the message itself point the operator at `reverie migrate`, rather than surfacing
  Postgres's raw missing-relation error.
- **`MigrationError::VerificationRead`**: a read failed during `verify_schema_current`. Because that function runs on
  the `reverie_app` pool, not the migration DSN, the message names the likeliest cause as a database migrated before the
  `SELECT` grant on `_sqlx_migrations` existed, alongside ordinary connectivity failure.
- **`MigrationError::ChecksumMismatch`**: `run_locked` detects that an already-applied migration's stored checksum no
  longer matches the embedded file. This comparison exists only in the write path: `verify_schema_current`, the check
  every ordinary server start performs on the default topology, compares version sets alone and never reads or compares
  a checksum. A tampered or hand-edited already-applied migration file is therefore caught the next time
  `reverie migrate` runs, not by an ordinary server restart.
- **A `TIMESTAMPTZ` value outside the decode-range `CHECK`**: rejected by PostgreSQL at `INSERT`/`UPDATE` time with a
  constraint-violation error, before the value is ever stored — the alternative, a value the application cannot decode
  reaching `sqlx`'s row-decode path and panicking the request that reads it back, is what the constraint exists to
  prevent.

## Security and operations

Two separate guarantees keep the `reverie_migrator` credential away from request serving, and they hold over different
things. The repository-provided Compose topology keeps it out of the environment: `docker/compose.staging.yml` scopes
`DATABASE_URL_MIGRATION` to the one-shot `reverie-migrate` service's `env_file: .env.migrate` alone, so the long-lived
`reverie` service's environment never carries it. The configuration loader keeps it out of the `Config`: it forces
`Config::migration_database_url` to `None` whenever `auto_migrate` is false, so the DSN reaches no pool and no startup
branch even on a load where the variable is present.

Neither guarantee substitutes for the other. The loader controls the configuration, not the environment, so an operator
who sets `DATABASE_URL_MIGRATION` on the serving service leaves that credential readable in the process environment
whatever the flag says; what the flag decides is whether the process uses it. Setting `REVERIE_AUTO_MIGRATE=true` is
what makes it used: the long-lived process then holds the `reverie_migrator` credential for its entire run, the explicit
trade-off for an operator with no orchestration to sequence a separate migrate step.

`reverie_migrator`'s least-privilege posture is proved by two tests independent of each other:
`migrator_role_is_least_privilege` reads `pg_roles` and asserts the role is not `rolsuper`, `rolcreaterole`, or
`rolbypassrls`; `migration_set_has_no_superuser_only_operations` statically scans every embedded migration's SQL for
`ALTER SYSTEM`, `CREATE ROLE`/`CREATE USER`, `CREATE EVENT TRIGGER`, or a `CREATE EXTENSION` naming anything other than
the trusted `pg_trgm`, `pgcrypto`, or `unaccent`. Together they show the identity holds no cluster-wide authority and
that nothing in the migration set would need more than it has. Being the table owner still exempts `reverie_migrator`
from the RLS policies those tables carry, since none sets `FORCE ROW LEVEL SECURITY`; "Row-level security and database
context" covers that consequence for the roles that matter at request time.

Bypassing the `depends_on` ordering compose applies — for example, restarting only the `reverie` container without a
preceding successful `reverie-migrate` run — opens a version-skew window that this subject's two independent defences
narrow but do not eliminate at the instant of the restart: `verify_schema_current`'s fail-closed refusal on
`SchemaBehind` still runs (the app refuses to serve rather than running against a schema it does not expect), and the
advisory lock still serialises against any migration that is concurrently in flight.

## More information

- [Database migrations](../../../../docs/deployment/database-migrations.md): the operator-facing runbook for running
  migrations across bare `docker run`, compose, and the auto-migrate opt-in.
- "Row-level security and database context": its database-role table names `reverie_migrator`'s privileges alongside the
  RLS-scoped roles.
