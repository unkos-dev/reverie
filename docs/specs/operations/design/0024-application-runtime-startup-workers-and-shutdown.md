---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0024"
title: "Application runtime: startup, workers, and shutdown"
satisfies:
  - "REV-REQ-0064"
governed-by:
  - "REV-ADR-0021"
---

# Application runtime: startup, workers, and shutdown

This Design covers the process lifecycle of the `reverie-api` binary: how the command line selects between the
long-lived server and its one-shot administrative subcommands, the ordered sequence `run` executes before it accepts a
request, the five background workers it spawns and the shared-deadline drain that shuts them down, the liveness and
readiness probes, and the runtime stage of the container image that packages all of it.

## Purpose and boundaries

This subject owns: CLI subcommand dispatch (`Command`, `parse_command`); the ordered, fallible setup sequence inside
`run` (configuration load, CSP header finalisation, tracing, database pools, the schema apply-or-verify step, admin
bootstrap, OIDC and JWT client construction, settings load, login limiter construction, `AppState` assembly, router
build, listener bind); when and in what order the five background workers are spawned; the shared-deadline drain that
awaits them on shutdown; the `/health` and `/health/ready` probes; and the `runtime` stage of `Dockerfile` (its
`ENTRYPOINT`, `HEALTHCHECK`, `USER` and `EXPOSE`).

It does not own: what each background worker does once running (the ingestion pipeline, the enrichment queue, the
writeback pipeline, the settings live-reload mechanism and the session sweep are each their own subject); the meaning or
validation of individual configuration fields, or the `figment` pipeline that produces a `Config`
(`backend/src/config/`); how a schema migration is applied, verified, or which role runs it (`backend/src/db.rs`); the
content of the CSP headers `run` finalises, or the frontend-dist validation it calls (`backend/src/security/csp.rs`,
`backend/src/security/dist_validation.rs`); session cookie attributes or the session store's own persistence
(`backend/src/auth/session.rs`, `backend/src/auth/store.rs`); OIDC discovery or JWT validation internals
(`backend/src/auth/oidc.rs`, `backend/src/auth/jwt.rs`); or the identity mechanics behind the `bootstrap`,
`reset-password` and `unlock-account` subcommands, which this subject only dispatches to (`backend/src/models/user.rs`,
`backend/src/auth/recovery.rs`, `backend/src/models/login_throttle.rs`). It does not own the builder or frontend stages
of `Dockerfile`, only the `runtime` stage that consumes their output. It does not own cross-instance coordination,
because there is none to own: `run` performs no leader election, node discovery or peer handshake, and every instance of
the process spawns the same five workers and serves the same routes.

Depends on: `config::Config` for every setting `run` reads; `db::init_pool`, `db::init_writeback_pool`,
`db::run_migrations` and `db::verify_schema_current` for every database connection it opens;
`auth::oidc::init_oidc_client` and `auth::jwt::init_jwt_validator` for the two optional identity clients it constructs;
`security::csp::build_api_csp`, `security::csp::build_html_csp` and `security::dist_validation::validate_frontend_dist`
for the CSP headers it finalises before building the router; `services::settings::load` for the initial settings
snapshot; `build_router` for the Axum router it serves; and the five worker entry points named in Structure below.

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
  behaviour, spawns the five background workers, serves HTTP until shutdown, then drains the workers before returning.
- `backend/src/lib.rs::build_router` and `::build_router_with_session_store` assemble the Axum router `run` serves: the
  reserved-prefix routes, the OPDS mount when enabled, the SPA fallback when a frontend dist path is configured, and the
  middleware layers (CSP, CSRF, the security-headers wrapper, the `problem_instance_layer`, the session layer, tracing).
  Each layer's own behaviour belongs to its own subject; this Design only fixes the order in which
  `build_router_with_session_store` composes them, which decides what a rejection at one layer carries from the layers
  outside it.
