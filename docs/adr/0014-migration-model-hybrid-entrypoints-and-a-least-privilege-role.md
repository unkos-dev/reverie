---
type: ADR
profile-version: 1
id: "REV-ADR-0014"
title: "Migration model: hybrid entrypoints and a least-privilege role"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-02"
decision-makers:
  - "John Unkovich"
---

# Migration model: hybrid entrypoints and a least-privilege role

## Context and problem statement

Reverie's schema evolves across releases, so the application must apply database migrations reliably. Three
questions define the model: under what database **identity** migrations run, through what **invocation** path, and
with what **failure semantics**.

The answer is shaped by the audience and threat model. Operators run Reverie as Docker Compose (the majority for a
Postgres-backed app), bare `docker run`, or Kubernetes; the common upgrade must stay one-command. The instance is
treated as exposed and multi-user, so the long-lived web process must not carry database credentials more powerful
than it needs at runtime. And because migration failures are inevitable over a project's life, recovery must be as
simple as pinning the previous image tag and restarting.

## Decision drivers

- Least privilege: the web process should hold no credential it does not need at runtime, ideally none for schema
  management.
- Migrations need only DDL and trusted extensions: `pg_trgm` and `pgcrypto` are trusted (PG13+), installable by any
  non-superuser role with `CREATE` on the database; no migration performs a superuser-only operation. A dedicated
  least-privilege role is therefore sufficient.
- One-command upgrades for the compose majority: `docker compose up -d` must remain the whole upgrade procedure.
- Compose contract stability: the shipped compose topology is a contract; changing it post-release forces every
  operator to hand-edit on upgrade, and pre-release is the only cost-free moment to fix the v1 shape.
- Recoverable failures: a failed migration must leave the database untouched so recovery is "pin the old tag and
  restart".

## Considered options

- Migration identity: cluster superuser
- Migration identity: dedicated `reverie_migrator` role
- Migration identity: reuse a runtime role (`reverie_app`)
- Migration invocation: in-process, always-on
- Migration invocation: out-of-band only
- Migration invocation: hybrid invocation
- Transaction semantics: all-or-nothing batch transaction
- Transaction semantics: per-migration transactions (sqlx default)
- Transaction semantics: dry-run preflight
- Schema-version safety: schema-ahead detection
- Schema-version safety: no version check

## Decision outcome

Chosen option: **dedicated `reverie_migrator` role, hybrid invocation, all-or-nothing batch transaction, and
schema-ahead detection**, because each dimension independently satisfies its own decision driver while the four
share one migration runner and one failure story.

### Identity

Chosen option: **dedicated `reverie_migrator` role**, because it isolates schema management from the cluster
superuser at no cost to migration content. `init-roles.sql` provisions a non-superuser role, created
`NOSUPERUSER NOCREATEROLE NOBYPASSRLS`, that owns the schema objects (created by it on the initial migration) with
`CREATE` on the database: sufficient for the trusted extensions and all DDL. `DATABASE_URL_MIGRATION` uses this
role, sourced from a `REVERIE_MIGRATOR_PASSWORD` secret. The bootstrap superuser is used only by first-boot
`init-roles.sql` and never appears in any application or migration container environment thereafter.

Cluster superuser was rejected: it places the highest-privilege credential in a long-lived process for no
functional gain. Reusing a runtime role was rejected: it collapses the privilege separation the architecture relies
on.

### Invocation

Chosen option: **hybrid invocation**, because it keeps compose upgrades one command while still giving bare
`docker run` operators an escape hatch. Both entrypoints delegate to one `db::run_migrations`. The shipped
`docker/compose.staging.yml` runs a one-shot `reverie-migrate` service, with the app gated by
`depends_on: { reverie-migrate: { condition: service_completed_successfully } }`. In this default topology the app
container holds no DDL credentials: `DATABASE_URL_MIGRATION` is set only on the short-lived migrate service. The
compose upgrade path stays one command: `docker compose pull && docker compose up -d` runs the migrate service to
completion, then the app.

The opt-in `REVERIE_AUTO_MIGRATE=true` flag restores single-process behaviour for bare `docker run` operators not
using the shipped compose, who then accept carrying the (non-superuser) migration credential in the app environ.

In-process-only was rejected: it forces migration credentials into the long-lived process for every deployment
style. This is the shape the earlier always-on auto-migrate decision took. Out-of-band-only was rejected: it leaves
bare-`docker run` operators with a mandatory two-step upgrade and no escape hatch; the opt-in flag avoids that
cheaply.

