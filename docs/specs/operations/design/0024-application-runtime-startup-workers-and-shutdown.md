---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0024"
title: "Application runtime: startup, workers, and shutdown"
satisfies:
  - "REV-REQ-0066"
governed-by:
  - "REV-ADR-0021"
  - "REV-ADR-0053"
  - "REV-ADR-0051"
  - "REV-ADR-0057"
---

# Application runtime: startup, workers, and shutdown

This Design covers the process lifecycle of the `reverie-api` binary: how the command line selects between the
long-lived server and its one-shot administrative subcommands, the ordered sequence `run` executes before it accepts a
request, its supervised service workers and coordinator-owned filesystem watcher, the drain that shuts them down, the
liveness and readiness probes, and the runtime stage of the container image that packages all of it.

## Purpose and boundaries

This subject owns: CLI subcommand dispatch (`Command`, `parse_command`); the ordered, fallible setup sequence inside
`run` (configuration load, CSP header finalisation, tracing, database pools, the schema apply-or-verify step, admin
storage-root acquisition, admin bootstrap, OIDC runtime construction, settings load, login limiter construction,
`AppState` assembly, router build, listener bind); how the five unconditional service workers and optional OIDC
discovery worker are registered and the coordinator owns its filesystem watcher; the supervised drain that awaits them
on shutdown; the `/health` and `/health/ready` probes; and the `runtime` stage of `Dockerfile` (its `ENTRYPOINT`,
`HEALTHCHECK`, `USER` and `EXPOSE`).

It does not own: what each background worker does once running (the ingestion pipeline, the enrichment queue, the
writeback pipeline, the settings live-reload mechanism and the session sweep are each their own subject); the meaning or
validation of individual configuration fields, or the `figment` pipeline that produces a `Config`
(`backend/src/config/`); how a schema migration is applied, verified, or which role runs it (`backend/src/db.rs`); the
content of the CSP headers `run` finalises, or the frontend-dist validation it calls (`backend/src/security/csp.rs`,
`backend/src/security/dist_validation.rs`); session cookie attributes or the session store's own persistence
(`backend/src/auth/session.rs`, `backend/src/auth/store.rs`); OIDC discovery internals (`backend/src/auth/oidc.rs`); or
the identity mechanics behind the `bootstrap`, `reset-password` and `unlock-account` subcommands, which this subject
only dispatches to (`backend/src/models/user.rs`, `backend/src/auth/recovery.rs`,
`backend/src/models/login_throttle.rs`). It does not own the builder or frontend stages of `Dockerfile`, only the
`runtime` stage that consumes their output. It does not own cross-instance coordination, because there is none to own:
`run` performs no leader election, node discovery or peer handshake, and every instance of the process spawns the same
service workers and filesystem watcher, and serves the same routes.

Depends on: `config::Config` for every setting `run` reads; `db::init_pool`, `db::init_writeback_pool`,
`db::run_migrations` and `db::verify_schema_current` for every database connection it opens;
`auth::oidc::OidcRuntime::configured` for synchronous optional OIDC runtime construction;
`security::csp::build_api_csp`, `security::csp::build_html_csp` and `security::dist_validation::validate_frontend_dist`
for the CSP headers it finalises before building the router; `services::settings::seed_ingestion` for the initial
settings snapshot; `build_router` for the Axum router it serves; and the worker entry points named in Structure below.

Depended on by: `main.rs`, whose `#[tokio::main]` `main` is the sole caller of `parse_command` and the command
entrypoints; the container image's `ENTRYPOINT`, which invokes the compiled binary with no arguments (the `Serve` path)
in normal operation, or `migrate` in a one-shot migration step; and any caller embedding the library and calling `run`
directly rather than assembling its own `AppState` and calling `build_router`.

## Structure

- `backend/src/main.rs` is the binary's entry point: it collects `argv[1..]`, calls `parse_command`, and matches the
  resulting `Command` to one of six library-crate functions: five are async and called with `.await` (`run_migrate`,
  `run_bootstrap`, `run_reset_password`, `run_unlock_account`, `run`), and one, `print_config_schema`, is synchronous
  and called directly. It installs no signal handling and no tracing of its own; both belong to the functions it calls.
