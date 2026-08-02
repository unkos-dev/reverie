# Backend

This directory contains the Rust Axum backend.

## Development Database

The development database is a local Docker Postgres cluster defined in `docker/compose.dev.yml`. Start it with `just db-up` (or `docker compose -f docker/compose.dev.yml up -d --wait` from the repository root). The cluster serves two transports: loopback-only TCP on `127.0.0.1:5432`, keeping the trivially-credentialed dev cluster off the LAN, and a unix socket bind-mounted to `${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock` on the host. Tooling defaults to the socket; the server and GUI clients use TCP (see "Transports" below). Roles seed from `docker/init-roles.sql` on first init. The cluster is a fresh install: roles and schema build from zero, with no data imports from any prior environment.

The local loop: `just db-up`, then `just db-migrate` to apply migrations, then `just rust::test` / `just rust::doctests` / `just rust::sqlx-check`. Those recipes inject the schema-owner DSN over the socket (`postgres:///reverie_dev?host=$HOME/.local/state/reverie/pgsock&user=reverie&password=reverie`); bare `cargo` invocations must set `DATABASE_URL` themselves, to either that socket form or the TCP form from the roles table below.

### Transports

Local tooling (the DB-backed just recipes: tests, doctests, the sqlx cache, migrations) connects over the unix socket. That is what lets those recipes run inside network-isolated dev sandboxes, which block TCP loopback but not AF_UNIX connects. The runtime server keeps connecting over TCP as the `reverie_app` role, matching the transport and password auth mode it ships with, and GUI clients keep using `localhost:5432`; both transports reach the same cluster.

Socket DSNs use the params-only URI form: `postgres:///reverie_dev?host=<socket-dir>&user=<role>&password=<password>`. sqlx rejects the libpq-style `postgres://user@/db?host=...` spelling (userinfo with an empty authority host fails its URL parsing), and a socket DSN never falls back to TCP: if the socket is absent the connection fails immediately. A container created before the socket mount existed has no host socket; one `just db-up` recreates it. Socket connections match the image's `local all all trust` pg_hba rule and are passwordless for every role, the same effective access the role-name passwords on TCP already grant. Docker Desktop on macOS/Windows cannot share unix sockets across its VM boundary; there, drop the mount with a local compose override and set `REVERIE_DEV_DB_URL` to the TCP schema-owner DSN.

To run the server itself: `just rust::dev` in the foreground, or `just rust::dev-start` / `dev-stop` / `dev-status` for a background process logging to `backend/.dev-server.log`. `just dev-up` from the repository root does the whole sequence above and brings Vite up as well. Unlike the test recipes, these run as the RLS-enforced `reverie_app` role, the identity the deployed server uses. They fill in `DATABASE_URL` and the OPDS-required `REVERIE_PUBLIC_URL` only when neither the environment nor `.env` supplies one, so a `.env` copied from `.env.example` stays authoritative.

The `#[sqlx::test]` macro creates a fresh database per test, which requires a superuser connection; the compose bootstrap role `reverie` qualifies. Running tests with the `reverie_app` DSN from `.env` fails per-test with a permission error. A `failed to connect to setup test database: PoolTimedOut` error means the dev cluster is not running; start it with `just db-up`. CI runs the same commands against its own Postgres service container.

### Roles

The `docker/init-roles.sql` script creates these roles when the cluster starts:

| Role                | Connection                                                                  | Purpose                                                   |
| ------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------- |
| `reverie`           | `postgres://reverie:reverie@localhost:5432/reverie_dev`                     | Bootstraps the cluster. Do not use for application logic. |
| `reverie_migrator`  | `postgres://reverie_migrator:reverie_migrator@localhost:5432/reverie_dev`   | Runs migrations. Owns schema objects.                     |
| `reverie_app`       | `postgres://reverie_app:reverie_app@localhost:5432/reverie_dev`             | Serves web traffic. Obeys RLS policies.                   |
| `reverie_ingestion` | `postgres://reverie_ingestion:reverie_ingestion@localhost:5432/reverie_dev` | Runs background pipelines. Obeys RLS policies.            |
| `reverie_readonly`  | `postgres://reverie_readonly:reverie_readonly@localhost:5432/reverie_dev`   | Queries data for debugging. SELECT only.                  |

The `tower_sessions` schema bypasses RLS. The session id resolves user identity. Role grants control access. The `reverie_app` role receives DML access, `reverie_readonly` receives SELECT, and `reverie_ingestion` receives no access.

### Migrations

