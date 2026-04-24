# Backend — Rust + Axum

## Dev Database

Start the dev postgres: `docker compose up -d` from the repo root.
Port 5433 (5432 is taken by the host's shared-postgres).

**Roles** (created by `docker/init-roles.sql` on first start):

| Role | Connection | Purpose |
|------|-----------|---------|
| `reverie` | `postgres://reverie:reverie@localhost:5433/reverie_dev` | Schema owner. Runs migrations. Never used at runtime. |
| `reverie_app` | `postgres://reverie_app:reverie_app@localhost:5433/reverie_dev` | Web application. RLS enforced. |
| `reverie_ingestion` | `postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev` | Background pipeline. Scoped RLS. |
| `reverie_readonly` | `postgres://reverie_readonly:reverie_readonly@localhost:5433/reverie_dev` | Debug/reporting. SELECT only. |

Run migrations as the schema owner:
`DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev sqlx migrate run`

## Conventions

- **Error handling:** Use `thiserror` for library errors, `anyhow` for application
  errors. Axum handlers return `Result<impl IntoResponse, AppError>` where `AppError`
  implements `IntoResponse`.
- **Database:** `sqlx` with compile-time checked queries. Migrations in
  `backend/migrations/`.
- **Testing:** Use `axum-test` for integration tests. Unit tests live alongside the
  code in `#[cfg(test)]` modules.
- **DB-backed tests use `#[sqlx::test(migrations = "./migrations")]`.** The macro
  provisions a fresh isolated database per test, runs every migration, and
  injects a `PgPool` owned by the schema owner (`reverie` — bypasses RLS). Tests
  that need the runtime roles (`reverie_app`, `reverie_ingestion`) build
  secondary pools against the same per-test DB via
  `crate::test_support::db::{app_pool_for, ingestion_pool_for}`. Tests run in
  parallel thanks to database isolation; no manual fixture cleanup is required.
  `DATABASE_URL` must point at the schema owner so `sqlx::test` can create
  per-test databases (locally: `postgres://reverie:reverie@localhost:5433/reverie_dev`).
- **Logging:** Use `tracing` with structured fields. Never `println!` or `eprintln!`.
- **Formatting:** `cargo fmt` is enforced by CI. Do not fight the formatter.
- **Linting:** `cargo clippy -- -D warnings` is enforced by CI. Fix warnings, don't
  suppress them with `#[allow(...)]` unless there's a documented reason.
- **Time:** use the `time` crate, not `chrono`. The blueprint mentions chrono
  but the scaffold predates that decision — don't reintroduce chrono.

## Rust Code Rules

Project-specific hard rules. Broader Rust idioms (ownership, iterators,
trait design, pattern matching, lifetime minimization) live in the
`rust-patterns` skill — invoke it for deep patterns.

- **No `unwrap()` or `expect()` in non-test code.** Propagate with `?` or
  handle explicitly. Tests may use them freely.
- **No `let _ = <Result>`.** Either log and continue via
  `if let Err(e) = ... { tracing::warn!(…); }`, or propagate with `?`.
  Silently discarding errors is forbidden.
- **No wildcard imports** (`use foo::*`). Name what you import.
- **`&str` over `String` in function parameters** when the function does not
  need ownership. Callers pass owned strings via auto-deref.
- **`#[non_exhaustive]` on public enums and structs that may grow** at crate
  boundaries — protects downstream `match` exhaustiveness from breakage.
- **Enums over boolean flags** for distinct states with different behaviour
  (`enum Mode { Read, Write, ReadWrite }`, not `read: bool, write: bool`).
- **`From<SourceError>` via `thiserror`'s `#[from]`** for `?` propagation
  across error boundaries.
- **`unsafe` requires a `// SAFETY:` comment per block** explaining the
  invariant. Adjacent unsafe blocks under the same invariant each get their
  own comment. Crate-level `unsafe_code = "deny"` (see `Cargo.toml`) enforces
  scope at the boundary; only `#[allow(unsafe_code)]`-marked code may use
  unsafe, and that marking requires reviewer justification.

## Database Migration Rules

- **Pre-v1.0 schema is freely mutable.** Add migrations and constraints now
  rather than deferring for a future cleanup PR.
- **Enum column type changes:** `DROP DEFAULT` before `ALTER COLUMN TYPE`,
  then `SET DEFAULT` after. Postgres requires the default expression to
  type-check against the current column type.
- **Test data for `find_or_create` with `pg_trgm`:** titles must use distinct
  vocabulary. Shared words push trigram similarity above the 0.6 match
  threshold and cause false-positive de-duplication in tests.

## Project Structure (as it grows)

```text
backend/
├── Cargo.toml
├── migrations/          # sqlx migrations
├── src/
│   ├── main.rs          # Entrypoint, router assembly, server setup
│   ├── auth/            # Authentication subsystem
│   │   ├── backend.rs   # axum-login AuthnBackend (OIDC credentials)
│   │   ├── basic_only.rs # BasicOnly extractor (OPDS Basic-only auth)
│   │   ├── middleware.rs # CurrentUser extractor (session + Basic auth)
│   │   ├── oidc.rs      # OIDC client init and discovery
│   │   └── token.rs     # Device token generation and sha256 constant-time verification
│   ├── routes/          # Axum route handlers, grouped by domain
│   ├── models/          # Database models and queries
│   ├── services/        # Business logic
│   ├── config.rs        # Environment-based configuration
│   ├── state.rs         # AppState (shared across handlers)
│   └── error.rs         # AppError type
└── tests/               # Integration tests (if separate from unit tests)
```