- `backend/src/lib.rs::Command` and `::parse_command` define the CLI surface: `Serve` (no arguments), `Migrate`
  (`migrate`), `PrintConfigSchema` (`print-config-schema`), `Bootstrap` (`bootstrap`), `ResetPassword { email }`
  (`reset-password <email>`) and `UnlockAccount { email }` (`unlock-account <email>`). `Command` is not
  `#[non_exhaustive]`, so `main.rs`'s match is exhaustive at compile time; `parse_command` itself rejects an unknown
  token or unexpected trailing arguments rather than falling through to `Serve`.
- `backend/src/lib.rs::run` is the server's entry point. It performs the ordered setup sequence described in Runtime
  behaviour, starts the worker supervisor and serves HTTP alongside it, awaiting both before returning.
- `backend/src/lib.rs::build_router` and `::build_router_with_session_store` assemble the Axum router `run` serves: the
  reserved-prefix routes, the OPDS mount when enabled, the SPA fallback when a frontend dist path is configured, and the
  middleware layers (CSP, CSRF, the security-headers wrapper, the `problem_instance_layer`, the session layer, tracing).
  Each layer's own behaviour belongs to its own subject; this Design only fixes the order in which
  `build_router_with_session_store` composes them, which decides what a rejection at one layer carries from the layers
  outside it.
- `backend/src/lib.rs::worker_supervisor` configures task-supervisor's restart window, backoff, dead-task threshold and
  stop timeout. `spawn_workers` registers five unconditional workers and uses `with_oidc_discovery` to add the
  configured OIDC worker. `Worker` adapts successful returns outside cancellation into failures;
  `supervise_until_shutdown` maps a terminal supervisor error to shutdown cancellation and an error returned by `run`.
  The ingestion handle shares the supervisor handle with the status route so lifecycle reads use the supervisor's task
  status.
- `backend/src/lib.rs::apply_or_verify_schema` is the schema-step flag selector: it branches on `config.auto_migrate`
  between an in-process migration run (`db::run_migrations`) and a read-only verification (`db::verify_schema_current`).
  It is extracted from `run` specifically so both branches are pinned by tests.
- `backend/src/lib.rs::shutdown_signal` races `tokio::signal::ctrl_c()` against a registered SIGTERM handler and cancels
  the shared `CancellationToken` when either resolves.
- `backend/src/state.rs::AppState` is the `Clone` handle `run` builds once and threads into the router, the request
  handlers and (via per-task clones of its constituent fields) the background workers. Its fields are documented in Data
  and state below.
- `backend/src/services/files.rs::LibraryFiles` shares immutable library-identity bindings and the opened ingestion
  directory across AppState clones. Startup opens all required capabilities before seeds or workers.
- `backend/src/routes/health.rs` supplies `GET /health` (liveness: always `200 ok`) and `GET /health/ready` (readiness:
  pings the application pool with `SELECT 1`, returning `200 ok` or a `503` Problem Details body). Both are outside
  `/api/v1` and carry an explicit empty `security(())` OpenAPI annotation, opting out of the document-level
  session-cookie default.
- The five unconditional background workers, each entered from its own module and given only what it needs:
  - `services::settings::listen_loop` (settings LISTEN/NOTIFY reload, refreshing `AppState.settings`).
  - `services::ingestion::run_watcher` (one owner for capability discovery, readiness, attempts, retries and cleanup,
    receiving shared `LibraryFiles`, the live settings cache and command receiver; it owns its watcher and event
    channel).
  - `services::enrichment::queue::spawn_queue` (the enrichment job queue).
  - `services::session_sweep::run_sweep` (the hourly authentication-state reaper, given the application pool, three
    source limiter handles and `PostgresStore`). Native governor retention runs before database cleanup. Session
    deletion cascades `sign_in_cohorts`; expired attempts and inactive account throttles drain in batches of 1,000, with
    cancellation between batches. Live protection remains; failures warn and retry next tick.
  - `services::writeback::queue::spawn_worker` (the writeback job queue, given the dedicated system-context pool
    described in Data and state and shared `LibraryFiles`; also owns the bounded relocation-intent sweep).