- `backend/src/lib.rs::drain_workers` is the shutdown-side counterpart to spawning: it awaits a `Vec` of named
  `JoinHandle<()>`s against one shared deadline and aborts (with a bounded grace check) any that overruns it.
- `backend/src/lib.rs::apply_or_verify_schema` is the schema-step flag selector: it branches on `config.auto_migrate`
  between an in-process migration run (`db::run_migrations`) and a read-only verification (`db::verify_schema_current`).
  It is extracted from `run` specifically so both branches are pinned by tests.
- `backend/src/lib.rs::shutdown_signal` races `tokio::signal::ctrl_c()` against a registered SIGTERM handler and cancels
  the shared `CancellationToken` when either resolves.
- `backend/src/state.rs::AppState` is the `Clone` handle `run` builds once and threads into the router, the request
  handlers and (via per-task clones of its constituent fields) the background workers. Its fields are documented in Data
  and state below.
- `backend/src/routes/health.rs` supplies `GET /health` (liveness: always `200 ok`) and `GET /health/ready` (readiness:
  pings the application pool with `SELECT 1`, returning `200 ok` or a `503` Problem Details body). Both are outside
  `/api/v1` and carry an explicit empty `security(())` OpenAPI annotation, opting out of the document-level
  session-cookie default.
- The five background workers, each entered from its own module and given only what it needs:
  - `services::settings::spawn_listener` (settings LISTEN/NOTIFY reload, refreshing `AppState.settings`).
  - `services::ingestion::run_watcher` (the filesystem watcher and ingestion scan loop).
  - `services::enrichment::queue::spawn_queue` (the enrichment job queue).
  - `services::session_sweep::run_sweep` (the hourly expired-session reaper, driving `PostgresStore`'s `ExpiredDeletion`
    trait).
  - `services::writeback::queue::spawn_worker` (the writeback job queue, given the dedicated system-context pool
    described in Data and state).
- `Dockerfile`'s `runtime` stage (the final `FROM debian:trixie-slim ... AS runtime` block) copies the release binary
  and the built frontend, creates a fixed non-root user, sets `REVERIE_FRONTEND_DIST_PATH`, and declares the
  `ENTRYPOINT` and `HEALTHCHECK` this subject's binary and readiness probe satisfy.

### State-writer census

The only piece of shared, mutable runtime-coordination state this subject owns is the shutdown `CancellationToken`
created in `run`. Every other field `run` builds (`AppState`, the two pool handles, the OIDC and JWT clients) is written
once at construction and never mutated again through this subject's own code.

The token is a local binding in `run`, cloned once per worker and once more into `shutdown_signal`. Two call sites write
its cancelled flag: `shutdown_signal` (on `ctrl_c()` or SIGTERM) and `run` itself, unconditionally, immediately after
`axum::serve` returns. Both call `.cancel()` on a clone that shares the same underlying token, and
`CancellationToken::cancel` is idempotent: whichever call site runs first actually flips the flag, and the other is a
no-op. No worker, and no other part of this subject, calls `.cancel()`; every worker only ever reads the token (via
`cancel.cancelled()` or an equivalent `tokio::select!` arm inside its own loop) to learn when to stop.

## Interfaces and dependencies

- The CLI surface `parse_command` accepts is the only interface `main.rs` exposes: no argument (`Serve`), or exactly one
  of `migrate`, `print-config-schema`, `bootstrap`, `reset-password <email>`, `unlock-account <email>`. Each subcommand
  other than `Serve` reads only what its own task needs directly from the environment or a minimally built `Config`,
  rather than the full startup path: `run_migrate` reads only `DATABASE_URL_MIGRATION`, deliberately bypassing
  `Config::from_env` so a migration-only invocation never holds the OIDC secret or the application DSN.
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
  `config` is owned, cloned data; `oidc_client` and `jwt_validator` are `Option`s, `None` on an instance that has not
  configured the corresponding identity mode; `login_limiter` is an `Arc<LoginLimiter>`; `last_settings_reload` is an
  `Arc<RwLock<..>>` handle written only by the settings worker, not by this subject; `settings` is likewise an
  `Arc<RwLock<..>>` handle, but it has a second writer outside this subject — the `PUT /api/v1/settings` route handler
  also writes it directly, as an immediate local-cache update guarded by `apply_if_newer`'s revision check — so neither
  writer belongs to this subject, but there is more than one.
