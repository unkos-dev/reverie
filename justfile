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

# One install for the whole tree; there is no per-plane equivalent, because the
# workspace members are declared centrally in pnpm-workspace.yaml.
# --frozen-lockfile rather than a plain install: it is lockfile-exact, it fails
# rather than silently updating the lockfile, and it works from nothing.
#
# Install the pnpm workspace (frontend + docs) from the root lockfile.
[group('aggregate')]
install:
    pnpm install --frozen-lockfile

# infra::zizmor-offline rides along here rather than inside infra::check
# because it audits a different CI job (workflow-security) than the one
# infra::lint mirrors (repo-lint); it stays additive so that doc comment
# keeps describing exactly one job. It only ever runs zizmor's offline
# audits, so it cannot regress this recipe's offline guarantee; run
# `just preflight` (or `just infra::zizmor`) for the network-backed audits
# it skips.
#
# Verify every plane (locally-runnable gates only; DB/CI recipes and a11y excluded).
[group('aggregate')]
check: js::check rust::check infra::check docs::check infra::zizmor-offline

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
# slower DB-backed and network-backed ones run. infra::zizmor runs the full
# audit (online audits included when a GH_TOKEN/GITHUB_TOKEN/
# ZIZMOR_GITHUB_TOKEN is exported, offline-degraded otherwise, matching CI's
# own token-driven coverage). Still CI-only, and not run here: the MSRV
# toolchain check, coverage lanes, the docker image build, IaC/SAST/secret
# scans, npm-license, dependency-review, and the accessibility scan.
#
# Two recipes are locally runnable and excluded anyway, because a laptop cannot
# answer for either honestly. The accessibility scan reuses an already-running
# dev server with no ownership check, so from any checkout that does not own
# port 5173 it scans a different tree and reports the result as this branch's.
# infra::selftests covers this repository's local developer tooling (doctor,
# worktree, the dev-server lifecycle, the detached gate), which CI has no stake
# in, and gates nothing anywhere. Run either by hand when working on what it
# covers.
#
# The lanes are a list passed to scripts/gate-run.sh rather than just
# dependencies, so the run ends with one `GATE: PASS`/`GATE: FAIL` line naming
# the lane that failed. A dependency list cannot do that: a failing dependency
# stops the chain before any recipe body could report on it, which leaves the
# last line of a captured run saying nothing about the run. See gate-run.sh for
# why that matters and what it costs.
#
# `just preflight` is the default, scoped gate below; run this unconditional
# form before any push (unless a scoped run already escalated to it), when
# the change is broad, or when you are unsure.
#
# Run everything CI runs that is locally runnable, DB-backed tests included, unconditionally.
[group('aggregate')]
preflight-full:
    #!/usr/bin/env bash
    set -ueo pipefail
    scripts/gate-run.sh preflight-full \
        rust::guards db-up check rust::doc-lint test rust::doctests \
        rust::sqlx-check rust::machete rust::deny js::build \
        js::font-integrity infra::zizmor

# The same gate as `preflight-full`, minus the lanes CI itself would skip. A
# frontend-only branch pays for the frontend lanes and the unconditional
# repo-lint mirror, not a database, a Rust rebuild, and a dependency audit.
# This is the default gate and the mid-branch reflex.
#
# The skip decisions come from .github/path-filters.yml, the same file CI's
# `changes` job feeds to dorny/paths-filter, so widening a filter for CI
# widens this gate with it. Changes to the verification machinery itself
# (justfiles, scripts/, tool pins, that filter file) escalate to the full lane
# set, because a scoped run cannot reason about a change to its own rules.
#
# `just preflight-full` stays the unconditional answer: run it when the
# change is broad or you are unsure, though a scoped run's own escalation
# reaches the same lanes automatically when it applies. Args pass through to
# scripts/preflight-scope.sh (`--base <ref>` to compare against something
# other than origin/main).
#
# Run only the preflight lanes this branch's changed paths require (the default gate).
[group('aggregate')]
[positional-arguments]
preflight *args:
    #!/usr/bin/env bash
    set -ueo pipefail
    # Command substitution, not `mapfile < <(...)`: a process substitution's
    # exit status is unobservable, so a scoper that died would read as an
    # empty lane list and this recipe would report a green "nothing to do".
    scope="$(scripts/preflight-scope.sh --explain "$@")"
    if [ -z "$scope" ]; then
        # Still a gate run, so it still gets a verdict: a no-op that printed
        # only prose would be the one outcome a caller could not read back.
        echo "preflight: no lane required for the changed paths"
        exec scripts/gate-run.sh preflight
    fi
    mapfile -t lanes <<< "$scope"
    echo "preflight: ${lanes[*]}"
    # Safe to pass the whole array here only because gate-run.sh runs one lane
    # per `just` invocation. It must never reach `just` on a single command
    # line: a lane that takes parameters, such as `rust::test *args`, swallows
    # every following name as its own argument, so the lanes after it silently
    # never run and the gate can still exit 0.
    exec scripts/gate-run.sh preflight "${lanes[@]}"