Even when it does not migrate, the app's startup retains the schema-ahead and checksum read check described below,
so an app older than the database refuses to serve with a clear message instead of cryptic SQL errors.

### Transaction semantics

Chosen option: **all-or-nothing batch transaction**, because a partial failure (for example three of five
migrations applied) would leave the database in a state where neither the old nor the new image works, requiring
manual SQL, and PostgreSQL's transactional DDL makes an all-or-nothing batch reliable. A custom runner wraps sqlx's
embedded `Migrator`; all pending migrations execute in one `BEGIN`/`COMMIT`, and any failure rolls the batch back to
the pre-migration state. With all-or-nothing, the operator pins the previous tag, restarts, and the app works
because the database was never mutated.

Migrations marked `-- no-transaction` (for `CREATE INDEX CONCURRENTLY` and some `ALTER TYPE ... ADD VALUE`) run
individually after the batch commits. Transactional migrations run first in version order, then no-transaction
migrations in version order; an interleaving such as `[M1(tx), M2(no-tx), M3(tx)]` is safe only when M3 does not
depend on M2, which is enforced at review when a `-- no-transaction` migration is added.

Per-migration transactions and dry-run preflight were rejected for partial-state failure and doubled migration time
respectively (see Pros and cons of the options).

### Schema-version safety

Chosen option: **schema-ahead detection**, extended to a bidirectional check plus a checksum comparison, because a
one-directional check only catches half of the operator errors this model creates. On startup the runner compares
the binary's embedded migration list against `_sqlx_migrations`; if the database holds rows unknown to the binary,
startup fails with a clear "schema is newer than this application: upgrade the image or roll back the database"
message. It also verifies each applied migration's stored checksum against the embedded file's SHA-384 hash, failing
on mismatch and naming the offending version.

In the out-of-band default (`REVERIE_AUTO_MIGRATE=false`) the application does not migrate, so at startup it instead
runs a read-only schema check that is fail-closed in both directions: it refuses to serve when the database is ahead
of the binary, and when the binary is ahead of the database, which is an operator who deployed a new image but has
not yet run `reverie migrate`. The schema-behind direction is the more common operator error and, left undetected,
surfaces as scattered runtime SQL failures against missing columns rather than a single legible startup refusal; the
bare `docker run` path has no compose gating, so this check is the only backstop there. It also reports a
never-migrated database (no migration history) as a distinct "not initialized" error rather than a raw
missing-relation failure. The check is read-only (`SELECT` on `_sqlx_migrations`) and holds no migration credential.

### Connection and concurrency

The runner opens an ephemeral pool (max one connection), migrates, then drops it before runtime pools initialise, so
the migration identity holds no connection during request serving. Concurrent starts are serialised by a PostgreSQL
advisory lock matching sqlx's internal lock ID, acquired via `pg_try_advisory_lock` in a bounded retry loop (about
30 seconds) rather than a blocking `pg_advisory_lock`; failure to acquire fails startup with a clear error. The
ephemeral connection sets `lock_timeout=30s` to bound heavyweight lock waits, an interim default pending a
project-wide database lock and timeout strategy.

### Logging

Interim levels pending project-wide logging conventions:

| Scenario                                       | Level | Message                                                                                       |
| ---------------------------------------------- | ----- | --------------------------------------------------------------------------------------------- |
| No pending migrations                          | DEBUG | `database schema is up to date`                                                               |
| Migrations applied                             | INFO  | `applied {n} pending migrations ({elapsed}ms)`                                                |
| Individual migration applying                  | DEBUG | `applying migration {version} ({name})`                                                       |
| Schema ahead of binary                         | ERROR | `database schema is newer than this application version` plus recovery guidance               |
| Schema behind binary (out-of-band app start)   | ERROR | `database schema is older than this application — run reverie migrate` plus recovery guidance |
| Never-migrated database (no migration history) | ERROR | `database is not initialized (no migration history) — run reverie migrate first`              |
| Batch migration failure                        | ERROR | `migration batch failed: {error}` plus batch recovery guidance                                |
| No-tx migration SQL failure                    | ERROR | `no-transaction migration failed: {version} ({name})` plus no-tx recovery                     |
| No-tx tracking INSERT failure                  | ERROR | `no-transaction migration {version} ({name}) applied but tracking failed`                     |

