# Rust plane: backend format/lint/test/build. All recipes run from backend/.
# Backend tools are system-PATH (image-baked toolchain), so bare invocation is
# correct here (unlike the node-local JS plane). Recipes marked DB need the
# local dev Postgres up (`just db-up`).
#
# DB recipes inject the schema-owner DSN inline via shell parameter expansion,
# never a just variable: just echoes recipe lines after {{var}} interpolation,
# so a just-variable DSN would print a credential-bearing override into
# terminal/CI logs, while `${REVERIE_DEV_DB_URL:-…}` stays unexpanded in the
# echo. The owner DSN is injected explicitly (mirroring the CI job env)
# because sqlx::test needs a superuser connection to mint per-test databases,
# and a developer `.env` carries the reverie_app runtime DSN instead.

set shell := ["bash", "-ueo", "pipefail", "-c"]
set working-directory := 'backend'

# Format the workspace in place. WRITES; never a dependency of check/lint.
[group('rust')]
fmt:
    cargo fmt --all

# Verify workspace formatting without writing (the pre-push gate form).
[group('rust')]
fmt-check:
    cargo fmt --all -- --check

# Clippy across the whole workspace, all targets, warnings denied.
[group('rust')]
clippy:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Reject broken intra-doc links in the public docstrings.
[group('rust')]
doc-lint:
    RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --no-deps --workspace

# Offline (no database), so it runs locally; this is the backend pre-push gate.
#
# Generated-artifact drift guard (OpenAPI spec + config reference).
[group('rust')]
drift:
    SQLX_OFFLINE=true cargo test --locked --test gen_openapi --test gen_config_ref

# DB: full test suite, fast local loop (no coverage; see `cov`).
[group('rust')]
test:
    DATABASE_URL="${REVERIE_DEV_DB_URL:-postgres://reverie:reverie@localhost:5432/reverie_dev}" cargo nextest run --workspace --locked

# DB: full test suite under coverage instrumentation.
[group('rust')]
cov:
    DATABASE_URL="${REVERIE_DEV_DB_URL:-postgres://reverie:reverie@localhost:5432/reverie_dev}" cargo llvm-cov nextest --workspace --lcov --output-path lcov.info

# DB: doctests across the workspace.
[group('rust')]
doctests:
    DATABASE_URL="${REVERIE_DEV_DB_URL:-postgres://reverie:reverie@localhost:5432/reverie_dev}" cargo test --doc --workspace

# DB: sqlx query-cache freshness against the live schema.
[group('rust')]
sqlx-check:
    DATABASE_URL="${REVERIE_DEV_DB_URL:-postgres://reverie:reverie@localhost:5432/reverie_dev}" cargo sqlx prepare --check -- --tests

# Compile the workspace.
[group('rust')]
build:
    cargo build --workspace --locked

# Backend lint surface: clippy + doc-lint.
[group('rust')]
lint: clippy doc-lint

# DB-backed recipes (test/doctests/sqlx-check) are excluded from the offline
# gate; run them with the dev DB up (`just db-up`).
#
# Locally-runnable backend gate: format check, clippy, doc-lint, drift.
[group('rust')]
check: fmt-check clippy doc-lint drift
