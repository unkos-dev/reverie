# Backend

This directory contains the Rust Axum backend. Its runtime behaviour is recorded in the
[Designs](../docs/specs/index.md).

## Development database

`just db-up` starts the persistent development cluster in `docker/compose.dev.yml`. The shared provisioning script
generates five independent credentials once and retains them at
`${XDG_STATE_HOME:-$HOME/.local/state}/reverie/postgres/<compose-project>/credentials.env`. The directory is mode 0700
and the file is mode 0600. Provisioning is serialised by a stable lock; restarts reuse the complete state. A volume
without its original credential state refuses startup. Restore that state or coordinate an explicit reset.

Run `just db-migrate` before starting the server, or use `just dev-up` for migrations, the API and Vite. The dev loader
resolves the process environment, then the parsed `REVERIE_DEV_ENV` file (default `~/reverie/dev/env`), then retained
state for absent database inputs required by the command. Migration recipes need only the migration DSN; server recipes
need runtime and ingestion DSNs, plus the migration DSN when automatic migration is enabled. Explicit empty assignments
stay empty; without a non-empty matching file alias, required credentials fail application validation. Omit database
assignments from a local env file to use retained state. Supplying a command's required external DSNs needs no local
state; a conflicting bootstrap `POSTGRES_PASSWORD` is rejected. Operators supply their own role-specific DSNs through
deployment tooling or credential files selected by process environment variables.

`just db-down` preserves the volume and credentials. `just db-reset <confirmed-volume>` permanently deletes that
volume's data and recreates it with retained valid credentials. Confirm the resolved project, container and mounted
volume, and coordinate all consumers before resetting. A code revert cannot recover deleted data. Neither dev readers
nor verification commands rotate development credentials.

### Credential files

The credential file aliases are `DATABASE_URL_FILE`, `DATABASE_URL_MIGRATION_FILE`, `DATABASE_URL_INGESTION_FILE`,
`OIDC_CLIENT_SECRET_FILE`, `REVERIE_GOOGLEBOOKS_API_KEY_FILE`, `REVERIE_HARDCOVER_API_TOKEN_FILE`,
`REVERIE_SMTP_PASSWORD_FILE` and `REVERIE_MAIL_QUEUE_KEY_FILE`. Each selects a UTF-8 file readable inside the process or
container. Empty direct and file variables are absent; two non-empty sources conflict before reading, including when
automatic migration is disabled. Whitespace-only variables remain present.

The reader strips all trailing LF and CRLF terminators and preserves all other text, including spaces, interior newlines
and a lone CR. Empty contents after stripping are absent for these settings; optional metadata credentials remain unset.
Existing required and blank checks apply. Errors identify the setting without paths, contents or raw I/O details.
Unknown file aliases, including the bootstrap password alias, are ignored.

With automatic migration disabled, the migration file is never opened and the field remains unset. `reverie migrate`
reads only its migration source, and schema output opens no credential files. The dev loader yields generated database
defaults to non-empty matching file aliases without reading the files. External sources can satisfy a command without
retained dev state. `db-migrate-raw` accepts direct DSNs only. To roll back, restore direct variables and remove their
file aliases before reverting the application.

### Authentication mail and recovery

The [mail and recovery guide](../docs/deployment/mail-and-recovery.md) covers optional SMTP, secret files, relay
submission checks, mailbox verification, administrator and host links, and queue-key maintenance. The host
`reset-password` command reads only its database credential and `REVERIE_PUBLIC_URL`; `send-test-email` needs only
transport, sender and support settings. Neither command needs ingestion configuration.

### Bulk retries

Administrators with admin scope can retry one current group with either endpoint:

- `POST /api/v1/dashboard/enrichment-failures/retries` accepts `{ "source": "openlibrary", "class": "timeout" }`. Use
  `"none"` for a group without a source. Selection matches the primary source and failure class of the group reads,
  excludes active claims, and preserves provider retry deadlines. The single-book trigger keeps its existing timing
  reset behaviour.
- `POST /api/v1/ingestion/inputs/retries` accepts `{ "reason": "needs_change" }` or `{ "reason": "retries_exhausted" }`.
  The coordinator resets and schedules only that class, preserving readiness, publication exclusions and ingestion pause
  conditions. It does not scan the folders or retry other classes.

Both return HTTP 202 with `{ "queued": 500, "remaining": 1 }`, capped at 500 resets per request. `queued` counts
persisted resets, including work waiting for readiness, a provider deadline or ingestion to resume. `remaining` counts
members still in the selected group after the reset; an empty group returns zero for both fields. Browser sessions
require the existing CSRF token. Input command admission and reply share a five-second timeout; accepted commands may
finish after the caller times out. A failed response from either endpoint can leave the outcome uncertain, so refresh
the group before trying again.

### Verification ownership and transports

Database verification requires access to a running Docker daemon.