The `reverie_migrator` role executes migrations out of band: `just db-migrate` runs them over the socket, or set `DATABASE_URL_MIGRATION` to either transport's migrator DSN and run `cargo run -- migrate`. The application process calls `db::verify_schema_current()` on startup and exits if the schema diverges. The `#[sqlx::test]` macro uses the built-in sqlx migrator for tests.

`just db-migrate` compiles the backend binary first, which is circular when a branch is authoring a new migration: the binary needs the sqlx offline cache to reflect the migration, and the cache needs the migration already applied. `just db-migrate-raw` breaks that cycle by applying `backend/migrations/` with sqlx-cli directly, no compile step; follow it with `just rust::sqlx-prepare`. It is a local authoring shortcut only, not a substitute for `just db-migrate` in any shipped environment. The two runners also group transactions differently: the shipped runner applies all pending transactional migrations in one batch transaction, while sqlx-cli commits each migration individually, so a migration that depends on an earlier migration's commit passes under sqlx-cli and fails under the shipped runner. Before pushing a branch that adds a migration, run `just db-reset && just db-migrate` once so the shipped runner has applied it to a fresh database; no other local loop or preflight lane exercises it.

Operator-facing `MigrationError` modes:

| Variant              | Meaning                             | Recovery                                      |
| -------------------- | ----------------------------------- | --------------------------------------------- |
| `Connection`         | Network failure                     | Fix `DATABASE_URL_MIGRATION`                  |
| `SessionSetup`       | Init failed                         | Check database permissions                    |
| `BatchFailed`        | SQL error                           | DB untouched. Pin previous image              |
| `NoTxFailed`         | Non-transactional SQL failed        | TX migrations committed. Fix SQL and redeploy |
| `NoTxTrackingFailed` | Tracking row insert failed          | Insert tracking row manually                  |
| `SchemaAhead`        | DB ahead of binary                  | Upgrade binary or rollback DB                 |
| `SchemaBehind`       | Binary ahead of DB                  | Run `reverie migrate`                         |
| `NotInitialized`     | DB missing `_sqlx_migrations`       | Run `reverie migrate`                         |
| `VerificationRead`   | App pool cannot read tracking table | Grant `reverie_app` SELECT                    |
| `ChecksumMismatch`   | File modified                       | Restore original file                         |
| `LockTimeout`        | Advisory lock timeout               | Kill concurrent migration processes           |

### Upgrade Note

The Postgres 18 upgrade changed the volume mount from `pgdata:/var/lib/postgresql/data` to `pgdata:/var/lib/postgresql`. You must drop existing development volumes:

```bash
just db-reset
just db-migrate
```

`just db-reset` runs `docker compose -f docker/compose.dev.yml down -v`, which removes the project volume by reference regardless of its generated name, then recreates the cluster.

## Security Headers

The backend provides response headers. Every response receives XCTO, Referrer-Policy, Permissions-Policy, and X-Frame-Options. HTML routes receive a hash-based Content-Security-Policy. API routes receive `default-src 'none'`.

The `backend/src/security/` module implements these headers. The `build_router_with_session_store` function attaches the policies. The `vite-plugins/csp-hash.ts` script hashes the inline `fouc.js` script at build time. Do not add inline `<script>` tags without updating the hash. Do not emit duplicate CSP headers from a reverse proxy.

## Architecture Invariants

- **Stateless application.** Postgres stores all durable state. You can terminate the process at any time.
- **Atomic transactions.** Group multi-statement state changes inside transactions. Do not rely on statement ordering.
- **No N+1 queries.** Write set-based queries. The synthetic performance fixture verifies query counts in CI.
- **Keyset pagination.** Build bounded lists using cursors. Do not use offset pagination.
- **Timeouts.** Configure a timeout for every request, connection pool acquire, database statement, and outbound HTTP call.

## Project Structure

```text
backend/
├── Cargo.toml           # [lib] reverie_api + [[bin]] reverie-api
├── migrations/          # sqlx migrations
├── src/
│   ├── lib.rs           # Library crate root
│   ├── main.rs          # Thin binary entry
│   ├── auth/            # Authentication subsystem (OIDC + local password, recovery, rate limiting)
│   ├── routes/          # Axum route handlers
│   ├── models/          # Database models and queries
│   ├── services/        # Business logic
│   ├── security/        # Response security headers + CSRF validating middleware
│   ├── config/          # Declarative config module
│   ├── state.rs         # AppState
│   └── error.rs         # AppError type
└── tests/               # Integration tests
```