- The optional `oidc-discovery` worker calls `auth::oidc::OidcRuntime::run_discovery` with the shared runtime and its
  run token. It retries expected provider failures internally, publishes the first discovered snapshot and then waits
  for cancellation. Without OIDC configuration, this worker is absent.
- `services::ingestion::watcher::register` establishes a watch synchronously after the coordinator folder check. The
  coordinator owns and polls its returned future using the coordinator run token.
- `Dockerfile`'s `runtime` stage (the final `FROM debian:trixie-slim ... AS runtime` block) copies the release binary
  and the built frontend, creates a fixed non-root user, sets `REVERIE_FRONTEND_DIST_PATH`, and declares the
  `ENTRYPOINT` and `HEALTHCHECK` this subject's binary and readiness probe satisfy.

### State-writer census

This subject owns the shutdown `CancellationToken` and the storage bindings created in `run`. The bindings have one
writer: startup calls `LibraryFiles::open` before constructing AppState. Library and ingestion capabilities are
immutable for the lifetime of all AppState clones. Requests neither acquire nor replace roots. Other dependencies are
written once at construction.

Startup owns the three source maps and supplies their shared handles to requests and the session-sweep worker. Password
login/setup/registration, recovery and OIDC start each spend only their own map. The sweep reclaims them using native
governor retention before database cleanup. Account-throttle writers and locked-row reclamation belong to
[local password sign-in](../../accounts/design/0012-local-password-sign-in.md); this runtime only supplies the pool and
hourly lifecycle. Synchronous password verification and its account-row wait have no aggregate computation or waiting
bound here.

The process token has three cancellation writers: `shutdown_signal` on SIGINT or SIGTERM, `run` after HTTP serving
returns, and `supervise_until_shutdown` when the supervisor exits. Cancellation is idempotent; these writers share the
single decision to stop the process. task-supervisor alone creates and cancels the distinct token for each worker run.
No worker schedules its own lifecycle restart.

`AppState.ingestion` holds the command sender created at startup. Its receiver is held in a shared `tokio::sync::Mutex`
retained by the supervisor's coordinator factory. Each run holds the receiver guard for its lifetime. Dropping a run
releases the guard without closing the channel; queued commands survive backoff, while a command already removed for
processing may report interrupted discovery if that run fails. Request handlers keep the same sender. The coordinator is
the sole writer of its scheduling and watcher state. Its watcher future and event channel are local to that run, and the
watcher receives the same cancellation token. Dropping the coordinator drops the watcher. Watcher, startup and deadline
signals coalesce inside that owner. Delete and rename notifications bypass create/modify batching; capability rechecks
establish absence. Shutdown cancellation reaches the active attempt's child token. A kernel-blocked read can outlast the
supervisor's stop timeout.

The coordinator alone publishes dependency observations through a `watch` channel retained with the command receiver. It
records the first failure's timestamp, preserves it across repeated failures and reason refinement, and clears the
observation after successful recovery. Run termination leaves the last observation intact during backoff. Replacement
run startup resets it before probing. The status handler composes that observation with the saved `ingestion_enabled`
value in the live settings cache and the supervisor's `Restarting` task status. It never acquires the command receiver
or waits for the coordinator's database or filesystem work. The settings service owns the boolean; the coordinator owns
the observation; task-supervisor owns lifecycle status.

The owner reconciles acknowledged publication evidence before reclaiming interrupted attempts under its advisory lock.
Unresolved local ownership suspends only the affected input. Streaming accepts idle cancellation after 120 seconds;
validation and publication are protected phases that retain earlier shutdown requests. Phase transitions reset idle
observation. Watchdog ticks and completion precede queued watcher traffic.

## Interfaces and dependencies

