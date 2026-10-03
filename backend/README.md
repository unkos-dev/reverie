# Backend

This directory contains the Rust Axum backend.

## Development Database

The development database is a local Docker Postgres cluster defined in `docker/compose.dev.yml`. Start it with
`just db-up` (or `docker compose -f docker/compose.dev.yml up -d --wait` from the repository root). The cluster serves
two transports: loopback-only TCP on `127.0.0.1:5432`, keeping the trivially-credentialed dev cluster off the LAN, and a
Unix socket bind-mounted to `${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock` on the host. Tooling defaults to the
socket; the server and GUI clients use TCP (see "Transports" below). Roles seed from `docker/init-roles.sql` on first
init. The cluster is a fresh install: roles and schema build from zero, with no data imports from any prior environment.

The local loop: `just db-up`, then `just db-migrate` to apply migrations, then `just rust::test` / `just rust::doctests`
/ `just rust::sqlx-check`. Those recipes inject the schema-owner DSN over the socket
(`postgres:///reverie_dev?host=$HOME/.local/state/reverie/pgsock&user=reverie&password=reverie`); bare `cargo`
invocations must set `DATABASE_URL` themselves, to either that socket form or the TCP form from the roles table below.

### Transports

Local tooling (the DB-backed just recipes: tests, doctests, the sqlx cache, migrations) connects over the Unix socket.
That is what lets those recipes run inside network-isolated dev sandboxes, which block TCP loopback but not AF_UNIX
connects. The runtime server keeps connecting over TCP as the `reverie_app` role, matching the transport and password
auth mode it ships with, and GUI clients keep using `localhost:5432`; both transports reach the same cluster.

Socket DSNs use the params-only URI form: `postgres:///reverie_dev?host=<socket-dir>&user=<role>&password=<password>`.
sqlx rejects the libpq-style `postgres://user@/db?host=...` spelling (userinfo with an empty authority host fails its
URL parsing), and a socket DSN never falls back to TCP: if the socket is absent the connection fails immediately. A
container created before the socket mount existed has no host socket; one `just db-up` recreates it. Socket connections
match the image's `local all all trust` pg_hba rule and are passwordless for every role, the same effective access the
role-name passwords on TCP already grant. Docker Desktop on macOS/Windows cannot share Unix sockets across its VM
boundary; there, drop the mount with a local compose override and set `REVERIE_DEV_DB_URL` to the TCP schema-owner DSN.
The schema recipes (`just rust::schema-dump` and `just rust::schema-check`) take `REVERIE_PG_HOST=localhost` instead,
plus `REVERIE_MIGRATOR_PASSWORD` when the cluster's migrator password is not the dev default.

To run the server itself: `just rust::dev` in the foreground, or `just rust::dev-start` / `dev-stop` / `dev-status` for
a background process logging to `backend/.dev-server.log`. `just dev-up` from the repository root does the whole
sequence above and brings Vite up as well. Unlike the test recipes, these run as the RLS-enforced `reverie_app` role,
the identity the deployed server uses. They fill in `DATABASE_URL` and the OPDS-required `REVERIE_PUBLIC_URL` only when
neither the environment nor `.env` supplies one, so a `.env` copied from `.env.example` stays authoritative.

The `#[sqlx::test]` macro creates a fresh database per test, which requires a superuser connection; the compose
bootstrap role `reverie` qualifies. Running tests with the `reverie_app` DSN from `.env` fails per-test with a
permission error. A `failed to connect to setup test database: PoolTimedOut` error means the dev cluster is not running;
start it with `just db-up`. CI runs the same commands against its own Postgres service container.

### Roles

The `docker/init-roles.sql` script creates these roles when the cluster starts:

| Role | Connection | Purpose |
| ---- | ---------- | ------- |
| `reverie` | `postgres://reverie:reverie@localhost:5432/reverie_dev` | Bootstraps the cluster. Do not use for application logic. |
| `reverie_migrator` | `postgres://reverie_migrator:reverie_migrator@localhost:5432/reverie_dev` | Runs migrations. Owns schema objects. |
| `reverie_app` | `postgres://reverie_app:reverie_app@localhost:5432/reverie_dev` | Serves web traffic. Obeys RLS policies. |
| `reverie_ingestion` | `postgres://reverie_ingestion:reverie_ingestion@localhost:5432/reverie_dev` | Runs background pipelines. Obeys RLS policies. |
| `reverie_readonly` | `postgres://reverie_readonly:reverie_readonly@localhost:5432/reverie_dev` | Queries data for debugging. SELECT only. |

The `tower_sessions` schema bypasses RLS. The session id resolves user identity. Role grants control access. The
`reverie_app` role receives DML access, `reverie_readonly` can read only the `expiry_date` column, and
`reverie_ingestion` receives no access.

### Migrations

The `reverie_migrator` role executes migrations out of band: `just db-migrate` runs them over the socket, or set
`DATABASE_URL_MIGRATION` to either transport's migrator DSN and run `cargo run -- migrate`. The application process
calls `db::verify_schema_current()` on startup and exits if the schema diverges. The `#[sqlx::test]` macro uses the
built-in sqlx migrator for tests.

