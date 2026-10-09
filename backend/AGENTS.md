# Backend Agent Operating Manual

_(Operator/DB detail: see `./README.md`)_

<cardinal_rules>
These rules define the Rust, Axum, and sqlx architecture. Do not deviate.

1. **Intentional Clones:** Borrow (`&T`) by default. Clone when ownership or isolation requires it, and avoid cloning
   merely to silence an unexplained borrow-checker error.
2. **No unwrap():** `unwrap()` and `expect()` are banned in production code. Propagate with `?` or handle explicitly.
   Tests are exempt.
3. **No Silent Failures:** Do not discard errors. `let _ = <Result>`, converting to `.ok()` without checking, and
   `.unwrap_or_default()` on critical operations are banned. Log the error or propagate it.
4. **No N+1 Queries:** Use set-based queries. Do not issue per-row follow-up database queries in a loop.
   </cardinal_rules>

<rust_and_architecture>

- **Parse, Don't Validate:** Convert unstructured data to typed structs at the system boundary. Make invalid states
  unrepresentable using enums and the Newtype pattern (e.g., `struct UserId(u64)`).
- **Error Boundaries:** Use `thiserror` for library boundaries and `anyhow` for application logic. When propagating
  errors with `?`, attach context using `.context("...")` or `.with_context(|| ...)`. Axum handlers must map errors to
  generic client responses by implementing `IntoResponse` on your custom `AppError` type. NEVER expose internal database
  errors, paths, or stack traces to the API client.
- **Enums over Ambiguous Bools:** Use enums when states have distinct behaviour or a boolean would obscure meaning. A
  boolean is acceptable for an inherently binary predicate.
- **Iterators:** Prefer declarative iterator chains (`.filter().map()`) over mutable `for` loops for data
  transformations.
- **Parallel Async:** Run independent async tasks concurrently when doing so improves behaviour without violating
  ordering, rate, resource, or transaction constraints. Use `tokio::join!` to await all or `try_join!` when a failure
  should short-circuit the rest.
- **Unsafe Code:** `unsafe` requires a `// SAFETY:` comment per block explaining the invariant. It is forbidden unless
  strictly necessary and explicitly allowed via `#[allow(unsafe_code)]`.
- **Secrets Management:** Supply operational credentials externally through direct environment variables or credential
  files selected by `_FILE` variables. Generate credentials for isolated tests. Literals used to test credential
  handling are test data. Production credential defaults and disclosure remain security findings. Only proven test-only
  code may leave production SAST; mixed files, setup tools and secret scans stay covered. </rust_and_architecture>

<api_design>

- **RESTful Naming:** URLs must be plural, kebab-case nouns. NEVER use verbs in URLs (e.g., `/getUsers`).
- **Semantic Status Codes:** Return 201 for creation, 422 for validation, 204 for deletion. Do not return 200 for
  everything; status codes are assigned by failure class, see the JSON API conventions ADR. </api_design>

<axum_invariants>

- **Extractor Order:** The request body extractor (e.g., `Json<T>`, `String`, `Bytes`) MUST be the absolute last
  argument in the handler function signature.
- **State Injection:** Use `axum::extract::State` for dependency injection. Do not use the legacy `Extension` extractor
  unless integrating with third-party middleware that strictly requires it.
- **No Manual Responses:** Handlers should return `Result<impl IntoResponse, AppError>`. Rely on the `IntoResponse`
  trait. Do not manually construct `Response` objects using builders in the handler. </axum_invariants>

<database_and_sqlx>

- **No ORM:** Use explicit `sqlx` queries.
- **Compile-Time SQL:** `query!`, `query_as!`, and `query_scalar!` are mandatory for the data path. If the macro fails
  because of a missing schema change, update the `.sqlx` cache with `just rust::sqlx-prepare`. DO NOT downgrade to
  runtime `sqlx::query()` to bypass the compiler.
- **Transaction Binding:** When executing a query inside a transaction, you MUST pass the transaction reference (e.g.,
  `&mut *tx`) to the query executor. Passing the connection `&pool` will execute outside the transaction and silently
  break atomicity.
- **Runtime SQL Ban:** Runtime `sqlx::query(...)` and `sqlx::raw_sql(...)` are strictly reserved for DDL, dynamic SQL,
  Postgres GUCs, enum-drift tests, and static multi-statement operator scripts executed verbatim. A CI grep-gate rejects
  every other invocation: add a justified entry to `backend/guards/sqlx-runtime-allowlist.txt` (with reviewer rationale
  in the PR) in the same PR, or CI fails.
- **Transactions:** Wrap multi-statement state changes in an atomic transaction.
- **Bounded Queries:** Every list must be bounded by construction (keyset/cursor or hard limit). No unbounded scans.
- **Migration Mutations:** If altering an Enum column type, you must `DROP DEFAULT` before `ALTER COLUMN TYPE`, then
  `SET DEFAULT` after. </database_and_sqlx>

<local_environment>

- **Database Reachability:** `just db-up` owns persistent development provisioning and retains generated credentials
  outside checkouts. `just db-migrate` and dev recipes read that state through `scripts/backend-dev-env.sh`, preserving
  process/file precedence and explicit empty inputs. Local migration recipes default to the Unix socket for
  network-isolated sandboxes; the runtime server uses TCP, matching the deployed transport. Both transports require
  SCRAM. Verification recipes provision independent disposable clusters; they never target development or honour
  REVERIE_DEV_DB_URL. Test mode leaves the bootstrap database empty for SQLx per-test migrations; schema mode prepares
  it for online SQLx compilation. Socket DSNs use `postgres:///database?host=<dir>&user=<role>&password=<pw>`; SQLx
  rejects `postgres://user@/database?host=...`. See `./README.md` for lifecycle and explicit-supply instructions.
  </local_environment>

<testing_standards>

- **Integration Tests:** Use `axum-test`.
- **Database Tests:** Use `#[sqlx::test(migrations = "./migrations")]`. It provisions a fresh isolated database per test
  within an invocation-owned disposable cluster. Role helpers require the owner-supplied nonempty passwords.
- **OIDC Mocking:** Use `crate::test_support::oidc_mock` for auth flows.
  </testing_standards>

<tool_standards>

- **Formatting & Linting:** You must respect `cargo fmt` and `cargo clippy`. Do not fight the formatter. Fix warnings,
  do not suppress them with `#[allow(...)]` unless heavily justified.
- **Datetime Crate:** First-party code uses `chrono` (`DateTime<Utc>` for instants, `NaiveDate` for calendar dates). The
  `time` crate appears only where a third-party API requires its types; clippy's `disallowed-types`
  (`backend/clippy.toml`) enforces that, and each permitted site carries a scoped `#[expect]` naming the API that forces
  it. See `docs/adr/0046-chrono-is-the-first-party-datetime-crate.md`.
- **Logging:** Use `tracing` with structured fields. Never use `println!`.
- **Artifact Regen:** Editing a config/ or API-surface doc-comment regenerates artifacts. Run `just rust::regen` and
  commit them in the same PR; drift tests gate CI. </tool_standards>