- The CLI surface `parse_command` accepts is the only interface `main.rs` exposes: no argument (`Serve`), or exactly one
  of `migrate`, `print-config-schema`, `bootstrap`, `reset-password <email>`, `unlock-account <email>`. `migrate` and
  `print-config-schema` load no `Config`: `run_migrate` reads only `DATABASE_URL_MIGRATION`, deliberately bypassing
  `Config::from_env` so a migration-only invocation never holds the OIDC secret or the application DSN, and
  `print_config_schema` only renders the schema. `bootstrap`, `reset-password` and `unlock-account` each load the full
  configuration through `Config::from_env`, so each must pass the same configuration validation the server does.
- `GET /health` and `GET /health/ready` are the two HTTP interfaces this subject owns directly, documented in the
  generated OpenAPI spec via `#[utoipa::path]` annotations that the `routes!` macro compile-checks. Every other route
  reachable through `build_router` belongs to the subject that owns that route.
- `Dockerfile`'s `HEALTHCHECK` is the interface between the container runtime (Docker, Compose, or another orchestrator)
  and `GET /health/ready`: `curl --fail` against `http://127.0.0.1:3000/health/ready`, exec form, on a 30-second
  interval with a 60-second start period and three retries.
- `run`'s own `# Errors` contract is the interface its caller (`main.rs`, or a library embedding it) relies on: every
  fallible step returns an error rather than panicking, so a non-zero process exit is the uniform signal for every
  startup failure class listed in Failure and recovery.

## Data and state

- **`AppState`** (`backend/src/state.rs`) is built exactly once in `run`, after every fallible setup step has succeeded,
  and is `Clone` for cheap distribution to handlers and workers: `pool` and `ingestion_pool` are `Arc`-backed `PgPool`s;
  `config` is owned, cloned data; `library_files` is a cheap shared `LibraryFiles` handle; `oidc` is an
  `Option<Arc<_>>`, `None` on an instance that has not configured OIDC; a configured runtime initially has no snapshot;
  `login_limiter`, `recovery_limiter` and `oidc_limiter` are distinct `Arc<LoginLimiter>` instances;
  `last_settings_reload` is an `Arc<RwLock<..>>` handle written only by the settings worker, not by this subject;
  `settings` is likewise an `Arc<RwLock<..>>` handle, but it has a second writer outside this subject — the
  `PUT /api/v1/settings` route handler also writes it directly, as an immediate local-cache update guarded by
  `apply_if_newer`'s revision check — so neither writer belongs to this subject, but there is more than one.
- **The writeback pool** (`backend/src/db.rs::init_writeback_pool`) is built after `AppState` and is deliberately not
  one of its fields: it is retained by the writeback task factory and cloned for each run. No request handler receives
  it, because no request handler receives anything not reachable through `AppState`.
- **The supervisor handle** is local to `run`. task-supervisor owns run handles, restart history, timers and drain
  deadlines. Fifteen restarts are allowed per task in a thirty-minute sliding window; the sixteenth failure marks that
  task dead. The dead-task threshold is zero, so any dead task stops the supervisor. Backoff starts at one second,
  doubles up to sixty seconds and uses the crate's default ten percent random delay reduction. Instant continuous
  failure exhausts the limit in about nine to ten minutes; thirty-second failed attempts exhaust it in about seventeen
  to eighteen minutes. Stop timeout is thirty seconds per run, concurrently.
- **Per-worker enabled flags.** `config.enrichment.enabled` and `config.writeback.enabled` are read at the start of each
  run. Disabled workers wait for cancellation rather than returning early. These five tasks are registered
  unconditionally; `oidc-discovery` is registered only when OIDC is configured.

## Runtime behaviour

**From process start to the first request accepted**, on the default `Serve` path:

1. `Config::from_env` loads shared configuration, then `Config::validate_server` requires a non-blank
   `DATABASE_URL_INGESTION`. Missing, empty or whitespace-only values return a fixed error naming that variable before
   anything else in this list runs. One-shot administrative commands use the shared loader without this check.