# Detaches from the invoking terminal (setsid) rather than relying on a
# hand-rolled setsid-plus-log-file pipeline, which is what this replaces:
# scripts/gate-detach.sh owns the mechanics so nothing has to hand-roll one
# again. The log lands under the same $XDG_STATE_HOME/reverie/gate/ area the
# lane records use, keyed per checkout the same way, so `just gate-status`
# reads the finished run back regardless of which mode produced it. MODE
# selects which gate runs detached: "scoped" (default) is `just preflight`,
# and its *args pass through to scripts/preflight-scope.sh; "full" is `just
# preflight-full`, which takes no arguments of its own.
#
# Run the scoped (default) or full preflight gate detached from the terminal; see `just gate-status` for the verdict.
[group('aggregate')]
[positional-arguments]
preflight-detach mode="scoped" *args:
    #!/usr/bin/env bash
    set -ueo pipefail
    case "$1" in
        scoped)
            target=preflight
            ;;
        full)
            if [ "$#" -gt 1 ]; then
                echo "preflight-detach: full mode takes no extra arguments" >&2
                exit 1
            fi
            target=preflight-full
            ;;
        *)
            echo "preflight-detach: unknown mode '$1' (expected 'scoped' or 'full')" >&2
            exit 1
            ;;
    esac
    shift
    exec scripts/gate-detach.sh "$target" "$@"

# The record lives under $XDG_STATE_HOME, keyed per checkout: it is machine
# state rather than repository content, and two worktrees running a gate at
# once must not write over each other. Every outcome a caller must not confuse
# gets its own exit status: 1 the last run failed, 2 it died unfinished, 3 it
# is still in progress, 4 there is no recorded run at all. A warning also
# names a checkout that has moved past the recorded commit or picked up
# uncommitted changes the run never saw, so an old green cannot quietly stand
# in for the current tree.
#
# Report the last recorded preflight run: per-lane timings and the verdict.
[group('aggregate')]
gate-status:
    scripts/gate-run.sh --status

# Worktree root. Override with WORKTREE_ROOT to keep checkouts elsewhere; the
# default is a sibling of the repo so it inherits the same filesystem and
# backup rules without ever nesting inside the checkout.
worktree_root := env_var_or_default("WORKTREE_ROOT", parent_directory(justfile_directory()) / "worktrees")

