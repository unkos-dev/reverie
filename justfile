# Root task graph for Reverie. Aggregates fan out to per-plane module recipes via
# cross-module dependencies; each module file owns its own shell + working dir
# (just settings do not propagate into submodules).
#
# This file is the canonical definition of "lint/format/test/build the repo".
# The CI lint/format jobs invoke these recipes, and the oxlint/stylelint git
# hooks call js::oxlint/js::stylelint, so the CI command definitions live here
# once rather than duplicated inline in the workflow.

set shell := ["bash", "-ueo", "pipefail", "-c"]
set dotenv-load := false

mod js
mod rust
mod infra
mod docs

# List recipes (default target).
_default:
    @just --list

# Verify every plane (locally-runnable gates only; DB/CI recipes and a11y excluded).
[group('aggregate')]
check: js::check rust::check infra::check docs::check

# Docs content is linted whole-tree by infra::prose and js::markdownlint, so the
# docs plane carries no lint recipe of its own.
#
# Lint the js, rust, and infra surfaces.
[group('aggregate')]
lint: js::lint rust::lint infra::lint

# oxfmt (js::fmt) covers all its types whole-tree, incl. docs and backend TOML;
# cargo fmt (rust::fmt) covers Rust. Together they format the whole tree.
#
# Format the whole tree in place. WRITES; never depended on by check/lint.
[group('aggregate')]
fmt: js::fmt rust::fmt

# Backend tests are DB-backed: bring the dev DB up first (`just db-up`).
# Doctests stay out of the aggregate (slow; CI runs them).
#
# Run every locally-runnable unit test.
[group('aggregate')]
test: js::test rust::test

# Build every shippable artifact.
[group('aggregate')]
build: js::build rust::build docs::build

# Roles seed from docker/init-roles.sql on first init only.
#
# Start (or create) the local dev Postgres.
[group('db')]
db-up:
    docker compose -f docker/compose.dev.yml up -d --wait

# Stop the local dev Postgres; the data volume survives.
[group('db')]
db-down:
    docker compose -f docker/compose.dev.yml down

# Destroy and recreate the local dev Postgres, then re-seed roles. DESTRUCTIVE.
[group('db')]
db-reset:
    docker compose -f docker/compose.dev.yml down -v
    docker compose -f docker/compose.dev.yml up -d --wait

# The DSN uses shell parameter expansion (not a just variable) so an
# overridden credential never echoes into logs.
#
# Apply pending migrations with the dedicated migrator identity.
[group('db')]
db-migrate:
    cd backend && DATABASE_URL_MIGRATION="${DATABASE_URL_MIGRATION:-postgres://reverie_migrator:reverie_migrator@localhost:5432/reverie_dev}" cargo run -- migrate