2. `security::csp::build_api_csp` builds the API CSP string and stores it on `config.security.csp_api_header`. If
   `config.security.frontend_dist_path` is set, `security::dist_validation::validate_frontend_dist` runs next, and only
   on success does `security::csp::build_html_csp` build and store the HTML CSP string.
3. The tracing subscriber is installed (`try_init`, not `init`, because `run` is a library entry point a host process
   may have already instrumented); a configured log level that fails to parse falls back to `info` with a warning, once
   the subscriber exists to carry it.
4. `db::init_pool` opens the primary application pool.
5. `apply_or_verify_schema` runs: the default branch (`auto_migrate == false`) calls `db::verify_schema_current` against
   that pool; the opt-in branch re-derives `config.migration_database_url` behind its own defensive `.context(...)?`
   guard — the configuration gate already guarantees the value is present whenever `auto_migrate` is `true`, so this is
   a second, redundant check — before calling `db::run_migrations` against it. Either branch's failure stops startup.
6. The application pool selects the default library identity. A blocking task opens its configured library root and the
   ingestion roots through `LibraryFiles::open`. Missing identity, missing or non-directory roots, and acquisition
   failures stop startup with context. Root configuration is absolute; parsing does not access disk.
7. `seed_admin_if_configured` creates the first administrator from `REVERIE_BOOTSTRAP_*` when configured and no
   administrator yet exists; it is a no-op otherwise. Before hashing or opening the credential transaction, the shared
   password policy checks the configured maximum and minimum length, zxcvbn strength with email and display-name
   context, and the enabled breach check. The breach-check client uses the shared SSRF protections and a ten-second
   timeout; an unavailable breach service logs a warning and permits a sufficiently strong password. Policy rejection
   stops startup without creating the administrator.
8. One bounded HTTPS OIDC transport is built when `config.oidc_configured()` is true; local-only mode builds none. The
   runtime validates the configured issuer and callback URI synchronously and initially has no provider snapshot;
   otherwise `AppState.oidc` stays `None`. Invalid configured URLs, non-HTTPS issuers, client-construction errors and
   configured OIDC without a transport fail startup. No discovery or JWKS request precedes listener binding.
   [OIDC outbound transport](../../accounts/design/0025-oidc-outbound-transport.md) describes the client policy and
   request paths.
9. `db::init_pool` opens the ingestion pool; `services::settings::seed_ingestion` conditionally seeds ingestion fields
   through the primary pool and reads the winning settings row in one transaction; three independent authentication
   source limiters are built from `config.login_rate_per_min`. The persisted `ingestion_enabled` field is loaded with
   that snapshot but is independent of the configuration seed. Its database default is true and it has no environment or
   `Config` field.
10. `AppState` is assembled with the opened `LibraryFiles`, and `build_router` is called on a clone of it.
11. `db::init_writeback_pool` opens the writeback pool, and the TCP listener is bound. One more fallible call follows
    immediately: `listener.local_addr()` is read back to log the bound address. Nothing has been spawned yet at this
    point, so an early return from any of these steps cannot leak a running task.
12. A process `CancellationToken` is created and `spawn_workers` starts one supervisor with five unconditional tasks
    and, when configured, `oidc-discovery`. Worker startup order is unspecified. Each run receives a fresh cancellation
    token and owned clones of its dependencies. The shared ingestion handle receives the supervisor handle before HTTP
    serving starts.
13. HTTP serving and `supervise_until_shutdown` run concurrently. The graceful-shutdown future responds to either an
    operating-system signal or process-token cancellation.

The OIDC worker attempts discovery immediately. Expected discovery, JWKS and discovered-endpoint failures log at error
level; an attempt suppressed by the key-fetch floor logs at warning level. Retry waits are 1, 2, 4, 8, 16, 32, then 60
seconds repeatedly, without random variation. These waits use Tokio time and share the supervisor's one-to-sixty-second
base and cap, while the supervisor retains its own random delay reduction. Provider failures do not consume the
supervisor's restart budget. Local routes and readiness remain independent of discovery.

