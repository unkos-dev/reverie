# Backend — Rust + Axum

## Dev Database

Start the dev postgres: `docker compose up -d` from the repo root.
Port 5433 (5432 is taken by the host's shared-postgres).

**Roles** (created by `docker/init-roles.sql` on first start):

| Role | Connection | Purpose |
|------|-----------|---------|
| `tome` | `postgres://tome:tome@localhost:5433/tome_dev` | Schema owner. Runs migrations. Never used at runtime. |
| `tome_app` | `postgres://tome_app:tome_app@localhost:5433/tome_dev` | Web application. RLS enforced. |
| `tome_ingestion` | `postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev` | Background pipeline. Scoped RLS. |
| `tome_readonly` | `postgres://tome_readonly:tome_readonly@localhost:5433/tome_dev` | Debug/reporting. SELECT only. |

Run migrations as the schema owner:
`DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev sqlx migrate run`

## Conventions

- **Error handling:** Use `thiserror` for library errors, `anyhow` for application
  errors. Axum handlers return `Result<impl IntoResponse, AppError>` where `AppError`
  implements `IntoResponse`.
- **Database:** `sqlx` with compile-time checked queries. Migrations in
  `backend/migrations/`.
- **Testing:** Use `axum-test` for integration tests. Unit tests live alongside the
  code in `#[cfg(test)]` modules.
- **Logging:** Use `tracing` with structured fields. Never `println!` or `eprintln!`.
- **Formatting:** `cargo fmt` is enforced by CI. Do not fight the formatter.
- **Linting:** `cargo clippy -- -D warnings` is enforced by CI. Fix warnings, don't
  suppress them with `#[allow(...)]` unless there's a documented reason.

## Project Structure (as it grows)

```text
backend/
├── Cargo.toml
├── migrations/          # sqlx migrations
├── src/
│   ├── main.rs          # Entrypoint, router assembly, server setup
│   ├── auth/            # Authentication subsystem
│   │   ├── backend.rs   # axum-login AuthnBackend (OIDC credentials)
│   │   ├── middleware.rs # CurrentUser extractor (session + Basic auth)
│   │   ├── oidc.rs      # OIDC client init and discovery
│   │   └── token.rs     # Device token generation and argon2 verification
│   ├── routes/          # Axum route handlers, grouped by domain
│   ├── models/          # Database models and queries
│   ├── services/        # Business logic
│   ├── config.rs        # Environment-based configuration
│   ├── state.rs         # AppState (shared across handlers)
│   └── error.rs         # AppError type
└── tests/               # Integration tests (if separate from unit tests)
```