# Worktrees live outside the checkout: nested ones put every other branch
# inside the Docker build context, cargo workspace discovery, and file
# watchers. They must also sit on real storage, because a worktree on a tmpfs
# loses unpushed commits at reboot; this recipe refuses to create one there.
#
# The branch (and optional base) arrive through "$@" rather than a {{ }}
# substitution: just expands substitutions into the script text before bash
# parses it, and double quotes do not stop command substitution, so a branch
# name containing $() would execute. Git permits such names.
#
# BRANCH resolution, in order: an existing local branch of that name is
# reused as-is; failing that, an existing origin/<branch> is checked out as a
# new tracking branch; failing that, a brand-new branch is created and its
# start point comes from scripts/worktree-base.sh, which prefers origin/main,
# then falls back to a local main, and fails when neither exists rather than
# guessing: the only remaining candidate is the invoking checkout's current
# HEAD, and basing a new branch on that silently carries the checked-out
# branch's own commits into every new worktree (pass HEAD as BASE to opt in
# deliberately). BASE, when given, is an explicit start point that
# overrides that whole chain for the brand-new-branch case; it is validated
# and the recipe fails clearly if it does not resolve. BASE has no effect on
# the first two cases, since an existing branch (local or remote-tracking)
# already has its own history to build on.
#
# The worktree also gets its own `.cargo/config.toml` pinning `target-dir` to
# its local `target/`, so concurrent worktree builds never share (and thrash)
# a machine-level target-dir override. Cargo gives an exported
# CARGO_TARGET_DIR or CARGO_BUILD_TARGET_DIR precedence over that config key
# (CARGO_TARGET_DIR wins when both are set), so this recipe warns rather
# than silently unsetting a variable in the caller's environment.
#
# Create a git worktree for BRANCH at `$WORKTREE_ROOT/reverie/<slug>`, with an isolated cargo target dir, the node dependencies installed under the npm the destination declares (so this recipe needs network access), and, when present, Claude and active Codex policy overlays carried over; a new branch bases on origin/main (falling back to local main, failing when neither exists) unless BASE is given explicitly; warns if Cargo environment overrides defeat isolation.
[group('git')]
[positional-arguments]
worktree branch base="":
    #!/usr/bin/env bash
    set -ueo pipefail
    branch="$1"
    base="${2:-}"
    slug="$(printf '%s' "$branch" | tr '/' '-')"
    dest={{ quote(worktree_root) }}"/reverie/${slug}"
    parent="$(dirname "$dest")"
    mkdir -p "$parent"
    # scripts/require-disk-backed.sh centralizes the tmpfs/ramfs filesystem
    # check this recipe and `just doctor`'s low-disk warning both need, so
    # the underlying `stat -f -c` invocation (GNU-only; BSD/macOS rejects it)
    # and its warn-but-continue fallback live in one place.
    if ! scripts/require-disk-backed.sh "$parent"; then
        echo "refusing to create a worktree there: ${dest}" >&2
        echo "unpushed commits there do not survive a reboot; set WORKTREE_ROOT to a disk-backed path" >&2
        exit 1
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
        # A genuinely new branch needs an explicit start point: left to its
        # own default, `git worktree add -b` bases it on the CALLER's current
        # HEAD, so invoking this recipe from a feature branch silently
        # carried that branch's commits into the new worktree. Delegate the
        # choice to scripts/worktree-base.sh so the resolution chain (and its
        # selftest) live in one place.
        base_resolution="$(scripts/worktree-base.sh "$branch" "$base")"
        base_mode="${base_resolution%% *}"
        base_ref="${base_resolution#* }"
        echo "worktree base: ${base_mode} (${base_ref})"
        git worktree add -b "$branch" "$dest" "$base_ref"
    fi
    # Trust is keyed to Codex's canonical absolute project path, so use the
    # physical paths Codex will resolve instead of preserving a symlinked
    # spelling from the invoking shell.
    source_root="$(cd "$(git rev-parse --show-toplevel)" && pwd -P)"
    dest="$(cd "$dest" && pwd -P)"
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
    # Worktrees inherit only tracked files, but per-checkout tool behavior
    # can live in the ignored `.claude/settings.local.json` overlay (for
    # example, sandbox scoping for this repo's recipes). Without the copy
    # that configuration silently does not apply in the new checkout, and
    # the resulting failures look environmental rather than caused by the
    # missing overlay.
    if [ -f .claude/settings.local.json ]; then
        mkdir -p "$dest/.claude"
        cp .claude/settings.local.json "$dest/.claude/settings.local.json"
        echo "copied .claude/settings.local.json into the worktree"
    fi
    # Codex project policy is operator-owned and ignored for the same reason,
    # but `.codex` can also contain runtime and account data that must not
    # spread across checkouts.
    # THREAT: Keep this allowlist narrow so sessions, caches, credentials,
    # state, and other unrelated `.codex` content never cross this boundary.
    if [ -f .codex/config.toml ]; then
        mkdir -p "$dest/.codex"
        cp .codex/config.toml "$dest/.codex/config.toml"
        echo "copied .codex/config.toml into the worktree"
    fi
    if [ -d .codex/rules ]; then
        mkdir -p "$dest/.codex"
        cp -R .codex/rules "$dest/.codex/rules"
        echo "copied .codex/rules into the worktree"
    fi
    # Codex keys project trust to the checkout's exact absolute path and skips
    # project-local policy for untrusted paths. Inherit only an existing trust
    # decision from this checkout. An explicit destination decision remains
    # authoritative, and an unrecognized config shape fails closed.
    if [ -f .codex/config.toml ] || [ -d .codex/rules ]; then
        codex_home="${CODEX_HOME:-${HOME}/.codex}"
        codex_user_config="${codex_home}/config.toml"
        codex_toml_escape() {
            local value="$1"
            value="${value//\\/\\\\}"
            value="${value//\"/\\\"}"
            printf '%s' "$value"
        }
        codex_trust_level() {
            local config="$1"
            local project="$2"
            local header
            if [ ! -f "$config" ]; then
                printf '%s\n' absent
                return
            fi
            header="[projects.\"$(codex_toml_escape "$project")\"]"
            awk -v wanted_header="$header" '
                function trim(value) {
                    sub(/^[[:space:]]+/, "", value)
                    sub(/[[:space:]]+$/, "", value)
                    return value
                }
                {
                    line = $0
                    sub(/[[:space:]]+#.*$/, "", line)
                    line = trim(line)
                    if (line == wanted_header) {
                        if (found) duplicate = 1
                        found = 1
                        in_project = 1
                        next
                    }
                    if (in_project && line ~ /^\[/) {
                        in_project = 0
                    }
                    if (in_project && line ~ /^trust_level[[:space:]]*=/) {
                        if (saw_value) duplicate = 1
                        saw_value = 1
                        sub(/^[^=]*=[[:space:]]*/, "", line)
                        sub(/[[:space:]]+#.*$/, "", line)
                        value = trim(line)
                    }
                }
                END {
                    if (duplicate || (found && (!saw_value || (value != "\"trusted\"" && value != "\"untrusted\"")))) {
                        print "unknown"
                    } else if (value == "\"trusted\"") {
                        print "trusted"
                    } else if (value == "\"untrusted\"") {
                        print "untrusted"
                    } else {
                        print "absent"
                    }
                }
            ' "$config"
        }

        case "${source_root}${dest}" in
            *$'\n'* | *$'\r'* | *$'\t'*)
                echo "Codex: cannot safely persist trust for a path containing control characters; copied policy in $dest/.codex is inactive" >&2
                ;;
            *)
                source_trust="$(codex_trust_level "$codex_user_config" "$source_root")"
                dest_trust="$(codex_trust_level "$codex_user_config" "$dest")"
                if [ "$source_trust" = trusted ]; then
                    case "$dest_trust" in
                        trusted)
                            echo "Codex: $dest is already trusted"
                            ;;
                        absent)
                            dest_key="$(codex_toml_escape "$dest")"
                            if ! (
                                umask 077
                                printf '\n[projects."%s"]\ntrust_level = "trusted"\n' "$dest_key" >> "$codex_user_config"
                            ); then
                                echo "Codex: failed to inherit trust for $dest" >&2
                                exit 1
                            fi
                            echo "Codex: inherited trust for $dest"
                            ;;
                        *)
                            echo "Codex: $dest has an explicit or unrecognized trust setting; copied policy in $dest/.codex is inactive" >&2
                            echo "fix: review the copied policy, then trust this worktree from Codex" >&2
                            ;;
                    esac
                else
                    echo "Codex: this checkout is untrusted, so copied policy in $dest/.codex is inactive" >&2
                    echo "fix: review the copied policy, then trust this worktree from Codex" >&2
                fi
                ;;
        esac
    fi
    # Node dependencies, and the one step in this recipe that touches the
    # network. Without them the worktree is a checkout where every JS lane
    # fails on a third-party message naming neither the condition nor the fix,
    # so the recipe pays for the install rather than leaving it to be found.
    #
    # --ignore-scripts, because the root `prepare` script runs `lefthook
    # install` and a worktree already shares $GIT_COMMON_DIR/hooks: it can
    # provision nothing here, and only rewrites which absolute lefthook path
    # those shared hooks bake in, repository-wide and last-writer-wins, on
    # every worktree creation. The generated hook probes that path before its
    # per-checkout fallback, so a stale one heals itself; the cost while this
    # worktree lives is that every other checkout runs its binary. Leaving
    # install scripts unrun in a fresh checkout also matches
    # adr/2026-08-03-package-ingress-default-deny.md.
    #
    # The install runs under the pnpm the destination declares, not the one this
    # checkout has on PATH, because a branch may pin a different version. The
    # version has to be explicit: a branch old enough to declare a different
    # pnpm also predates the `pnpm` entry in mise.toml, so a bare `mise exec`
    # there resolves nothing.
    pnpm_pin=""
    if [ -f "$dest/package.json" ] && command -v jq > /dev/null; then
        pnpm_pin="$(jq -r '.packageManager // empty' "$dest/package.json" 2> /dev/null | sed -n 's/^pnpm@//p' || true)"
    fi
    if [ -n "$pnpm_pin" ] && command -v mise > /dev/null; then
        echo "installing node dependencies in $dest (pnpm ${pnpm_pin} via mise)"
        install_cmd=(mise exec "pnpm@${pnpm_pin}" -- pnpm install --frozen-lockfile --ignore-scripts)
    else
        echo "installing node dependencies in $dest (pnpm install)"
        install_cmd=(pnpm install --frozen-lockfile --ignore-scripts)
    fi
    # Not `&&`, for the reason given above the mise trust call: the recipe
    # would report a ready worktree that no JS lane can run in.
    if ! (cd "$dest" && "${install_cmd[@]}"); then
        echo "failed to install node dependencies in $dest" >&2
        echo "the branch and the worktree are correct; run 'just install' inside it to finish" >&2
        exit 1
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