After the first successful publication, the worker makes no further discovery requests and waits for cancellation.
Signing-key refresh belongs to the OIDC runtime: its 30-second floor counts from each key-fetch attempt's start,
successful or failed. An initial JWKS attempt also observes this floor; metadata retries retain the discovery cadence.
[OIDC outbound transport](../../accounts/design/0025-oidc-outbound-transport.md) owns snapshot writers and fetch
sharing. On cancellation the worker stops admitting refreshes, awaits any bounded discovery operation and drains the
active refresh. Discovery has at most two sequential ten-second HTTP requests; refresh has one. The existing
thirty-second supervisor drain remains in force.

The enabled writeback worker resets orphaned claims before claiming, then sweeps at most 100 eligible relocation
intents. Transient reset, startup-sweep, periodic-sweep and claim errors are logged inside that worker and retried on
its existing timer. It retains tracked jobs and permits through these failures. The indexed sweep repeats every five
minutes. Open intents use five-minute claim spacing; ordinary jobs awaiting recovery preserve their edit budgets until
destination finalisation starts an edit. Relocation carriers reconcile only; metadata and cover jobs reconcile, reload
their snapshots and continue. Disabling writeback skips resets, claims and sweeps. Its tracked-job drain completes
before shutdown recovery unless the supervisor stop timeout expires.

**An authorised OPDS download:** the handler first accepts its database/RLS lookup, then calls
`LibraryFiles::open_download` with the recorded library identity and checked relative path. Unknown identities never
fall back to another library. Path classification, contained opening and handle metadata run in `spawn_blocking`.
Classification supports relative and absolute links resolving inside the owning library; the resulting relative target
is opened through its pinned directory. `tokio::fs::File::from_std` supplies ReaderStream with that same file, and its
metadata supplies Content-Length.

**A graceful shutdown**, triggered by SIGTERM or Ctrl+C while serving:

1. `shutdown_signal` cancels the process token. Axum stops accepting connections and finishes in-flight requests.
2. `supervise_until_shutdown` calls the supervisor's `shutdown`; the crate cancels every run and drains them
   concurrently. Writeback and enrichment stop claiming, await tracked jobs and perform shutdown recovery. Session sweep
   and the settings listener return from their cancellation branches. The coordinator drops its watcher when the run
   ends.
3. A run still pending after thirty seconds is dropped by the crate. Writeback blocking phases retain their semaphore
   permit across async cancellation; unfinished claims remain for startup recovery. This timeout does not guarantee
   completion of a blocking operating-system call.
4. `run` awaits both HTTP and supervisor outcomes. A supervisor error takes precedence; otherwise it returns the HTTP
   result. `main` propagates errors to a non-zero process exit.

**An HTTP serving error** cancels the process token and follows the same supervisor shutdown and drain path before
returning the error. A supervisor terminal error also cancels the process token, causing HTTP graceful shutdown.

**Worker recovery:** task-supervisor catches run errors and panics and schedules restarts. `Worker` also rejects an
`Ok(())` returned before its run token was cancelled. Restart history belongs to each task independently: five different
workers failing once each do not exhaust a shared budget. Once any task exhausts its own limit, the supervisor drains
all remaining runs and returns `TooManyDeadTasks`; the process exits non-zero. The deployment runtime supplies the
container restart policy.

The coordinator's advisory-lock connection closes asynchronously when a run drops. A new run may temporarily fail its
ownership probe; its existing dependency probe retry clears that pause after the old connection closes. Command senders
and queued commands remain valid during the restart. The coordinator starts its watcher after the folder check passes
and before discovery. A watcher failure repeats the same folder check. A failed check pauses as
`ingestion_folder_unavailable` through the existing dependency probe schedule; a successful recheck registers a new
watch before discovery. A watcher failure with healthy folders cancels the active attempt's child token and awaits the
attempt with watchdog ticks before returning the error to supervision. The coordinator retains its advisory lock during
this drain and does not cancel its run token. Process shutdown still uses the supervisor's stop timeout. A restarted
coordinator registers its watch before startup discovery. Both paths discover files that arrived while no watch was
registered. Enrichment drains admitted jobs before returning a claim error, so restart recovery cannot reset their rows
while they are still running.

