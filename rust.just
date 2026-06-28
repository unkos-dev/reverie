# Rust plane: backend format/lint/test/build. All recipes run from backend/.
# Backend tools are system-PATH (image-baked toolchain), so bare invocation is
# correct here (unlike the node-local JS plane). Recipes marked CI/DB need the
# dev Postgres and cannot run in the local workspace (no local DB); they ride CI.

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

# Needs Postgres; not runnable in the local workspace (no local DB) and rides CI.
#
# CI/DB: full test suite under coverage instrumentation.
[group('rust')]
test:
    cargo llvm-cov nextest --workspace --lcov --output-path lcov.info

# CI/DB: doctests across the workspace. Needs Postgres; rides CI.
[group('rust')]
doctests:
    cargo test --doc --workspace

# Needs Postgres; rides CI.
#
# CI/DB: sqlx query-cache freshness against the live schema.
[group('rust')]
sqlx-check:
    cargo sqlx prepare --check -- --tests

# Compile the workspace.
[group('rust')]
build:
    cargo build --workspace --locked

# Backend lint surface: clippy + doc-lint.
[group('rust')]
lint: clippy doc-lint

# DB-backed recipes (test/doctests/sqlx-check) are excluded; they ride CI.
#
# Locally-runnable backend gate: format check, clippy, doc-lint, drift.
[group('rust')]
check: fmt-check clippy doc-lint drift
