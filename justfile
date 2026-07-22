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

# `just --list` collapses each module to a one-line `js ...` entry, which
# hides the per-plane recipes; this expands them.
#
# List every recipe, including the ones inside modules.
help:
    @just --list --list-submodules

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

# CI-parity local gate: everything the GitHub CI gate runs that is runnable on
# a workstation. rust::guards runs first: it needs no toolchain, database, or
# install, so it fails fastest (mirroring where ci.yml places its own static
# guards). db-up brings the dev database up by construction (idempotent,
# --wait), so every DB-backed recipe below it in this dependency list always
# runs against a ready database; just runs dependencies serially in listed
# order, so the cheap offline gates (rust::guards, check) fail fast before the
# slower DB-backed and network-backed ones run. Still CI-only, and not run
# here: the MSRV toolchain check, coverage lanes, the docker image build,
# workflow/IaC/SAST/secret scans, npm-license, and dependency-review.
#
# Run everything CI runs that is locally runnable, DB-backed tests included.
[group('aggregate')]
preflight: rust::guards db-up check test rust::doctests rust::sqlx-check rust::machete rust::deny js::build js::font-integrity js::a11y

# Worktree root. Override with WORKTREE_ROOT to keep checkouts elsewhere; the
# default is a sibling of the repo so it inherits the same filesystem and
# backup rules without ever nesting inside the checkout.
worktree_root := env_var_or_default("WORKTREE_ROOT", parent_directory(justfile_directory()) / "worktrees")

# Worktrees live outside the checkout: nested ones put every other branch
# inside the Docker build context, cargo workspace discovery, and file
# watchers. They must also sit on real storage, because a worktree on a tmpfs
# loses unpushed commits at reboot; this recipe refuses to create one there.
#
# The branch arrives through "$@" rather than a {{ }} substitution: just
# expands substitutions into the script text before bash parses it, and
# double quotes do not stop command substitution, so a branch name
# containing $() would execute. Git permits such names.
#
# The worktree also gets its own `.cargo/config.toml` pinning `target-dir` to
# its local `target/`, so concurrent worktree builds never share (and thrash)
# a machine-level target-dir override. Cargo gives an exported
# CARGO_TARGET_DIR or CARGO_BUILD_TARGET_DIR precedence over that config key
# (CARGO_TARGET_DIR wins when both are set), so this recipe warns rather
# than silently unsetting a variable in the caller's environment.
#
# Create a git worktree for BRANCH at `$WORKTREE_ROOT/reverie/<slug>`, with an isolated cargo target dir; warns if CARGO_TARGET_DIR or CARGO_BUILD_TARGET_DIR would override it.
[group('git')]
[positional-arguments]
worktree branch:
    #!/usr/bin/env bash
    set -ueo pipefail
    branch="$1"
    slug="$(printf '%s' "$branch" | tr '/' '-')"
    dest={{ quote(worktree_root) }}"/reverie/${slug}"
    parent="$(dirname "$dest")"
    mkdir -p "$parent"
    # `stat -f -c` is GNU-only; BSD stat (macOS) rejects it. Report the gap
    # rather than defaulting to a value that would silently pass the guard,
    # so a skipped check never looks like a passed one.
    if fstype="$(stat -f -c %T "$parent" 2>/dev/null)"; then
        case "$fstype" in
            tmpfs | ramfs)
                echo "refusing to create a worktree on ${fstype}: ${dest}" >&2
                echo "unpushed commits there do not survive a reboot; set WORKTREE_ROOT to a disk-backed path" >&2
                exit 1
                ;;
        esac
    else
        echo "warning: cannot read the filesystem type of ${parent} (non-GNU stat); the tmpfs guard did not run" >&2
    fi
    # Prefer an existing local branch, then a remote-tracking one. Without
    # the second case a branch that exists only on the remote would be
    # recreated from the current HEAD, putting the worktree on unrelated
    # history under a familiar name.
    if git show-ref --verify --quiet "refs/heads/${branch}"; then
        git worktree add "$dest" "$branch"
    elif git show-ref --verify --quiet "refs/remotes/origin/${branch}"; then
        git worktree add --track -b "$branch" "$dest" "origin/${branch}"
    else
        git worktree add -b "$branch" "$dest"
    fi
    # A user-level `[build] target-dir` override (a known cargo pattern for
    # warm cross-checkout builds) makes concurrent worktree builds thrash a
    # shared target dir: each branch's rebuild invalidates the other's freshly
    # built binaries. Cargo resolves config by nearest-file-wins, so a
    # worktree-local override pins this worktree to its own `target/`
    # regardless of what the user level sets. On a machine with no such
    # override this restates cargo's own default, so it is a no-op there.
    # `target/` lives inside the worktree, so `git worktree remove` deletes it
    # with no separate cleanup step.
    mkdir -p "$dest/.cargo"
    printf '%s\n' '[build]' 'target-dir = "target"' > "$dest/.cargo/config.toml"
    # A config file cannot outrank the environment: cargo resolves both
    # CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR (its generic
    # CARGO_<SECTION>_<KEY> mapping for [build] target-dir) before it ever
    # reads the config key, so either one, exported by a developer's shell
    # profile or an inherited CI env, silently defeats the isolation just
    # written above while this recipe still reports success. Verified
    # empirically: with both set, CARGO_TARGET_DIR is the one cargo actually
    # honors, so that is the variable named as active below. Detect and warn
    # rather than unsetting the variable ourselves, since surprising the
    # caller's environment is worse than naming the fix. The warning never
    # echoes the variable's value: an unsanitized value could contain
    # newlines or control sequences and forge or obscure other log lines.
    if [ -n "${CARGO_TARGET_DIR:-}" ] && [ -n "${CARGO_BUILD_TARGET_DIR:-}" ]; then
        echo "warning: CARGO_TARGET_DIR is set and overrides the isolated target-dir just written to $dest/.cargo/config.toml (CARGO_BUILD_TARGET_DIR is also set but is shadowed by CARGO_TARGET_DIR)" >&2
        echo "fix: unset both CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR, or set both to $dest/target for work in this worktree" >&2
    elif [ -n "${CARGO_TARGET_DIR:-}" ]; then
        echo "warning: CARGO_TARGET_DIR is set and overrides the isolated target-dir just written to $dest/.cargo/config.toml" >&2
        echo "fix: unset CARGO_TARGET_DIR, or set it to $dest/target for work in this worktree" >&2
    elif [ -n "${CARGO_BUILD_TARGET_DIR:-}" ]; then
        echo "warning: CARGO_BUILD_TARGET_DIR is set and overrides the isolated target-dir just written to $dest/.cargo/config.toml" >&2
        echo "fix: unset CARGO_BUILD_TARGET_DIR, or set it to $dest/target for work in this worktree" >&2
    fi
    # mise keys trust to path, so a fresh worktree is untrusted and the first
    # command run there blocks on an interactive prompt, which a non-interactive
    # session cannot answer. Inherit the decision rather than make it: trust the
    # worktree only when this checkout is already trusted, so the recipe never
    # grants a config more trust than the operator has given it.
    if command -v mise > /dev/null; then
        if mise trust --show 2>/dev/null | grep -q ': trusted'; then
            # Not `&&`: a failing left operand of && is exempt from set -e,
            # so the recipe would report success while leaving the worktree
            # untrusted, reintroducing the prompt this exists to prevent.
            if ! mise trust "$dest" > /dev/null; then
                echo "mise: failed to inherit trust for $dest" >&2
                exit 1
            fi
            echo "mise: inherited trust for $dest"
        else
            echo "mise: this checkout is untrusted, so $dest is too; run 'mise trust' there after reviewing mise.toml" >&2
        fi
    fi
    echo "worktree ready: $dest"