**CLI dispatch**, for example an operator or a one-shot container step invoking `reverie migrate`: `parse_command`
matches the single token `"migrate"` to `Command::Migrate`; `main.rs`'s exhaustive match calls `run_migrate`, which
resolves `DATABASE_URL_MIGRATION` directly from the environment (treating a blank value as absent, mirroring the
equivalent guard in configuration loading), calls `db::run_migrations`, logs the applied count, and exits. No pool other
than the ephemeral migration connection is ever opened on this path, and `run` (and therefore every background worker)
is never reached.

## Failure and recovery

- **Any startup step failing before the first spawn.** Every fallible step returns an `Err` that `main.rs` surfaces as a
  non-zero process exit: configuration load, an API or HTML CSP string that fails to parse as a valid header value,
  frontend-dist validation, tracing-subscriber installation, any of the three pool opens (application, ingestion,
  writeback), the schema apply-or-verify step, the administrator bootstrap from the environment seed, OIDC configuration
  and client construction, the initial settings load, the login limiter's rate conversion, the TCP listener bind, or
  reading the bound address back from the listener. Because none of these runs after the first `tokio::spawn`, none of
  them can leave a worker running with nothing left to drain it. `axum::serve` also returns its error to `main.rs`, but
  it runs after the workers are spawned; that path is the unclean shutdown in Runtime behaviour, where the workers are
  drained before `run` returns.
- **Storage-root acquisition failing.** Startup stops before seeds, workers or requests. Configuration errors name the
  root variable; acquisition errors identify the required directory. No request retries acquisition.
- **Download access failing.** Established escapes map to 403, missing files to 404, and unknown library identities,
  invalid locations and other I/O or blocking-task failures to generic 500 responses. PermissionDenied from the
  contained open is ambiguous and maps to 500 rather than an inferred escape.
- **CLI dispatch of an unrecognised or malformed subcommand.** A single unknown token, or an unexpected trailing
  argument after a recognised one, is rejected with a message naming the bad token or the expected usage, rather than
  falling through to `Serve`; both directions are pinned by `parse_command_maps_args_rejects_unknown_and_trailing`.
- **`reverie migrate` with no migration DSN.** `resolve_migration_dsn` rejects a missing, empty, or whitespace-only
  `DATABASE_URL_MIGRATION` before `db::run_migrations` is ever called, independently of the equivalent guard inside
  configuration loading (this path deliberately never builds a full `Config`).
- **The readiness probe finding the database unreachable.** `GET /health/ready` returns a `503` Problem Details body and
  logs at `warn`; `GET /health` is unaffected, because liveness and readiness are independent checks — a process can be
  live (serving requests, including this very probe) while its database connection is down. This is the distinction the
  Dockerfile's `HEALTHCHECK` relies on: it probes readiness, not liveness, so the container is only reported healthy
  once the database is actually reachable.
- **Worker error, panic or unexpected success.** The supervisor restarts that task with backoff. The settings listener
  returns connection failures to the supervisor. Session sweep logs database failures and retries at its next scheduled
  sweep; writeback retains its reset, sweep and claim retry timers. Enrichment claim failures drain admitted jobs before
  propagating to supervision. Per-job retry schedules and ingestion dependency probes remain worker-owned.
- **An exhausted worker or failed supervisor.** The supervisor drains remaining runs, then process cancellation stops
  HTTP and `run` returns an error. A prolonged database outage can therefore restart the whole instance.
- **A worker overrunning its stop timeout.** The crate drops its future after thirty seconds. Blocking work may continue
  until its operating-system call returns; startup recovery handles unfinished durable claims.

## Security and operations

