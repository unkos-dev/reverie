# Backend — Rust + Axum

## Dev Database

Start the dev postgres: `docker compose up -d` from the repo root.
Connection: `postgres://tome:tome@localhost:5433/tome_dev` (port 5433 — 5432 is
taken by the host's shared-postgres).

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
│   ├── main.rs          # Entrypoint, server setup
│   ├── routes/          # Axum route handlers, grouped by domain
│   ├── models/          # Database models and queries
│   ├── services/        # Business logic
│   └── error.rs         # AppError type
└── tests/               # Integration tests (if separate from unit tests)
```
