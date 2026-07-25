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
# Generated-artifact drift guard (OpenAPI spec + config reference + config schema).
[group('rust')]
drift:
    SQLX_OFFLINE=true cargo test --locked --test gen_openapi --test gen_config_ref --test gen_config_schema

# Extra args pass through to nextest as filters, so the red-green loop can
# run just the tests under work instead of the whole suite: e.g.
# `just rust::test patch_identifier` (name substring) or
# `just rust::test -E 'test(rls)'` (filterset). No args = full suite.
#
# DB: full test suite, fast local loop (no coverage; see `cov`).
[group('rust')]
test *args:
    DATABASE_URL="${REVERIE_DEV_DB_URL:-postgres://reverie:reverie@localhost:5432/reverie_dev}" cargo nextest run --workspace --locked {{ args }}

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

# WRITES backend/.sqlx; commit the result in the same change as the queries
# that produced it. `sqlx-check` is the read-only gate form.
#
# DB: regenerate the sqlx offline query cache from the live schema.
[group('rust')]
sqlx-prepare:
    DATABASE_URL="${REVERIE_DEV_DB_URL:-postgres://reverie:reverie@localhost:5432/reverie_dev}" cargo sqlx prepare -- --tests

# WRITES docs/openapi.json plus the generated config reference and schema;
# commit the result in the same change that moved the API or config surface.
# `drift` is the read-only gate form. Offline: compiles against the sqlx
# cache, so run `sqlx-prepare` first when queries changed.
#
# Regenerate the OpenAPI spec, config reference, and config schema in place.
[group('rust')]
regen:
    REGEN=1 SQLX_OFFLINE=true cargo test --locked --test gen_openapi --test gen_config_ref --test gen_config_schema

# Compile the workspace.
[group('rust')]
build:
    cargo build --workspace --locked

# Offline (no database, no toolchain needed). Mirrors ci.yml's backend-checks
# static guards: runtime sqlx::query/raw_sql, raw axum .route( registrations,
# chrono imports, and em dashes in the generated config schema, each checked
# against its .github/*-allowlist.txt (or, for the em-dash guard, none).
#
# Backend static allowlist and prose guards.
[group('rust')]
guards:
    ../scripts/backend-guards.sh

# Offline (no database). Mirrors the CI "unused dependencies" job.
#
# Fail on unused workspace dependencies.
[group('rust')]
machete:
    cargo machete

# Reads the RustSec advisory database over the network, so it is offline-excluded
# from `check`. Mirrors the CI "cargo-deny" job (config: backend/deny.toml).
#
# Check advisories, license, bans, and source policy.
[group('rust')]
deny:
    cargo deny check

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
    #!/usr/bin/env bash
    set -ueo pipefail
    source ../scripts/backend-dev-env.sh
    exec cargo run

# Shared lifecycle plumbing for dev-start/dev-stop/dev-status. The script owns
# the ownership and race checks (see scripts/dev-server.sh); this sets the
# backend knobs: the resolved port for both the server and the readiness probe,
# a leader pattern anchored on the cargo or server binary itself (the script's
# same-directory check then excludes other checkouts' cargo processes), start
# ticks sized for a cold `cargo run` compile rather than bare process startup,
# and advice strings that name the rust recipes.
#
# The dev configuration the server needs comes from scripts/backend-dev-env.sh,
# which defers to `.env` instead of overriding it. A shebang body keeps the
# dev-default DSN out of just's per-line echo, which shebang recipes do not get.
_dev-lifecycle action:
    #!/usr/bin/env bash
    set -ueo pipefail
    source ../scripts/backend-dev-env.sh
    DEV_SERVER_DIR=. \
    DEV_SERVER_PORT="$REVERIE_PORT" \
    DEV_SERVER_CMD="cargo run" \
    DEV_SERVER_LEADER_PATTERN='(^|/)(cargo|reverie-api)$' \
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
