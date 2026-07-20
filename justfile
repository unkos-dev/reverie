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

# Worktree root. Override with WORKTREE_ROOT to keep checkouts elsewhere; the
# default is a sibling of the repo so it inherits the same filesystem and
# backup rules without ever nesting inside the checkout.
worktree_root := env_var_or_default("WORKTREE_ROOT", parent_directory(justfile_directory()) / "worktrees")

# Worktrees live outside the checkout: nested ones put every other branch
# inside the Docker build context, cargo workspace discovery, and file
# watchers. They must also sit on real storage, because a worktree on a tmpfs
# loses unpushed commits at reboot; this recipe refuses to create one there.
#
# Create a git worktree for BRANCH at $WORKTREE_ROOT/reverie/<slug>.
[group('git')]
worktree branch:
    #!/usr/bin/env bash
    set -ueo pipefail
    slug="$(printf '%s' "{{ branch }}" | tr '/' '-')"
    dest="{{ worktree_root }}/reverie/${slug}"
    mkdir -p "$(dirname "$dest")"
    fstype="$(stat -f -c %T "$(dirname "$dest")")"
    case "$fstype" in
        tmpfs | ramfs)
            echo "refusing to create a worktree on ${fstype}: ${dest}" >&2
            echo "unpushed commits there do not survive a reboot; set WORKTREE_ROOT to a disk-backed path" >&2
            exit 1
            ;;
    esac
    if git show-ref --verify --quiet "refs/heads/{{ branch }}"; then
        git worktree add "$dest" "{{ branch }}"
    else
        git worktree add -b "{{ branch }}" "$dest"
    fi
    # mise keys trust to path, so a fresh worktree is untrusted and the first
    # command run there blocks on an interactive prompt, which a non-interactive
    # session cannot answer. Inherit the decision rather than make it: trust the
    # worktree only when this checkout is already trusted, so the recipe never
    # grants a config more trust than the operator has given it.
    if command -v mise > /dev/null; then
        if mise trust --show 2>/dev/null | grep -q ': trusted'; then
            mise trust "$dest" > /dev/null && echo "mise: inherited trust for $dest"
        else
            echo "mise: this checkout is untrusted, so $dest is too; run 'mise trust' there after reviewing mise.toml" >&2
        fi
    fi
    echo "worktree ready: $dest"

# Remove a worktree by branch name, then prune the administrative state that
# a plain `rm -rf` would strand in .git/worktrees.
#
# Remove the worktree for BRANCH.
[group('git')]
worktree-rm branch:
    #!/usr/bin/env bash
    set -ueo pipefail
    slug="$(printf '%s' "{{ branch }}" | tr '/' '-')"
    git worktree remove "{{ worktree_root }}/reverie/${slug}"
    git worktree prune
    echo "removed worktree for {{ branch }}"

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
