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

# DB: needs the local dev Postgres up (`just db-up`).
#
# Run the backend API server in the foreground (Ctrl-C stops it).
[group('rust')]
dev:
    DATABASE_URL="${REVERIE_DEV_APP_DB_URL:-postgres://reverie_app:reverie_app@localhost:5432/reverie_dev}" \
    REVERIE_PUBLIC_URL="${REVERIE_PUBLIC_URL:-http://localhost:5173}" \
    cargo run

# Shared lifecycle plumbing for dev-start/dev-stop/dev-status. The script owns
# the ownership and race checks (see scripts/dev-server.sh); this sets the
# backend knobs: port 3000 (REVERIE_PORT overrides both the server and the
# probe together, and the leader pattern anchors on the cargo or server
# binary itself, with the script's same-directory check excluding other
# checkouts' cargo processes), the RLS-enforced reverie_app runtime DSN (dev-default
# password; REVERIE_DEV_APP_DB_URL overrides, expanded in-shell so an override
# never echoes), a leader pattern matching cargo and the built binary, start
# ticks sized for a cold `cargo run` compile rather than bare process startup,
# and advice strings that name the rust recipes.
# REVERIE_PUBLIC_URL is required whenever OPDS is enabled (the default); the
# dev default is the Vite origin, which proxies /opds to this server.
_dev-lifecycle action:
    DATABASE_URL="${REVERIE_DEV_APP_DB_URL:-postgres://reverie_app:reverie_app@localhost:5432/reverie_dev}" \
    REVERIE_PUBLIC_URL="${REVERIE_PUBLIC_URL:-http://localhost:5173}" \
    DEV_SERVER_DIR=. \
    DEV_SERVER_PORT="${REVERIE_PORT:-3000}" \
    DEV_SERVER_CMD="cargo run" \
    DEV_SERVER_LEADER_PATTERN="(^|/)(cargo|reverie-api)$" \
    DEV_SERVER_START_TICKS=600 \
    DEV_SERVER_STOP_HINT="just rust::dev-stop" \
    DEV_SERVER_FG_HINT="just rust::dev" \
    ../scripts/dev-server.sh '{{ action }}'

# DB: needs the local dev Postgres up (`just db-up`); without it the server
# exits during startup and the failure is reported from the log.
#
# Start the backend API server in the background; logs to backend/.dev-server.log.
[group('rust')]
dev-start: (_dev-lifecycle "start")

# Stop the background backend API server started by dev-start.
[group('rust')]
dev-stop: (_dev-lifecycle "stop")

# Exits nonzero when down, so the recipe works as a scriptable probe.
#
# Report whether the backend API server is up.
[group('rust')]
dev-status: (_dev-lifecycle "status")
