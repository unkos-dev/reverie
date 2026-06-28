# Root task graph for Reverie. Aggregates fan out to per-plane module recipes via
# cross-module dependencies; each module file owns its own shell + working dir
# (just settings do not propagate into submodules).
#
# This file is the canonical definition of "lint/format/test/build the repo".
# Nothing consumes it yet (no hooks or CI wired here); local use only.

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

# Backend tests are DB-backed (no local DB), so they ride CI via rust::test and
# rust::doctests rather than this aggregate.
#
# Run every locally-runnable unit test.
[group('aggregate')]
test: js::test

# Build every shippable artifact.
[group('aggregate')]
build: js::build rust::build docs::build
