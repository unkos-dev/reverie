# Backend — Rust + Axum

## Dev Database

Start dev postgres: `docker compose up -d` from repo root.
Port 5433 (5432 taken by host's shared-postgres).

**Roles** (created by `docker/init-roles.sql` on first start):

| Role                | Connection                                                                  | Purpose                                               |
| ------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------- |
| `reverie`           | `postgres://reverie:reverie@localhost:5433/reverie_dev`                     | Schema owner. Runs migrations. Never used at runtime. |
| `reverie_app`       | `postgres://reverie_app:reverie_app@localhost:5433/reverie_dev`             | Web application. RLS enforced.                        |
| `reverie_ingestion` | `postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev` | Background pipeline. Scoped RLS.                      |
| `reverie_readonly`  | `postgres://reverie_readonly:reverie_readonly@localhost:5433/reverie_dev`   | Debug/reporting. SELECT only.                         |

`tower_sessions` schema (created by the consolidated
`20260526000000_initial_schema` migration) RLS-exempt — session
rows not user-scoped like application data. Session id
(cryptographically random `tower_sessions::session::Id`) bootstraps
user resolution, so RLS-gating lookup is chicken-and-egg. Access
controlled at role-grant boundary: `reverie_app` gets DML,
`reverie_readonly` gets SELECT, `reverie_ingestion` gets nothing.

Migrations auto-apply on startup via `db::run_migrations()`. The runner
uses `DATABASE_URL_MIGRATION` (schema-owner DSN, required) to connect an
ephemeral single-connection pool, applies all pending migrations in a
batch transaction (all-or-nothing), then drops the pool before runtime
pools are created. See `adr/2026-05-26-auto-migration-on-startup.md`.

Dev: set `DATABASE_URL_MIGRATION=postgres://reverie:reverie@localhost:5433/reverie_dev`
(same as schema owner). `#[sqlx::test]` still uses sqlx's built-in
migrator for most tests. Exception: migration-runner tests that need a
fresh database use `#[sqlx::test(migrations = false)]` to suppress
sqlx's automatic migration pass.

**`MigrationError` failure modes** (operator-facing, with recovery):

| Variant              | Meaning                                            | Recovery                                             |
| -------------------- | -------------------------------------------------- | ---------------------------------------------------- |
| `Connection`         | Bad DSN, auth failure, unreachable host            | Fix `DATABASE_URL_MIGRATION`                         |
| `SessionSetup`       | Post-connect init failed (lock_timeout, lock acq)  | Check DB permissions and concurrent connections      |
| `BatchFailed`        | SQL error in transactional migration               | DB untouched — pin previous image                    |
| `NoTxFailed`         | `-- no-transaction` migration SQL failed           | TX migrations committed — fix failing SQL, re-deploy |
| `NoTxTrackingFailed` | No-tx migration applied but tracking INSERT failed | Migration IS applied — manually insert tracking row  |
| `SchemaAhead`        | DB has migrations unknown to binary                | Upgrade image or roll back DB                        |
| `ChecksumMismatch`   | Migration file modified after application          | Restore original migration file                      |
| `LockTimeout`        | Advisory lock not acquired (30s budget)            | Another instance running migrations                  |

### Upgrade note: postgres:18 mount path

Volume mount changed from `pgdata:/var/lib/postgresql/data` to
`pgdata:/var/lib/postgresql` to match postgres:18 major-version
subdirectory layout. Existing dev volumes from before change must be
dropped:

```bash
docker compose down
docker volume rm reverie_pgdata
docker compose up -d
# Migrations auto-apply on next `cargo run` (no manual sqlx migrate needed)
```

### Coder workspace caveat

Inside Coder workspace (DooD), bind-mounts of files at workspace paths
don't resolve on host docker daemon — `init-roles.sql` mounted as
directory, entrypoint init silently fails to seed runtime roles. Seed
manually after `docker compose up -d`:

```bash
docker cp docker/init-roles.sql reverie-postgres:/tmp/init-roles.sql
docker exec reverie-postgres psql -U reverie -d reverie_dev -f /tmp/init-roles.sql
```

Real LXC hosts don't have this constraint — staging deploy needs no
workaround.

## Conventions

- **Error handling:** `thiserror` for library errors, `anyhow` for
  application errors. Axum handlers return
  `Result<impl IntoResponse, AppError>` where `AppError` implements
  `IntoResponse`.
- **Database:** `sqlx` with compile-time checked queries — `query!`,
  `query_as!`, `query_scalar!` macros validate SQL + types against live
  dev DB at compile time, then check against committed `backend/.sqlx/`
  cache for offline builds (CI, Docker). Data-path queries went
  all-macro under [UNK-167](https://linear.app/unkos/issue/UNK-167)
  (PR series #157–#162, closer #163). Runtime `sqlx::query(...)`
  reserved for documented carve-outs only; CI grep-guard
  (`.github/sqlx-runtime-allowlist.txt`) fails any new invocation
  outside registry. Carve-out classes:
  - **DDL** (`CREATE`, `DROP`, `ALTER TYPE`) — macros can't validate
    against schema not existing yet at prepare time.
  - **Dynamic SQL** built from runtime input (rare; flag in review).
  - **`SELECT set_config(...)`** — Postgres GUC calls for RLS context
    injection (`app.current_user_id`, transaction-local — RLS
    enforcement seam consumed by every user-facing query) and
    writeback pool identity (`app.system_context`, session-scoped —
    seam `manifestations_*_system` policies match against). Not data
    access; macros can't validate GUC mutation against schema at
    prepare time.
  - **Enum-drift test probes** — `models/manifestation_format.rs`,
    `models/user.rs`, and `models/validation_status.rs` use
    `ALTER TYPE ... ADD VALUE` + cast to detect code-vs-schema enum
    drift at test time; all three need runtime SQL.

  Canonical carve-out registry: `.github/sqlx-runtime-allowlist.txt`.
  New entry needs reviewer justification in PR adding it.

  Established type-binding tactics (see `backend/src/models/work.rs`
  and `backend/src/models/user.rs`):
  - Custom Postgres ENUMs from string params: bind as text, cast in
    SQL — `($N::text)::enum_type`. Avoids Rust→PG-enum mapping for
    `&str`.
  - NUMERIC columns from `f64` / `Option<f64>`: bind as `float8`, let
    Postgres implicitly cast — `$N::float8`. Avoids sqlx's
    `bigdecimal` feature.
  - `query_as!` struct fields with custom enum types: use column-type
    override syntax — `column AS "name: Type"`. Forces macro to
    validate column's PG OID against Rust type's `sqlx::Type` impl at
    prepare time.
  - `format!()`-injected column lists are dynamic SQL, incompatible
    with macros. Inline columns at each call site; macro validation
    catches column drift independently per site.

  Cache regeneration: `DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo sqlx prepare -- --tests`
  from `backend/`. CI guards against stale cache via
  `cargo sqlx prepare --check -- --tests`. Migrations in
  `backend/migrations/`.

- **Testing:** `axum-test` for integration tests. Unit tests live
  alongside code in `#[cfg(test)]` modules.
- **DB-backed tests use `#[sqlx::test(migrations = "./migrations")]`.**
  Macro provisions fresh isolated database per test, runs every
  migration, injects `PgPool` owned by schema owner (`reverie` —
  bypasses RLS). Tests needing runtime roles (`reverie_app`,
  `reverie_ingestion`) build secondary pools against same per-test DB
  via `crate::test_support::db::{app_pool_for, ingestion_pool_for}`.
  Tests run parallel via database isolation; no manual fixture cleanup
  required. `DATABASE_URL` must point at schema owner so `sqlx::test`
  can create per-test databases (locally:
  `postgres://reverie:reverie@localhost:5433/reverie_dev`).
- **OIDC integration tests use `crate::test_support::oidc_mock`.**
  Spins up `wiremock` server with `/jwks` + `/token` endpoints,
  generates per-test 2048-bit RSA keypair, exposes `OidcClient` with
  JWKS embedded so `id_token_verifier` needs no network IO. Tests
  needing OIDC `nonce` set in session by `/auth/login` build router
  via `crate::build_router_with_session_store(state, auth_backend, store)`
  with shared `tower_sessions::MemoryStore` so test can read back
  before driving `/auth/callback`.
- **Logging:** `tracing` with structured fields. Never `println!` or
  `eprintln!`.
- **Operator env-var namespacing:** when introducing operator-facing
  knob overlapping with Rust ecosystem default (e.g. `RUST_LOG`),
  prefer `REVERIE_*` name and cascade with `REVERIE_*` taking
  precedence over ecosystem name. Resolve cascade once in `config.rs`
  so precedence is single source of truth. Rationale: operators read
  `REVERIE_*` namespace from staging docs; devs reach for ecosystem
  default. Cascading honours both without forcing either audience to
  learn the other. Exception: ecosystem names that _are_ canonical
  operator surface (e.g. `DATABASE_URL` — URL-spec name, no namespace
  alternative) honoured directly without cascade.
- **Formatting:** `cargo fmt` enforced by CI. Don't fight formatter.
- **Linting:** `cargo clippy -- -D warnings` enforced by CI. Fix
  warnings, don't suppress with `#[allow(...)]` unless documented
  reason.
- **Pre-push hook:** `.husky/pre-push` runs `cargo fmt --all -- --check`
  then `cargo clippy --workspace --all-targets --locked -- -D warnings`
  on every push, catching the fmt/clippy CI round-trip locally. Budget:
  ~35s warm on Coder workspace baseline. `cargo test` is deliberately
  excluded — 3–5 min wall-time plus shared dev-DB contention across
  worktrees (CI remains the authoritative test gate). Frontend is not
  mirrored: `pre-commit` lint-staged already runs frontend checks on
  staged changes, so a frontend pre-push would duplicate that. The
  hook's clippy is intentionally wider than CI's
  `cargo clippy -- -D warnings` (`ci.yml`): `--all-targets` lints
  test/bench/example code and `--locked` pins deps, so the hook blocks
  locally what CI would currently miss.
- **Time:** use `time` crate, not `chrono`. Blueprint mentions chrono
  but scaffold predates that decision — don't reintroduce chrono in
  first-party code. Single documented exception:
  `test_support.rs::oidc_mock`, where `openidconnect` v4 public API
  (`CoreIdTokenClaims::new`) forces chrono types on call site. That
  use is contained to OIDC mock, must not spread elsewhere.

## Rust Code Rules

Project-specific hard rules. Broader Rust idioms (ownership, iterators,
trait design, pattern matching, lifetime minimization) live in
`rust-patterns` skill — invoke for deep patterns.

- **No `unwrap()` or `expect()` in non-test code** — compiler-enforced
  via `clippy::unwrap_used = "deny"` / `expect_used = "deny"` in
  `Cargo.toml`. Propagate with `?` or handle explicitly. Tests may use
  them freely because `backend/clippy.toml` sets
  `allow-unwrap-in-tests = true` and `allow-expect-in-tests = true`;
  exemption covers `#[test]` functions, `#[cfg(test)]` modules, and
  integration tests under `tests/`.
- **No `let _ = <Result>`.** Either log and continue via
  `if let Err(e) = ... { tracing::warn!(…); }`, or propagate with `?`.
  Silently discarding errors is forbidden.
- **No wildcard imports** (`use foo::*`). Name what you import.
- **`&str` over `String` in function parameters** when function doesn't
  need ownership. Callers pass owned strings via auto-deref.
- **`#[non_exhaustive]` on public enums and structs that may grow** at
  crate boundaries — protects downstream `match` exhaustiveness from
  breakage.
- **Enums over boolean flags** for distinct states with different
  behaviour (`enum Mode { Read, Write, ReadWrite }`, not
  `read: bool, write: bool`).
- **`From<SourceError>` via `thiserror`'s `#[from]`** for `?`
  propagation across error boundaries.
- **`unsafe` requires `// SAFETY:` comment per block** explaining
  invariant. Adjacent unsafe blocks under same invariant each get own
  comment. Crate-level `unsafe_code = "deny"` (see `Cargo.toml`)
  enforces scope at boundary; only `#[allow(unsafe_code)]`-marked code
  may use unsafe, marking requires reviewer justification.

## Security headers (UNK-106)

Backend owns response-header policy. Every response carries XCTO,
Referrer-Policy, Permissions-Policy, X-Frame-Options unconditionally,
and route-class-differentiated `Content-Security-Policy`: HTML routes
get hash-based CSP (one inline FOUC script pinned via `'sha256-...'`),
API routes get `default-src 'none'`.

- Implementation: `backend/src/security/` (`csp.rs` builders,
  `dist_validation.rs` startup check, `headers.rs` middleware +
  composite fallback).
- Wiring: `backend/src/lib.rs::run` precomputes CSP strings on
  `SecurityConfig` at startup; `build_router_with_session_store`
  applies per-router `.layer(api_csp_layer)` / `.layer(html_csp_layer)`
  plus outermost `security_headers` uniform middleware; single
  composite `.fallback(composite_fallback)` manually attaches CSP to
  unmatched paths. `build_router` is thin wrapper calling
  `build_router_with_session_store` with `PostgresStore` (backed by
  `state.pool`) for production; tests pass own `MemoryStore` to share
  session state with harness (see Testing in `## Conventions`).
- Operator surface: `docs/security/content-security-policy.md`.
- Tests: `backend/src/security/**/tests` co-located; integration tests
  in `security::headers::tests` use `test_server_with_security()` to
  inject custom `SecurityConfig` fixtures.

**Never add inline `<script>` tags to `frontend/index.html` without
matching CSP hash.** Vite plugin `frontend/vite-plugins/csp-hash.ts`
hashes one specific script (`frontend/src/fouc/fouc.js`) at build
time. Additional inline scripts need either new hash source in plugin
or overhaul to nonce-based CSP (out of scope pre-v1.0).

**Never emit duplicate CSP headers from reverse proxy.** Reverie's CSP
is route-class-differentiated; stacking proxy-level CSP on top
nullifies differentiation.

## Database Migration Rules

- **Pre-v1.0 schema freely mutable.** Add migrations and constraints
  now rather than deferring for future cleanup PR.
- **Enum column type changes:** `DROP DEFAULT` before
  `ALTER COLUMN TYPE`, then `SET DEFAULT` after. Postgres requires
  default expression to type-check against current column type.
- **Test data for `find_or_create` with `pg_trgm`:** titles must use
  distinct vocabulary. Shared words push trigram similarity above 0.6
  match threshold, cause false-positive de-duplication in tests.

## Project Structure (as it grows)

```text
backend/
├── Cargo.toml           # [lib] reverie_api + [[bin]] reverie-api
├── migrations/          # sqlx migrations
├── src/
│   ├── lib.rs           # Library crate root: modules, build_router, run()
│   ├── main.rs          # Thin binary entry: #[tokio::main] reverie_api::run()
│   ├── auth/            # Authentication subsystem
│   │   ├── backend.rs   # axum-login AuthnBackend (OIDC credentials)
│   │   ├── basic_only.rs # BasicOnly extractor (OPDS Basic-only auth)
│   │   ├── middleware.rs # CurrentUser extractor (session + Basic auth)
│   │   ├── oidc.rs      # OIDC client init and discovery
│   │   ├── theme_cookie.rs # FOUC theme cookie (set_theme_cookie, attribute parity)
│   │   └── token.rs     # Device token generation and sha256 constant-time verification
│   ├── routes/          # Axum route handlers, grouped by domain
│   ├── models/          # Database models and queries
│   ├── services/        # Business logic
│   ├── config.rs        # Environment-based configuration
│   ├── state.rs         # AppState (shared across handlers)
│   └── error.rs         # AppError type
└── tests/               # Integration tests (if separate from unit tests)
```
