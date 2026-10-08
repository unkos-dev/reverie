# Backend

This directory contains the Rust Axum backend.

## Development database

`just db-up` starts the persistent development cluster in `docker/compose.dev.yml`. The shared provisioning script
generates five independent credentials once and retains them at
`${XDG_STATE_HOME:-$HOME/.local/state}/reverie/postgres/<compose-project>/credentials.env`. The directory is mode 0700
and the file is mode 0600. Provisioning is serialised by a stable lock; restarts reuse the complete state. A volume
without its original credential state refuses startup. Restore that state or coordinate an explicit reset.

Run `just db-migrate` before starting the server, or use `just dev-up` for migrations, the API and Vite. The dev loader
resolves the process environment, then the parsed `REVERIE_DEV_ENV` file (default `~/reverie/dev/env`), then retained
state for absent database inputs. Explicit empty assignments stay empty and fail application validation. Omit the three
database assignments from a local env file to use retained state. Fully supplied external DSNs need no local state; a
conflicting bootstrap `POSTGRES_PASSWORD` is rejected. Operators supply their own role-specific DSNs through deployment
tooling; the server reads only its process environment.

`just db-down` preserves the volume and credentials. `just db-reset <confirmed-volume>` permanently deletes that
volume's data and recreates it with retained valid credentials. Confirm the resolved project, container and mounted
volume, and coordinate all consumers before resetting. A code revert cannot recover deleted data. Neither dev readers
nor verification commands rotate development credentials.

### Verification ownership and transports

Each `just rust::test`, `doctests`, `sqlx-check`, `sqlx-prepare`, `schema-check` and `schema-dump` invocation owns a
disposable cluster with independent credentials, a dynamic loopback TCP port and a unique socket directory. Tests and
doctests compile against the committed SQLx cache. Test mode leaves the bootstrap database without migrations; SQLx
creates and migrates each test database. Schema mode prepares its bootstrap database for online compilation and SQLx
cache checks. Schema dumping uses a migrator-owned scratch database within its disposable cluster. CI uses the same
owner; its online Clippy and documentation commands receive prepared clusters.

Both TCP and Unix sockets require SCRAM authentication. Verification uses its owned socket when available and its owned
TCP endpoint across a Docker host boundary. Development publishes `127.0.0.1:5432` and the socket at
`${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock`; the runtime server uses TCP. Socket DSNs use the params-only
form `postgres:///database?host=<socket-dir>&user=<role>&password=<password>`. SQLx rejects
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

The `reverie_migrator` role executes migrations out of band: `just db-migrate` uses the retained migrator credentials,
or set `DATABASE_URL_MIGRATION` to either transport's migrator DSN and run `cargo run -- migrate`. The application
process calls `db::verify_schema_current()` on startup and exits if the schema diverges. The `#[sqlx::test]` macro uses
the built-in sqlx migrator for tests.

`just db-migrate` compiles the backend binary first, which is circular when a branch is authoring a new migration: the
binary needs the sqlx offline cache to reflect the migration, and the cache needs the migration already applied.
`just db-migrate-raw` breaks that cycle by applying `backend/migrations/` with sqlx-cli directly, no compile step;
follow it with `just rust::sqlx-prepare`. It is a local authoring shortcut only, not a substitute for `just db-migrate`
in any shipped environment. The two runners also group transactions differently: the shipped runner applies all pending
transactional migrations in one batch transaction, while sqlx-cli commits each migration individually, so a migration
that depends on an earlier migration's commit passes under sqlx-cli and fails under the shipped runner. Before pushing a
branch that adds a migration, coordinate `just db-reset <confirmed-volume>` then run `just db-migrate` so the shipped
runner has applied it to a fresh database; no other local loop or preflight lane exercises it.

`backend/schema.sql` is the committed `pg_dump` of a database with every migration applied. A change to the migrations
regenerates it with `just rust::schema-dump`, which migrates a scratch database on its own disposable cluster;
`just rust::schema-check` and CI fail when the committed file differs. A Postgres image bump that carries a new minor
release can change `pg_dump`'s output, and regenerates the dump the same way.

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
Confirm the development cluster's project, container and mounted volume, and coordinate all consumers before deleting
the existing development data:

```bash
just db-reset <confirmed-volume>
just db-migrate
```

`just db-reset <confirmed-volume>` verifies the exact project volume and the container's mounted volume before removing
that volume and recreating the cluster. It reuses retained valid credentials. Deleted data cannot be recovered by a code
revert.

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

Startup and admin scans discover the full tree. Watcher events recheck affected names and directory trees, and
finalisation refreshes only its source. Unchanged observations do not rewrite database rows or restart readiness.

EPUB is the only accepted format and the default. An empty accepted-format set accepts nothing. Hidden entries and
`Thumbs.db` are ignored; other files, including sidecars, remain independent inputs. Rejection preserves the original,
records its reason and creates neither a manifestation nor a quarantine copy. Unchanged rejected inputs remain
suppressed across restart. A changed fingerprint creates a new generation. Content duplicates link the existing work; a
destination-path collision selects a suffix and does not establish duplication.

The three ingestion environment values seed settings once before startup completes. Saved database values then control
acceptance and cleanup through the existing settings API and live reload, including empty acceptance and default values.
Changing those environment values on restart does not overwrite saved settings. Imported-source cleanup defaults to
enabled; duplicate-source cleanup defaults to disabled. Cleanup requires an unchanged source and uses the current
settings snapshot. Rejected and unaccepted files are retained. Upward pruning starts only from a successful deletion,
stops at the ingestion root and preserves unrelated empty directories. A directory can be pruned only if every remaining
entry is a regular `.DS_Store` or `Thumbs.db` file. Sidecars, other hidden files, directories and symlinks prevent
pruning. The old format-priority, cleanup-mode and quarantine-root settings are removed.

A local cleanup error preserves the completed import or duplicate and its work link; other inputs continue. Successful
deletion whose state update fails retains a live receipt for recommit. Absence without that receipt records
`unattributed_disappearance`, including after restart, because absence alone cannot identify the actor.

Attempts use a child of the worker's shutdown cancellation token and a progress counter. Source hashing and streaming
check cancellation between 64 KiB chunks and advance progress after each chunk. There is no total-duration deadline.
After 120 seconds without streaming progress, the coordinator requests cancellation. Validation and publication are
protected from idle cancellation; phase transitions reset idle observation and preserve earlier shutdown requests.
Cancellation discards the owned candidate and preserves the source. Attempt ownership lasts until the blocking closure
returns. After five minutes without progress, a stall warning repeats every five minutes until that return. A read
blocked inside the kernel cannot observe cancellation. Shutdown cancellation records no terminal outcome; startup
reclaims interrupted attempts. A progressing large copy can exceed two minutes, and a blocked read can outlast the
shared 30-second shutdown drain budget.

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

Before final publication, the linked attempt durably records its exact library/name, candidate identity, accepted hash
and size. The imported transaction clears this evidence with the manifestation claim and outcome. Startup reconciles
unresolved evidence before reclaiming interrupted attempts. It preserves committed owners, including files relocated by
writeback, and foreign content; only a verified unregistered owned name can be removed. Unavailable ownership, read,
removal or required sync retains evidence and suspends that input while healthy unrelated inputs continue. Correct the
obstruction and request a scan. Shared failures retain the global probe schedule.

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
SQL location updates remain separate. Accepted hash/size and exact source/destination intent persist before movement;
forward recovery verifies that evidence before adopting a destination, resuming movement or preserving a foreign file.
SQL failure retains intent without moving bytes back. Startup and periodic recovery carriers use the existing claimed
writeback owner. These guarantees do not establish a NAS server's acknowledgement or flush behaviour; representative
mounted-share verification is still needed.

Ingestion records the owning library and actual relative destination with accepted content evidence and input outcomes.
Requests and asynchronous cover warming select that same authority; cache responses stream an opened handle. Rebuildable
covers use complete atomic replacement without forced sync.

Development catalogues are disposable. Resolve the owned development database before using `just db-reset`, then run
`just db-migrate` and re-ingest source copies. No automatic reset, preserving upgrade or legacy-path fallback is
provided.

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