Each `just rust::test`, `doctests`, `sqlx-check`, `sqlx-prepare`, `schema-check` and `schema-dump` invocation owns a
disposable cluster with independent credentials, a dynamic loopback TCP port and a unique socket directory. Tests and
doctests compile against the committed SQLx cache. Test mode leaves the bootstrap database without migrations; SQLx
creates and migrates each test database. Schema mode prepares its bootstrap database for online compilation and SQLx
cache checks. Schema dumping uses a migrator-owned scratch database within its disposable cluster. CI uses the same
owner; its online Clippy and documentation commands receive prepared clusters.

Both TCP and Unix sockets require SCRAM authentication. Verification uses its owned socket when available and its owned
TCP endpoint across a Docker host boundary. Local migration recipes default to the Unix socket so they also work in
network-isolated sandboxes that block TCP loopback. The runtime server uses TCP, matching the deployed transport.
Development publishes `127.0.0.1:5432` and the socket at `${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock`. Socket
DSNs use the params-only form `postgres:///database?host=<socket-dir>&user=<role>&password=<password>`. SQLx rejects
`postgres://user@/database?host=...`, and a socket DSN never falls back to TCP.

The foreground owner forwards INT/TERM, waits for children and removes its socket contents, container, disposable
storage and private credential directory. A failed child remains failed; a cleanup failure is an infrastructure failure,
including when cargo-mutants reports findings. SIGKILL, host failure or Docker daemon loss can leave resources; the
reported resource identities support manual cleanup. Verification never resets development data.

Older checkouts with socket/default-password recipes fail against this SCRAM cluster. Upgrade verification recipes or
invoke commands from the older checkout through the updated provisioning script's absolute path, using
`bash <updated-checkout>/scripts/postgres-provision.sh test -- <command>`. For an older dev consumer, use a private
shell to load the updated `scripts/backend-dev-env.sh` by its absolute path and invoke the consumer with explicit DSNs.
Avoid echoing or storing these values in the checkout. `REVERIE_DEV_DB_URL` cannot redirect updated verification
recipes.

### Roles

`docker/init-roles.sql` requires all four nonempty role-password inputs before creating any application role. The
PostgreSQL image also requires the bootstrap password. Development and verification supply generated values.

| Role                | Purpose                                           |
| ------------------- | ------------------------------------------------- |
| `reverie`           | Cluster bootstrap and SQLx test provisioning.     |
| `reverie_migrator`  | Applies migrations and owns schema objects.       |
| `reverie_app`       | Serves web traffic under RLS.                     |
| `reverie_ingestion` | Runs background pipelines under its RLS policies. |
| `reverie_readonly`  | Queries permitted data for debugging.             |

The `tower_sessions` schema bypasses RLS. The session id resolves user identity. Role grants control access. The
`reverie_app` role receives DML access, `reverie_readonly` can read only the `expiry_date` column, and
`reverie_ingestion` receives no access.

Normal server startup requires `DATABASE_URL_INGESTION` with the dedicated `reverie_ingestion` credentials. Missing,
empty or whitespace-only values refuse startup before pools, workers or serving. Set the variable and restart; there is
no application-role fallback. Bootstrap, reset-password and unlock-account do not require ingestion credentials.
Migration uses `DATABASE_URL_MIGRATION`; schema printing requires no credentials.

### Migrations

The `reverie_migrator` role executes migrations out of band: `just db-migrate`, `just db-migrate-raw` and
`just rust::migrate` use the retained migrator credentials over the development socket, or set `DATABASE_URL_MIGRATION`
to either transport's migrator DSN and run `cargo run -- migrate`. `just db-migrate-raw` applies `backend/migrations/`
with sqlx-cli without building the backend; it is an authoring shortcut, not a substitute for the shipped runner.

`just db-migrate` and `just rust::migrate` compile against the committed SQLx cache, so a fresh development database
needs no schema for compilation. If queries or migrations change, regenerate the cache with `just rust::sqlx-prepare`;
that command prepares its own disposable database. To apply a changed migration set through the shipped runner, run
`just rust::test reverie_migrator_can_apply_full_migration_set`.

`backend/schema.sql` is the committed `pg_dump` of a database with every migration applied. A change to the migrations
regenerates it with `just rust::schema-dump`, which migrates a scratch database on its own disposable cluster;
`just rust::schema-check` and CI fail when the committed file differs. A Postgres image bump that carries a new minor
release can change `pg_dump`'s output, and regenerates the dump the same way.

The [migration Design](../docs/specs/operations/design/0023-database-migration-model.md) describes the runner, startup
verification and each `MigrationError` mode with its recovery.

### Upgrade Note

The [pre-v0.1 SQL lint transition](../docs/deployment/database-migrations.md#pre-v01-sql-lint-transition) changes
applied migration checksums and requires database recreation. The confirmed-volume procedure below applies to disposable
local development data.

The Postgres 18 upgrade changed the volume mount from `pgdata:/var/lib/postgresql/data` to `pgdata:/var/lib/postgresql`.
Confirm the development cluster's project, container and mounted volume, and coordinate all consumers before deleting
the existing development data:

```bash
just db-reset <confirmed-volume>
just db-migrate
```

`just db-reset <confirmed-volume>` verifies the exact project volume and the container's mounted volume before removing
that volume and recreating the cluster. It reuses retained valid credentials. Deleted data cannot be recovered by a code
revert.
