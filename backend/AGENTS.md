# Backend Agent Operating Manual

_(Operator/DB detail: see `./README.md`)_

## Foundations

Rust, Axum, Tokio, SQLx and PostgreSQL are Reverie's chosen backend stack. Use their documented capabilities at the
versions pinned by `rust-toolchain.toml` and `Cargo.lock`. An alternative or workaround needs evidence that the native
capability is deficient for Reverie, with approval governed by the root rules.

## Errors and failures

- **No panics on operational failures:** Production code never uses `unwrap()`. Propagate or handle input,
  configuration, database and I/O failures. `expect()` is permitted only where failure is impossible by construction,
  under a scoped `#[expect(clippy::expect_used, reason = "...")]` that states the invariant. Tests are exempt.
- **No silent failures:** Do not discard errors. `let _ = <Result>`, converting to `.ok()` without checking, and
  `.unwrap_or_default()` on critical operations are banned. Handle expected absence explicitly; log or propagate other
  failures.
- **Typed errors:** Use `thiserror` types where callers distinguish outcomes and `anyhow` for internal diagnostics. Do
  not add `anyhow` context to a typed client failure: `AppError` converts every `anyhow::Error` to a 500. Never expose
  internal database errors, paths or stack traces to API clients.
- **Parse at the boundary:** Convert unstructured input to typed values at the system boundary. Use enums when states
  have distinct behaviour or a boolean would obscure meaning.

## Credentials

- **External supply:** Supply operational credentials externally through direct environment variables or credential
  files selected by `_FILE` variables. Generate credentials for isolated tests. Literals used to test credential
  handling are test data. Production credential defaults and disclosure remain security findings.
- **SAST boundary:** Only proven test-only code may leave production SAST; mixed files, setup tools and secret scans
  stay covered.

## HTTP and Axum

- **Naming and status codes:** Resource collections use plural kebab-case nouns; authentication, protocol and command
  endpoints follow their documented contracts. Return 201 for completed creation, 202 for accepted asynchronous work and
  204 for success without content. Failure statuses follow the failure classes in
  [ADR-0011](../docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md).
- **Extractor order:** The request body extractor (e.g., `Json<T>`, `String`, `Bytes`) is the last argument in the
  handler signature.
- **State:** Use `axum::extract::State` for application dependencies. Request extensions carry request-scoped data set
  by middleware.
- **Responses:** Prefer typed responses and tuples. Fallible JSON API handlers return `Result<T, AppError>`. Convert
  fallible headers before tuple conversion; map construction failures through `AppError`.
- **Outbound timeouts:** Every outbound HTTP request and SMTP submission is bounded by a client or operation timeout.
  `reqwest` applies none by default.

## Database and SQLx

- **No ORM:** Use explicit `sqlx` queries.
- **Compile-time SQL:** `query!`, `query_as!`, and `query_scalar!` are mandatory for the data path. If the macro fails
  because of a missing schema change, update the `.sqlx` cache with `just rust::sqlx-prepare`. Do not downgrade to
  runtime `sqlx::query()` to bypass the compiler.
- **Runtime SQL:** Runtime `sqlx::query(...)` and `sqlx::raw_sql(...)` are limited to the carve-out classes listed in
  `backend/guards/sqlx-runtime-allowlist.txt`. A new use needs an entry of a listed class, with its rationale in the PR.
  A file-level entry does not cover new, unrelated calls in that file.
- **Transaction binding:** Inside a transaction, pass the transaction (e.g., `&mut *tx`) to the query executor. Passing
  `&pool` executes outside the transaction and silently breaks atomicity.
- **Transactions:** Wrap database writes that share an invariant in one transaction. Do not hold a transaction open
  across outbound HTTP calls or filesystem work.
- **No N+1 queries:** Use set-based queries. Do not issue per-row follow-up database queries in a loop.
- **Bounded queries:** Every returned list is bounded by construction: keyset pagination for growing lists, a hard limit
  otherwise.
- **Enum migrations:** When altering an enum column's type, drop an incompatible default before `ALTER COLUMN ... TYPE`
  and restore the intended default afterwards. Check indexes and constraints on the column the same way.

## Local environment and testing

- **Database environment:** `just db-up` owns persistent development provisioning; the database recipes own disposable
  verification clusters. See `./README.md` for transports, credentials and socket DSN syntax.
- **Integration tests:** Use `axum-test`.
- **Database tests:** Use `#[sqlx::test(migrations = "./migrations")]`. It provisions a fresh isolated database per test
  within an invocation-owned disposable cluster. Tests of the migration runner itself start with `migrations = false`.
  Role helpers require the owner-supplied nonempty passwords.
- **OIDC mocking:** Use `crate::test_support::oidc_mock` for auth flows.

## Tooling

- **Formatting and linting:** Respect `cargo fmt` and `cargo clippy`. Fix warnings. A necessary suppression is a scoped
  `#[expect(..., reason = "...")]`, never `#[allow(...)]`; configuration-dependent exceptions follow
  [ADR-0035](../docs/adr/0035-lint-suppressions-self-purge-expect-with-a-reason-never-allow.md).
- **Logging:** Use `tracing` with structured fields. Never use `println!` for diagnostics.
- **Artifact regeneration:** Editing a config/ or API-surface doc-comment regenerates artifacts. Run `just rust::regen`
  and commit them in the same PR; drift tests gate CI.
