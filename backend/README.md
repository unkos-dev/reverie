# Backend

This directory contains the Rust Axum backend.

## Development Database

Database tests run in CI. The CI pipeline provisions a Postgres 18 container, seeds roles using `docker/init-roles.sql`, and provides a connection string. Run database tests locally only when your shell can reach a provisioning-capable Postgres cluster. If you see `failed to connect to setup test database: PoolTimedOut`, your test runner cannot reach the cluster. Fix the network route or push your code and let CI run the tests.

Run `docker compose -f docker/compose.dev.yml up -d` from the repository root to start a local development cluster on port 5433. This cluster requires a local Docker daemon. The published port binds on the daemon host. If you use a remote Docker daemon, `localhost:5433` will fail.

### Roles

The `docker/init-roles.sql` script creates these roles when the cluster starts:

| Role                | Connection                                                                  | Purpose                                                   |
| ------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------- |
| `reverie`           | `postgres://reverie:reverie@localhost:5433/reverie_dev`                     | Bootstraps the cluster. Do not use for application logic. |
| `reverie_migrator`  | `postgres://reverie_migrator:reverie_migrator@localhost:5433/reverie_dev`   | Runs migrations. Owns schema objects.                     |
| `reverie_app`       | `postgres://reverie_app:reverie_app@localhost:5433/reverie_dev`             | Serves web traffic. Obeys RLS policies.                   |
| `reverie_ingestion` | `postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev` | Runs background pipelines. Obeys RLS policies.            |
| `reverie_readonly`  | `postgres://reverie_readonly:reverie_readonly@localhost:5433/reverie_dev`   | Queries data for debugging. SELECT only.                  |

The `tower_sessions` schema bypasses RLS. The session id resolves user identity. Role grants control access. The `reverie_app` role receives DML access, `reverie_readonly` receives SELECT, and `reverie_ingestion` receives no access.

### Migrations

The `reverie_migrator` role executes migrations out of band. Set `DATABASE_URL_MIGRATION=postgres://reverie_migrator:reverie_migrator@localhost:5433/reverie_dev` and run `cargo run -- migrate`. The application process calls `db::verify_schema_current()` on startup and exits if the schema diverges. The `#[sqlx::test]` macro uses the built-in sqlx migrator for tests.

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
docker compose -f docker/compose.dev.yml down
docker volume rm reverie_pgdata
docker compose -f docker/compose.dev.yml up -d
cargo run -- migrate
```

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
│   ├── auth/            # Authentication subsystem
│   ├── routes/          # Axum route handlers
│   ├── models/          # Database models and queries
│   ├── services/        # Business logic
│   ├── config/          # Declarative config module
│   ├── state.rs         # AppState
│   └── error.rs         # AppError type
└── tests/               # Integration tests
```