Recovery guidance distinguishes batch failure ("pin the previous image tag: database is untouched"), no-tx SQL
failure ("transactional migrations already committed; fix forward"), and no-tx tracking failure ("the migration IS
applied; do not revert, manually insert the tracking row").

### Consequences

- Positive: in the default topology, the web process carries zero schema-management credentials.
- Positive: the migration identity is least-privilege, so a compromised migrate step can DDL its own schema, not
  manage roles or read other databases.
- Positive: the common upgrade path stays one command.
- Positive: a failed migration surfaces as a non-zero migrate-service exit with isolated logs, not an app
  crash-loop with the error buried in startup output.
- Positive: all-or-nothing rollback and schema-ahead detection keep recovery to "pin the old tag and restart".
- Positive: the v1 compose contract is settled pre-release.
- Negative: bare `docker run` operators must run two steps on a migration upgrade or set `REVERIE_AUTO_MIGRATE`
  (then carry the migration credential in the app environ).
- Negative: two invocation paths exist over one runner, which is more surface than a single always-on path.
- Negative: the custom runner couples to sqlx's `_sqlx_migrations` schema and must be re-verified on sqlx bumps.
- Negative: a version-skew window exists if `depends_on` ordering is bypassed (a manual "restart just the app");
  mitigated by the advisory lock, the bidirectional schema-divergence check, and backward-compatible migration
  discipline.
- Negative: object ownership belongs to `reverie_migrator`; this is automatic only on a fresh database, and an
  existing database with objects owned by another role needs a one-time `REASSIGN OWNED` or a recreate.

## Pros and cons of the options

### Dedicated `reverie_migrator` role

- Positive: least-privilege isolates schema-management from the cluster superuser.
- Positive: feasible with zero migration-content changes, since the extensions in use are trusted.
- Neutral: adds one role and one secret.
- Negative: object ownership must be `reverie_migrator`, automatic only on a fresh database; an existing database
  owned by another role needs `REASSIGN OWNED` or a recreate.

### Cluster superuser

- Positive: zero new roles or secrets.
- Negative: the highest-privilege credential ends up in the application process environ, unacceptable for the
  threat model.

### Hybrid invocation

- Positive: the app holds no DDL credentials in the default topology.
- Positive: one-command upgrades for the compose majority.
- Positive: clean failure isolation for the migrate step.
- Negative: bare-`docker run` upgrades are two-step unless the flag is set.

### Out-of-band only

- Positive: the app never holds DDL credentials in any topology.
- Negative: no single-process escape hatch, so bare-`docker run` upgrades are always two-step.

### All-or-nothing batch transaction

- Positive: failure leaves the database untouched, so pinning the old image is enough to recover.
- Neutral: adds roughly 60-80 lines of custom runner code.
- Negative: `-- no-transaction` migrations cannot join the batch.

### Per-migration transactions (sqlx default)

- Positive: zero custom code.
- Negative: partial-state failure leaves the database where neither image works.

## More information

Related ADR: [Persist operator-tunable settings to database with live
reload](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md).

Related ADR: [tower-sessions sqlx store](../../adr/superseded/2026-05-08-tower-sessions-sqlx-store.md).

Bare `docker run` operators either run the image with the `migrate` argument (wait for exit, then run the server)
or set `REVERIE_AUTO_MIGRATE=true`. The shipped compose handles this automatically.

Semver and release notes: pre-v1.0 the schema is freely mutable. Post-v1.0, additive migrations are MINOR and
destructive ones MAJOR; the migration runs transparently either way, and the changelog communicates impact.

`start_period`: while migrating, the container is "starting"; operators using `HEALTHCHECK` must set `start_period`
to cover migration duration, and data-backfill migrations should document expected duration in release notes.

Revisit conditions:

- If sqlx merges batch-transaction mode
  ([launchbadge/sqlx#3770](https://github.com/launchbadge/sqlx/discussions/3770)), evaluate replacing the custom
  runner, including whether to extract the batch migration runner as a public crate.
- If a "bring your own managed Postgres" path is added, re-examine `reverie_migrator`'s grants (PG15+ needs `CREATE`
  on schema `public` for `CREATE EXTENSION ... WITH SCHEMA public`) and trusted-extension availability.
- If operators routinely set `REVERIE_AUTO_MIGRATE=true`, reconsider whether out-of-band should remain the shipped
  default.
- A `NOBYPASSRLS` owner is blocked by any future `FORCE ROW LEVEL SECURITY` cross-tenant backfill; re-examine the
  role's grants if that need arises.