- **The writeback pool** (`backend/src/db.rs::init_writeback_pool`) is built after `AppState` and is deliberately not
  one of its fields: it is handed directly to the writeback worker's `tokio::spawn` closure and nowhere else. No request
  handler receives it, because no request handler receives anything not reachable through `AppState`.
- **The five worker `JoinHandle<()>`s** are local bindings in `run`, moved into one `Vec` and consumed by
  `drain_workers` after `axum::serve` returns; nothing else observes them.
- **`WORKER_DRAIN_TIMEOUT`** (30 seconds) and **`ABORT_GRACE`** (1 second) are compile-time constants in
  `backend/src/lib.rs`; changing either needs a code change, not a configuration change.
- **Per-worker enabled flags.** `config.enrichment.enabled` and `config.writeback.enabled` are read once, inside each
  worker's own entry function, not by `run`: `run` spawns all five workers unconditionally regardless of these flags.
  When a flag is `false`, that worker's function logs once and then only awaits `cancel.cancelled()`, returning as soon
  as the shared token fires; its `JoinHandle` is otherwise indistinguishable, at drain time, from one that ran its full
  loop. The ingestion watcher (`services::ingestion::run_watcher`) has no such flag in its config section, so it always
  runs for the life of the process.

## Runtime behaviour

**From process start to the first request accepted**, on the default `Serve` path:

1. `Config::from_env` loads and validates configuration; any failure here (a missing or invalid environment variable)
   returns immediately, before anything else in this list runs.
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
6. `seed_admin_if_configured` creates the first administrator from `REVERIE_BOOTSTRAP_*` when configured and no
   administrator yet exists; it is a no-op otherwise.
7. The OIDC client is constructed when `config.oidc_configured()` is true, otherwise `AppState.oidc_client` stays
   `None`. The resource-server JWT validator is constructed independently when `config.resource_server_configured()` is
   true.
8. `db::init_pool` opens the ingestion pool; `services::settings::load` reads the initial settings row; the login rate
   limiter is built from `config.login_rate_per_min`.
9. `AppState` is assembled and `build_router` is called on a clone of it.
10. `db::init_writeback_pool` opens the writeback pool, and the TCP listener is bound. One more fallible call follows
    immediately: `listener.local_addr()` is read back to log the bound address. Nothing has been spawned yet at this
    point, so an early return from any of these steps cannot leak a running task.
11. A `CancellationToken` is created, and the five workers are spawned in this order: the settings listener, the
    ingestion watcher, the enrichment queue, the session sweep, then the writeback worker. Each receives its own clone
    of the token and (where relevant) its own clone of the pools and configuration it needs.
12. `axum::serve` begins accepting connections, wrapped in `with_graceful_shutdown(shutdown_signal(...))`.

**A graceful shutdown**, triggered by SIGTERM or Ctrl+C while serving:

1. `shutdown_signal`'s `tokio::select!` resolves on whichever of `ctrl_c()` or the registered SIGTERM handler fires
   first, logs once, and calls `cancel_token.cancel()`.
2. Every worker's own loop observes the same token becoming cancelled and begins its own exit path (for example, the
   writeback worker's shutdown-time revert of any `in_progress` job back to `pending`, described in its own subject).
3. Axum's graceful-shutdown future resolving makes `axum::serve` stop accepting new connections, finish in-flight ones,
   and return; `run` binds this as `serve_result`.
4. `run` calls `cancel_token.cancel()` again unconditionally. Because step 1 already cancelled the same underlying
   token, this is a no-op on the clean-shutdown path; it exists for the unclean path below.