# Remove a worktree by branch name, then prune the administrative state that
# a plain `rm -rf` would strand in .git/worktrees. Same "$@" handling as
# `worktree` above, for the same injection reason.
#
# Remove the worktree for BRANCH.
[group('git')]
[positional-arguments]
worktree-rm branch:
    #!/usr/bin/env bash
    set -ueo pipefail
    branch="$1"
    slug="$(printf '%s' "$branch" | tr '/' '-')"
    git worktree remove {{ quote(worktree_root) }}"/reverie/${slug}"
    git worktree prune
    echo "removed worktree for ${branch}"

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

# Idempotent by construction: db-up is a no-op when the container is already
# healthy, db-migrate is a no-op once the schema is current, and each
# dev-start exits 0 when it already owns a serving process, so re-running
# while the stack is up is safe. Dependencies run in order: db-up's --wait
# means migrations never race a database that is still coming up, and the
# backend starts only against a migrated schema.
#
# Start the whole dev stack in the background: dev Postgres, migrations, backend API, Vite.
[group('dev')]
dev-up: db-up db-migrate rust::dev-start js::dev-start

# The database stays up because it is cheap, stateful, and shared with the
# test suite; stop it explicitly with db-down. Not dependency-driven: a
# failing frontend stop must not strand the backend, so both stops always
# run and the recipe fails if either failed.
#
# Stop the background dev servers (frontend, then backend).
[group('dev')]
dev-down:
    #!/usr/bin/env bash
    set -uo pipefail
    rc=0
    just js::dev-stop || rc=1
    just rust::dev-stop || rc=1
    exit "$rc"

# Not dependency-driven: a dependency chain stops at the first failing status,
# and a probe must report both planes even when the first one is down.
#
# Report both dev servers' status; exits nonzero when either is down.
[group('dev')]
dev-status:
    #!/usr/bin/env bash
    set -uo pipefail
    rc=0
    just rust::dev-status || rc=1
    just js::dev-status || rc=1
    exit "$rc"

# Read-only; no writes, no network beyond the local docker daemon. Answers
# "is this machine ready to develop Reverie?" in seconds so a degraded
# environment (missing tool pin, stale dev DB, absent node_modules) surfaces
# immediately instead of as a confusing downstream failure.
#
# Check the local dev environment: tools, mise pins, docker, dev DB, deps.
[group('dev')]
doctor:
    scripts/doctor.sh