Reverie owns managed-library writes and reorganisation. External tools coordinate or pause the application before
changing managed files. An opened root remains tied to its directory object; replacing or relocating the configured root
requires a coordinated restart. Downloads classify paths through the root's canonical path before opening through the
capability, so replacing that directory can make downloads fail until restart. Provision and mount the library and
ingestion roots before starting the server, and stop it before removing a mount: open directory handles can keep a mount
busy. Directory existence cannot prove that the intended volume is mounted. Downloads use the owning capability after
authorisation and never reopen an ambient path for streaming. Ingestion, writeback, cover-source reads, cache
publication and response opening use recorded locations through capabilities.

Configuration retains the six authentication values as `SecretString` or `Option<SecretString>`. Startup explicitly
exposes the application, ingestion and migration DSNs at pool boundaries; OIDC and enrichment constructors explicitly
copy the secret into the existing client API where ownership requires it. Diagnostic formatting of Config redacts these
values. The migrate-only command reads only `DATABASE_URL_MIGRATION`, without constructing Config or loading unrelated
credentials. `backend/tests/ingestion_startup.rs` checks raw captured `stdout` and `stderr` for credential-marker
absence and setting-name presence before any test-diagnostic sanitisation. Its configured-role readiness test also
checks raw output for supplied DSNs and passwords.

When `auto_migrate` is unset or `false`, `run` never reads the migration DSN. Configuration loading forces
`config.migration_database_url` to `None` whenever `auto_migrate` is `false` (covered by that subject, not restated
here), and `apply_or_verify_schema`'s default branch calls `db::verify_schema_current` against the application pool
only, never reading `migration_database_url` at all; the opt-in branch is the only code path in this subject that does.
Both directions are pinned by tests (`apply_or_verify_flag_off_takes_verify_branch`,
`apply_or_verify_flag_on_takes_migrate_branch`), so an inverted branch condition fails the suite rather than shipping
silently. The repository-provided Compose topology keeps the migration credential entirely on the out-of-band
`reverie migrate` path.

The writeback pool's every connection sets the `app.system_context` GUC that the `manifestations_*_system`
row-level-security policies key on (owned by the row-level-security subject, not restated here); this subject's own
contribution to that boundary is structural rather than a runtime check: the writeback pool is never placed on
`AppState`, so no request handler holds a reference to it and none can reach the system-context policies by accident,
whatever the RLS policies themselves would otherwise allow.

The bootstrap seed password (`REVERIE_BOOTSTRAP_PASSWORD`) is read directly from the environment in
`seed_admin_if_configured` rather than retained on the long-lived `Config`, and only the resulting account's email is
logged, never the password.

Container deployments configure a restart policy so a non-zero worker-supervision exit restarts the instance. Compose
uses `restart: unless-stopped`; the repository's staging topology supplies that policy for the application service.
Deployments without a restart policy leave the instance stopped after escalation. The staging application service uses
`stop_grace_period: 35s`, allowing the thirty-second worker drain plus five seconds of headroom. HTTP requests drain
concurrently with workers; requests still running when the container grace period expires can be terminated by SIGKILL.

An operator who deploys the published image runs the process as the fixed, non-root `reverie` user (numeric id 10001,
pinned rather than allocated by `useradd -r` so it does not shift between base-image rebuilds); the `ENTRYPOINT` invokes
the binary directly with no shell, and the `HEALTHCHECK` runs `curl` in exec form for the same reason. An operator whose
Postgres has never been migrated must either run `reverie migrate` before first starting the server, or set
`REVERIE_AUTO_MIGRATE=true` and ensure the `HEALTHCHECK`'s start period also covers the resulting in-process migration
on first boot, since the default path fails closed on a never-migrated schema rather than serving in a degraded state.
An operator who sets `REVERIE_FRONTEND_DIST_PATH` to a directory whose build is invalid gets a startup failure rather
than a server that silently omits its security headers (owned by the response-headers subject, sequenced by this one in
Runtime behaviour).

## More information

- [Backend directory orientation](../../../../backend/README.md): local development setup; not a runtime walkthrough.
- [Database migrations](../../../../docs/deployment/database-migrations.md): the operator-facing explanation of the two
  migration identities and when to choose `reverie migrate` versus `REVERIE_AUTO_MIGRATE`.