# Roles seed from docker/init-roles.sql on first init only. Probe-first:
# when the database already answers over its unix socket the recipe exits
# without touching the docker CLI, so gate runs inside a network-isolated
# sandbox (which blocks the docker socket but not AF_UNIX connects) treat
# an already-running stack as up instead of failing on docker. Only a
# stack that is genuinely down falls through to compose, which needs an
# unsandboxed run.
#
# Start (or create) the local dev Postgres.
[group('db')]
db-up:
    #!/usr/bin/env bash
    set -ueo pipefail
    if scripts/db-ready.sh; then exit 0; fi
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
    cd backend && DATABASE_URL_MIGRATION="${DATABASE_URL_MIGRATION:-postgres:///reverie_dev?host=${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock&user=reverie_migrator&password=reverie_migrator}" cargo run --locked -- migrate

# Is: a development-loop unblocker for the compile/cache/migration cycle
# when a branch is authoring a new migration. `db-migrate` compiles the
# backend binary first, but the binary cannot compile until the sqlx
# offline cache reflects the new migration, and the cache cannot
# regenerate until the migration has been applied to the dev DB. This
# recipe breaks that cycle by applying the SQL files in
# backend/migrations/ straight through sqlx-cli, no compile involved.
# After it runs, `just rust::sqlx-prepare` regenerates the offline cache
# against the now-migrated schema so the binary builds again.
#
# Is not: the deployment path. Real instances still migrate through the
# application binary's `migrate` command (what `db-migrate` runs), which
# performs runtime checks sqlx-cli does not, and which applies all
# pending transactional migrations inside one batch transaction where
# sqlx-cli commits each migration individually. A migration that needs
# an earlier migration's commit (the classic case: using an enum value a
# previous migration just added) passes here and fails under the shipped
# runner on a fresh database. Before pushing a branch that adds a
# migration, run `just db-reset && just db-migrate` once so the shipped
# runner has applied it from scratch; nothing else in the local loop or
# preflight exercises that runner.
#
# Is not: a cache regenerator. Run `just rust::sqlx-prepare` afterward;
# this recipe only touches the database.
#
# Takes optional passthrough args, e.g. `--ignore-missing` for a shared
# dev DB that already carries a sibling worktree's migration. Not on by
# default: it weakens sqlx-cli's check that applied migrations match the
# local migration files, and that check should stay strict unless a
# developer knowingly needs to relax it. Even after it succeeds, the
# sibling's applied migration is still unknown to this branch's binary,
# so the application's schema-ahead check keeps rejecting the database:
# `db-migrate` and backend startup both fail until the branch gains the
# sibling's migration file or the database is rebuilt with
# `just db-reset` (destructive; discards the shared DB's data).
#
# Same migrator DSN default as db-migrate, as a deliberate copy that
# nothing in just enforces; scripts/recipe-secret-echo-test.sh asserts
# the two stay byte-identical, so change both together. Duplicated
# rather than lifted into a just variable for the same reason db-migrate
# inlines it: a just variable would echo an overridden credential into
# dry-run/verbose recipe output.
#
# No --locked here, unlike every resolving cargo invocation in these
# recipes: `cargo sqlx migrate run` replays SQL files through sqlx-cli and
# never resolves this workspace's dependencies, so there is nothing for the
# flag to lock. scripts/cargo-locked-guard.sh exempts it by pattern.
#
# Apply pending migrations directly with sqlx-cli, bypassing the backend build.
[group('db')]
db-migrate-raw *args:
    cd backend && DATABASE_URL="${DATABASE_URL_MIGRATION:-postgres:///reverie_dev?host=${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock&user=reverie_migrator&password=reverie_migrator}" cargo sqlx migrate run {{ args }}

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
