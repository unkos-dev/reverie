---
status: "accepted"
date: 2026-06-02
supersedes: ["superseded/2026-05-26-auto-migration-on-startup.md"]
decision-makers: junkovich
consulted: []
informed: []
---

# Database migration model: hybrid entrypoints, least-privilege role, all-or-nothing batch

## Context and Problem Statement

Reverie's schema evolves across releases, so the application must apply database
migrations reliably. Three questions define the model: under what database
**identity** migrations run, through what **invocation** path, and with what
**failure semantics**.

The answer is shaped by the audience and threat model. Operators run Reverie as
Docker Compose (the majority for a Postgres-backed app), bare `docker run`, or
Kubernetes; the common upgrade must stay one-command. The instance is treated as
exposed and multi-user (hard rule 6), so the long-lived web process must not
carry database credentials more powerful than it needs at runtime. And because
migration failures are inevitable over a project's life, recovery must be as
simple as pinning the previous image tag and restarting.

This ADR supersedes an earlier migration decision; see
[More Information](#more-information).

Related: [persisted settings ADR](2026-05-26-persisted-settings.md),
[tower-sessions ADR](2026-05-08-tower-sessions-sqlx-store.md),
[UNK-296](https://linear.app/unkos/issue/UNK-296) (lock/timeout strategy),
[UNK-297](https://linear.app/unkos/issue/UNK-297) (logging conventions).

## Decision Drivers

- **Least privilege.** The web process should hold no credential it does not need
  at runtime — ideally none for schema management.
- **Migrations need only DDL + trusted extensions.** `pg_trgm` and `pgcrypto` are
  trusted (PG13+), installable by any non-superuser role with `CREATE` on the
  database; no migration performs a superuser-only operation. A dedicated
  least-privilege role is therefore sufficient.
- **One-command upgrades for the compose majority.** `docker compose up -d` must
  remain the whole upgrade procedure.
- **Compose contract stability.** The shipped compose topology is a contract;
  changing it post-release forces every operator to hand-edit on upgrade.
  Pre-release is the only cost-free moment to fix the v1 shape.
- **Recoverable failures.** A failed migration must leave the database untouched
  so recovery is "pin the old tag and restart".

## Considered Options

### Migration identity

1. **Cluster superuser** — the bundled `POSTGRES_USER`.
2. **Dedicated non-superuser role `reverie_migrator`** — `LOGIN`, owns the schema
   objects, `CREATE` on the database; not `SUPERUSER`, not `BYPASSRLS`.
3. **Reuse a runtime role** (`reverie_app`).

### Migration invocation

1. **In-process, always-on** — the app migrates itself on every startup.
2. **Out-of-band only** — a `reverie migrate` step runs before the app; the app
   never migrates and never holds migration credentials.
3. **Hybrid** — both entrypoints share one runner: a `reverie migrate` subcommand
   (out-of-band) plus an opt-in `REVERIE_AUTO_MIGRATE` startup flag.

### Transaction semantics

1. **All-or-nothing batch** — all pending migrations in one `BEGIN`/`COMMIT`; any
   failure rolls the whole batch back.
2. **Per-migration transactions** (sqlx default) — a failure leaves partial state.
3. **Dry-run preflight** — run, verify, roll back, then run for real.

### Schema-version safety

1. **Schema-ahead detection** — refuse startup if the database holds migrations
   unknown to the binary.
2. **No version check.**

## Decision Outcome

### Identity — dedicated `reverie_migrator` (option 2)

`init-roles.sql` provisions a non-superuser role that owns the schema objects
(created by it on the initial migration) with `CREATE` on the database —
sufficient for the trusted extensions and all DDL. `DATABASE_URL_MIGRATION` uses
this role, sourced from a `REVERIE_MIGRATOR_PASSWORD` secret. The bootstrap
superuser is used only by first-boot `init-roles.sql` and never appears in any
application or migration container environment thereafter.

Cluster superuser rejected: it places the highest-privilege credential in a
long-lived process for no functional gain. Reusing a runtime role rejected: it
collapses the privilege separation the architecture relies on.

### Invocation — hybrid, out-of-band default (option 3)

Both entrypoints delegate to one `db::run_migrations`. The shipped
`docker-compose.yml` runs a one-shot `reverie-migrate` service, with the app
gated by `depends_on: { reverie-migrate: { condition:
service_completed_successfully } }`. In this default topology the **app container
holds no DDL credentials** — `DATABASE_URL_MIGRATION` is set only on the
short-lived migrate service. The compose upgrade path stays one command:
`docker compose pull && docker compose up -d` runs the migrate service to
completion, then the app.

The opt-in `REVERIE_AUTO_MIGRATE=true` flag restores single-process behaviour for
bare `docker run` operators not using the shipped compose, who then accept
carrying the (non-superuser) migration credential in the app environ.

In-process-only rejected: forces migration credentials into the long-lived
process for every deployment style. Out-of-band-only rejected: leaves
bare-`docker run` operators with a mandatory two-step upgrade and no escape
hatch; the opt-in flag avoids that cheaply.

Even when it does not migrate, the app's startup retains the schema-ahead /
checksum read check (below), so an app older than the database refuses to serve
with a clear message instead of cryptic SQL errors.

### Transaction semantics — all-or-nothing batch (option 1)

A custom runner wraps sqlx's embedded `Migrator`; all pending migrations execute
in one `BEGIN`/`COMMIT`, and any failure rolls the batch back to the
pre-migration state. This is the decisive operator-experience property: a
partial failure (e.g. 3 of 5 applied) would leave the database in a state where
neither the old nor the new image works, requiring manual SQL — with
all-or-nothing the operator pins the previous tag, restarts, and the app works
because the database was never mutated. PostgreSQL transactional DDL makes this
reliable.

Migrations marked `-- no-transaction` (for `CREATE INDEX CONCURRENTLY` and some
`ALTER TYPE ... ADD VALUE`) run individually after the batch commits.
**Ordering invariant**: transactional migrations run first (version order), then
no-transaction migrations (version order); an interleaving like `[M1(tx),
M2(no-tx), M3(tx)]` is safe only when M3 does not depend on M2. Enforced at
review when a `-- no-transaction` migration is added.

Per-migration and dry-run rejected: partial-state failure and doubled migration
time respectively (see Pros and Cons).

### Schema-version safety — bidirectional schema-divergence detection + checksum (option 1)

On startup the runner compares the binary's embedded migration list against
`_sqlx_migrations`; if the database holds rows unknown to the binary, startup
fails with a clear "schema is newer than this application — upgrade the image or
roll back the database" message. It also verifies each applied migration's stored
checksum against the embedded file's SHA-384 hash, failing on mismatch and
naming the offending version.

In the out-of-band default (`REVERIE_AUTO_MIGRATE=false`) the application does not
migrate, so at startup it instead runs a read-only schema check that is
fail-closed in **both** directions: it refuses to serve when the database is ahead
of the binary (as above) AND when the binary is ahead of the database — an
operator who deployed a new image but has not yet run `reverie migrate`. The
schema-behind direction is the more common operator error and, left undetected,
surfaces as scattered runtime SQL failures against missing columns rather than a
single legible startup refusal; the bare `docker run` path has no compose gating,
so this check is the only backstop there. The check is read-only (SELECT on
`_sqlx_migrations`) and holds no migration credential.

### Connection and concurrency

The runner opens an ephemeral pool (max 1 connection), migrates, then drops it
before runtime pools initialise — the migration identity holds no connection
during request serving. Concurrent starts are serialised by a PostgreSQL
advisory lock matching sqlx's internal lock ID, acquired via
`pg_try_advisory_lock` in a bounded retry loop (~30s) rather than a blocking
`pg_advisory_lock`; failure to acquire fails startup with a clear error. The
ephemeral connection sets `lock_timeout=30s` to bound heavyweight lock waits — an
interim default pending [UNK-296](https://linear.app/unkos/issue/UNK-296).

### Logging

Interim levels pending [UNK-297](https://linear.app/unkos/issue/UNK-297):

| Scenario                                     | Level | Message                                                                                    |
| -------------------------------------------- | ----- | ------------------------------------------------------------------------------------------ |
| No pending migrations                        | DEBUG | `database schema is up to date`                                                            |
| Migrations applied                           | INFO  | `applied {n} pending migrations ({elapsed}ms)`                                             |
| Individual migration applying                | DEBUG | `applying migration {version} ({name})`                                                    |
| Schema ahead of binary                       | ERROR | `database schema is newer than this application version` + recovery guidance               |
| Schema behind binary (out-of-band app start) | ERROR | `database schema is older than this application — run reverie migrate` + recovery guidance |
| Batch migration failure                      | ERROR | `migration batch failed: {error}` + batch recovery guidance                                |
| No-tx migration SQL failure                  | ERROR | `no-transaction migration failed: {version} ({name})` + no-tx recovery                     |
| No-tx tracking INSERT failure                | ERROR | `no-transaction migration {version} ({name}) applied but tracking failed`                  |

Recovery guidance distinguishes batch failure ("pin the previous image tag —
database is untouched"), no-tx SQL failure ("transactional migrations already
committed — fix forward"), and no-tx tracking failure ("the migration IS applied;
do NOT revert — manually insert the tracking row").

### Consequences

- Good, because in the default topology the web process carries zero
  schema-management credentials.
- Good, because the migration identity is least-privilege: a compromised migrate
  step can DDL its own schema, not manage roles or read other databases.
- Good, because the common upgrade path stays one command.
- Good, because a failed migration surfaces as a non-zero migrate-service exit
  with isolated logs, not an app crash-loop with the error buried in startup output.
- Good, because all-or-nothing rollback and schema-ahead detection keep recovery
  to "pin the old tag and restart".
- Good, because the v1 compose contract is settled pre-release.
- Bad, because bare `docker run` operators must run two steps on a migration
  upgrade or set `REVERIE_AUTO_MIGRATE` (then carry the migration credential in
  the app environ).
- Bad, because two invocation paths exist over one runner — more surface than a
  single always-on path.
- Bad, because the custom runner couples to sqlx's `_sqlx_migrations` schema and
  must be re-verified on sqlx bumps.
- Neutral, because a version-skew window exists if `depends_on` ordering is
  bypassed (manual "restart just the app"); mitigated by the advisory lock, the
  bidirectional schema-divergence check (which refuses on both schema-ahead and
  schema-behind), and backward-compatible migration discipline.
- Neutral, because object ownership belongs to `reverie_migrator`. On a fresh
  database this is automatic; an existing database with objects owned by another
  role needs a one-time `REASSIGN OWNED` or a recreate. `prp-plan` must confirm
  staging's actual object ownership (`\dt` + `pg_class.relowner`) before choosing.

### Confirmation

- `reverie_migrator` is created `NOSUPERUSER NOCREATEROLE NOBYPASSRLS` (verifiable
  in `init-roles.sql` and via `pg_roles`).
- In the shipped `docker-compose.yml`, `DATABASE_URL_MIGRATION` appears only on
  the `reverie-migrate` service, never on `reverie` (grep-checkable).
- A test asserts the migration set contains no superuser-only operation, keeping
  the non-superuser role sufficient as migrations are added.
- The out-of-band app startup check refuses on both schema-ahead and schema-behind
  divergence (a test asserts each direction errors), so a forgotten `reverie
migrate` is a legible startup failure, not silent runtime errors.

## Pros and Cons of the Options

### Dedicated `reverie_migrator` role

- Good, because least-privilege isolates schema-management from the cluster superuser.
- Good, because feasible with zero migration-content changes (trusted extensions).
- Neutral, because adds one role and one secret.
- Bad, because object ownership must be `reverie_migrator` — automatic only on a
  fresh DB; an existing DB owned by another role needs `REASSIGN OWNED` or recreate.

### Cluster superuser

- Good, because zero new roles or secrets.
- Bad, because the highest-privilege credential ends up in the application process
  environ — unacceptable for the threat model.

### Hybrid invocation

- Good, because the app holds no DDL creds in the default topology.
- Good, because one-command upgrades for the compose majority.
- Good, because clean failure isolation for the migrate step.
- Bad, because bare-`docker run` upgrades are two-step unless the flag is set.

### Out-of-band only

- Good, because the app never holds DDL creds in any topology.
- Bad, because no single-process escape hatch — bare-`docker run` upgrades are
  always two-step.

### All-or-nothing batch transaction

- Good, because failure leaves the DB untouched — pin the old image to recover.
- Neutral, because ~60–80 lines of custom runner code.
- Bad, because `-- no-transaction` migrations cannot join the batch.

### Per-migration transactions (sqlx default)

- Good, because zero custom code.
- Bad, because partial-state failure leaves the DB where neither image works.

## More Information

Supersedes
[the 2026-05-26 auto-migration ADR](superseded/2026-05-26-auto-migration-on-startup.md)
(moved to `adr/superseded/`, retained for history). The `superseded/` subfolder
is the convention for tombstoned ADRs.

**Bare `docker run` operators** either run the image with the `migrate` argument
(wait for exit, then run the server) or set `REVERIE_AUTO_MIGRATE=true`. The
shipped compose handles this automatically.

**Semver / release notes.** Pre-v1.0 the schema is freely mutable. Post-v1.0,
additive migrations are MINOR and destructive ones MAJOR; the migration runs
transparently either way, and the changelog communicates impact.

**`start_period`.** While migrating, the container is "starting"; operators using
HEALTHCHECK must set `start_period` to cover migration duration, and
data-backfill migrations should document expected duration in release notes.

**Revisit conditions:**

- If sqlx merges batch-transaction mode
  ([#3770](https://github.com/launchbadge/sqlx/discussions/3770)), evaluate
  replacing the custom runner; [UNK-299](https://linear.app/unkos/issue/UNK-299).
- If a "bring your own managed Postgres" path is added, re-examine
  `reverie_migrator`'s grants (PG15+ needs `CREATE` on schema `public` for
  `CREATE EXTENSION ... WITH SCHEMA public`) and trusted-extension availability.
- If operators routinely set `REVERIE_AUTO_MIGRATE=true`, reconsider whether
  out-of-band should remain the shipped default.

**For `prp-plan` to verify on a real instance** (not inherit): staging object
ownership; PG15+ `public`-schema grant; a `NOBYPASSRLS` owner is blocked by any
future `FORCE ROW LEVEL SECURITY` cross-tenant backfill. Cross-repo surface —
reverie: `init-roles.sql`, `config.rs`, `migrate` subcommand,
`docker-compose.yml`, `docker/staging.env.runtime.example`, `backend/CLAUDE.md`;
homelab: env templates, `REVERIE_MIGRATOR_PASSWORD` secret, ansible.