5. `drain_workers` computes one deadline (`now + WORKER_DRAIN_TIMEOUT`) and awaits the five `JoinHandle`s in the order
   they were spawned, each against that same deadline via `tokio::time::timeout_at`. A handle that resolves before the
   deadline logs `"background worker drained"`; one that resolves with a `JoinError` (a panic) logs the error and does
   not stop the loop from draining the remaining workers. A handle still pending at the deadline is aborted, logged, and
   given the separate, short `ABORT_GRACE` window to confirm the abort actually took effect before `drain_workers` moves
   on. Because the deadline is one fixed instant rather than a per-worker budget, a worker that overruns it exhausts the
   time remaining for every worker still queued behind it: once the deadline has passed, each subsequent `timeout_at`
   call returns immediately as elapsed.
6. `run` returns `serve_result` to `main`, which propagates a non-zero exit only when it carries an error.

**An unclean shutdown**, where `axum::serve` itself returns an `Err` (for example, an accept-loop failure) without a
signal ever having arrived: `shutdown_signal`'s future never resolves, so step 1 above never runs and the token is never
cancelled on that path. `run`'s own unconditional `cancel_token.cancel()` (step 4 above) is what cancels it in this
case, and `drain_workers` still runs before `run` returns the `Err` — the every-fallible-step-before-any-spawn ordering
at startup has a shutdown-side counterpart: a serve failure still drains every worker rather than returning immediately
and leaving them for the Tokio runtime to tear down.

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
  writeback), OIDC discovery, JWT-validator construction, the TCP listener bind, reading the bound address back from the
  listener, or `axum::serve` itself. Because none of these runs after the first `tokio::spawn`, none of them can leave a
  worker running with nothing left to drain it.
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
- **A worker's own function returning an `Err` while running.** The ingestion watcher, enrichment queue and writeback
  worker are each spawned inside a closure of the shape
  `if let Err(e) = <worker fn>(...).await { tracing::error!(...) }`; the closure itself always returns `()`, so the
  resulting `JoinHandle<()>` always resolves `Ok(())` regardless of whether the worker's own function failed internally.
  At drain time this means a worker that exited early because of an internal error is logged once, at the point of
  failure, and is otherwise indistinguishable from a worker that ran cleanly for the life of the process: nothing in
  this subject restarts it, and nothing re-surfaces the failure once the process has moved past that point in its logs.
- **A worker overrunning the shared drain deadline.** Covered in Runtime behaviour: it is aborted, logged, and given a
  short bounded window to confirm the abort took effect; if it is still running after that window, a final error is
  logged and the worker is left for the Tokio runtime to tear down when the process itself exits.

## Security and operations

The default schema-management path (`auto_migrate` unset or `false`) never puts a migration-capable database credential
in the long-lived server process. Configuration loading forces `config.migration_database_url` to `None` whenever
`auto_migrate` is `false` (covered by that subject, not restated here), and `apply_or_verify_schema`'s default branch
calls `db::verify_schema_current` against the application pool only, never reading `migration_database_url` at all; the
opt-in branch is the only code path in this subject that does. Both directions are pinned by tests
(`apply_or_verify_flag_off_takes_verify_branch`, `apply_or_verify_flag_on_takes_migrate_branch`), so an inverted branch
condition fails the suite rather than shipping silently. The shipped default keeps the migration credential entirely on
the out-of-band `reverie migrate` path.

The writeback pool's every connection sets the `app.system_context` GUC that the `manifestations_*_system`
row-level-security policies key on (owned by the row-level-security subject, not restated here); this subject's own
contribution to that boundary is structural rather than a runtime check: the writeback pool is never placed on
`AppState`, so no request handler holds a reference to it and none can reach the system-context policies by accident,
whatever the RLS policies themselves would otherwise allow.

The bootstrap seed password (`REVERIE_BOOTSTRAP_PASSWORD`) is read directly from the environment in
`seed_admin_if_configured` rather than retained on the long-lived `Config`, and only the resulting account's email is
logged, never the password.

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