`just db-migrate` compiles the backend binary first, which is circular when a branch is authoring a new migration: the
binary needs the sqlx offline cache to reflect the migration, and the cache needs the migration already applied.
`just db-migrate-raw` breaks that cycle by applying `backend/migrations/` with sqlx-cli directly, no compile step;
follow it with `just rust::sqlx-prepare`. It is a local authoring shortcut only, not a substitute for `just db-migrate`
in any shipped environment. The two runners also group transactions differently: the shipped runner applies all pending
transactional migrations in one batch transaction, while sqlx-cli commits each migration individually, so a migration
that depends on an earlier migration's commit passes under sqlx-cli and fails under the shipped runner. Before pushing a
branch that adds a migration, run `just db-reset && just db-migrate` once so the shipped runner has applied it to a
fresh database; no other local loop or preflight lane exercises it.

`backend/schema.sql` is the committed `pg_dump` of a database with every migration applied. A change to the migrations
regenerates it with `just rust::schema-dump`, which migrates a scratch database on the dev cluster instead of reading
`reverie_dev`; `just rust::schema-check` and CI fail when the committed file differs. A Postgres image bump that carries
a new minor release can change `pg_dump`'s output, and regenerates the dump the same way.

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

The Postgres 18 upgrade changed the volume mount from `pgdata:/var/lib/postgresql/data` to `pgdata:/var/lib/postgresql`.
You must drop existing development volumes:

```bash
just db-reset
just db-migrate
```

`just db-reset` runs `docker compose -f docker/compose.dev.yml down -v`, which removes the project volume by reference
regardless of its generated name, then recreates the cluster.

## Security Headers

The backend provides response headers. Every response receives XCTO, Referrer-Policy, Permissions-Policy, and
X-Frame-Options. HTML routes receive a hash-based Content-Security-Policy. API routes receive `default-src 'none'`.

The `backend/src/security/` module implements these headers. The `build_router_with_session_store` function attaches the
policies. The `vite-plugins/csp-hash.ts` script hashes the inline `fouc.js` script at build time. Do not add inline
`<script>` tags without updating the hash. Do not emit duplicate CSP headers from a reverse proxy.

## Architecture Invariants

- **Stateless application.** Postgres stores all durable state. You can terminate the process at any time.
- **Atomic transactions.** Group multi-statement state changes inside transactions. Do not rely on statement ordering.
- **No N+1 queries.** Write set-based queries. The synthetic performance fixture verifies query counts in CI.
- **Keyset pagination.** Build bounded lists using cursors. Do not use offset pagination.
- **Timeouts.** Configure a timeout for every request, connection pool acquire, database statement, and outbound HTTP
  call.

## Ingestion readiness and retries

One coordinator discovers inputs, waits for readiness and runs one attempt at a time. Startup, watcher events and admin
scans share that owner. An input needs ten seconds of observed unchanged size and modification time. A scan returns HTTP
202 after discovery with queued, deferred and suppressed counts and `/api/v1/dashboard/activity` as its monitor. These
counts describe discovery; they do not identify a separate import batch or promise completed imports.

EPUB is the only accepted format and the default. An empty accepted-format set accepts nothing. Hidden entries and
`Thumbs.db` are ignored; other files, including sidecars, remain independent inputs. Rejection preserves the original,
records its reason and creates neither a manifestation nor a quarantine copy. Unchanged rejected inputs remain
suppressed across restart. A changed fingerprint creates a new generation. Content duplicates link the existing work; a
destination-path collision selects a suffix and does not establish duplication.

Persisted settings control acceptance and cleanup through the existing settings API and live reload. Imported-source
cleanup defaults to enabled; duplicate-source cleanup defaults to disabled. Cleanup requires an unchanged source and
uses the current settings snapshot. Rejected and unaccepted files are retained. Upward pruning starts only from a
successful deletion, stops at the ingestion root and preserves unrelated empty directories. A directory can be pruned
only if every remaining entry is a regular `.DS_Store` or `Thumbs.db` file. Sidecars, other hidden files, directories
and symlinks prevent pruning. The old format-priority, cleanup-mode and quarantine-root settings are removed.

Attempts use a child of the worker's shutdown cancellation token and a progress counter. Source hashing and streaming
check cancellation between 64 KiB chunks and advance progress after each chunk. There is no total-duration deadline.
After 120 seconds without progress, the coordinator requests cancellation. Validation and no-overwrite publication
finish their current operation before cancellation is handled. Cancellation discards the owned candidate and preserves
the source. Attempt ownership lasts until the blocking closure returns. After five minutes without progress, a stall
warning repeats every five minutes until that return. A read blocked inside the kernel cannot observe cancellation.
Shutdown cancellation records no terminal outcome; startup reclaims interrupted attempts. A progressing large copy can
exceed two minutes, and a blocked read can outlast the shared 30-second shutdown drain budget.

Operational failures have three classes:

- **Shared dependency:** An unavailable database or a failed probe of the opened ingestion or library root pauses new
  attempts. Root failures include EIO, ENOTCONN, ESTALE, EHOSTDOWN and ENOSPC on the destination root. Probes run after
  30 seconds, one minute, two minutes and then every five minutes until successful. Observation and readiness continue
  where ingestion authority permits. Completed results whose outcome commit failed are retained and recommitted on this
  schedule. Pause and resume are each logged once. These failures consume no input retry budget.
- **Transient input:** With both roots probing healthy, input-specific EIO, an idle stall, a panic, an unchanged-source
  hash mismatch and other unlisted I/O errors are transient. Hash mismatch first rechecks the source fingerprint; a
  changed source discards the candidate and schedules another check without recording a failure. Unlisted I/O errors,
  including WriteZero and UnexpectedEof, record their error kind in the attempt reason. Five automatic retries follow
  the initial attempt, after five minutes, 30 minutes, two hours, eight hours and 24 hours, without `jitter`. The sixth
  failed transient attempt exhausts the generation and leaves operational failure without a deadline. Counts come from
  linked attempt history since the last persisted retry reset, excluding shared-dependency and interrupted outcomes.
- **Needs change:** Source EACCES or EPERM, ENAMETOOLONG, a non-regular file, ELOOP and an unrepresentable path do not
  retry automatically.

A new generation, startup reconstruction or an admin scan resets exhausted and needs-change inputs to eligibility,
subject to readiness, and persists the retry-reset marker. Rejection suppression is unchanged. Retry timings and budgets
are internal constants; no settings configure them. An outage can leave inputs waiting until authority becomes healthy
again. Operators currently manage retained originals on disk and request another scan after a correction; dedicated
input-management, retry UI and retention controls remain deferred.

## Managed library files

Reverie owns writes and reorganisation inside `REVERIE_LIBRARY_PATH`. Coordinate external tools with the application, or
pause it before they change managed files. Relocating the root, replacing its directory or changing its mount requires a
coordinated restart: an opened capability identifies the original directory object and does not follow a replacement
path into another library. Downloads classify paths through the root's canonical path before opening through the
capability, so replacing that directory can make downloads fail until restart.

`REVERIE_LIBRARY_PATH` and `REVERIE_INGESTION_PATH` must be absolute paths to provisioned directories. Mount the
intended volumes before starting Reverie. The server opens both roots before admin bootstrap, workers or requests;
empty, relative, missing and non-directory roots fail startup. It does not create these directories or retry acquisition
during requests. The `reverie migrate` command does not require storage roots. Stop Reverie before removing a mount,
since its open directory handles can keep a mount busy. Directory existence alone does not establish that the intended
volume is mounted.

Each manifestation records a library identity and a canonical path relative to that library. Startup binds the seeded
`default` identity to `REVERIE_LIBRARY_PATH`; downloads select that immutable binding, and unknown identities never fall
back. Changing metadata or a future organisation policy does not change the recorded file location. Relative and
absolute symlink targets resolving inside the owning library remain supported. Path resolution classifies the target;
the actual open uses a relative target through the pinned directory. The opened file supplies both Content-Length and
streamed bytes. Established escapes return 403, missing files return 404, and other failures, including unknown library
identities and ambiguous I/O denial, return generic 500 responses.

A library can reside on a NAS separate from the Linux container host when its mounted filesystem supplies contained file
access, atomic content replacement and useful sync/error semantics. Writeback relocation needs no-replace rename or hard
links. Only EINVAL or ENOSYS from no-replace rename enables the hard-link fallback; collision, permission and ambiguous
network errors remain failures. Unsupported operations preserve the source. This contract does not promise that every
NFS or SMB server supplies the required behaviour.

Hard-link relocation syncs the destination parent before removing the source, then syncs the source parent. A failed
link or destination sync retains the source; removal failure may leave both names. EXDEV copies stage inside an owned
directory on the actual destination filesystem, refuse occupied final names and independently verify the destination
before removing the original. Normal completion removes staging, but abrupt exit may leave bare UUID directory names
visible on the share. No cleanup sweep runs.

A successful sync after a reported failure does not prove that failed writes became durable. Filesystem relocation and
SQL location updates remain separate, with move-back on SQL failure. A crash between them can leave the recorded
location stale without automatic reconciliation. These guarantees do not establish a NAS server's acknowledgement or
flush behaviour; representative mounted-share verification is still needed.

This is a draft checkpoint. Initial ingestion does not yet supply the required library identity. The remaining
[pipeline limitations](../debt/2026-09-30-library-location-pipelines-incomplete.md) prevent end-to-end use and release
readiness. Their tests remain active.

Development catalogues are disposable. Resolve the owned development database before using `just db-reset`, then run
`just db-migrate`. Re-ingest from source copies after the producer pipelines support the location contract. No automatic
reset, preserving upgrade or legacy-path fallback is provided.

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
│   └── error/           # AppError and its Problem Details mapping
└── tests/               # Integration tests
```
