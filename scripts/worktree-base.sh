#!/usr/bin/env bash
# Resolve the start point for a brand-new `just worktree` branch: the third
# arm of that recipe, reached only when BRANCH is neither an existing local
# branch nor a remote-tracking one. `git worktree add -b` with no explicit
# start point bases the new branch on the CALLER's current HEAD, so invoking
# the recipe from a feature checkout silently carried that branch's commits
# into every new worktree. This picks a stable base instead.
#
# Usage: scripts/worktree-base.sh <branch> [explicit-base]
#
# Prints exactly one line, "<mode> <ref>", to stdout and exits 0:
#   explicit <ref>        an explicit-base argument was given and resolves
#   origin   origin/main  origin/main exists (this reads the last-fetched
#                          state; the caller decides whether to fetch first)
#   local    main         no origin/main, but a local main exists
#   head     HEAD         neither exists; the new branch starts from this
#                          checkout's current HEAD. A WARNING is printed to
#                          stderr, since this is almost never the intended
#                          base.
#
# An explicit-base argument that does not resolve to a commit is a hard
# failure: nothing is printed to stdout and the script exits nonzero, so a
# caller that forgets to check the exit status cannot silently fall through
# to the resolution chain below.
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$repo_root"

branch="${1:?usage: scripts/worktree-base.sh <branch> [explicit-base]}"
explicit_base="${2:-}"

if [ -n "$explicit_base" ]; then
  if git rev-parse --verify --quiet "${explicit_base}^{commit}" > /dev/null; then
    printf 'explicit %s\n' "$explicit_base"
    exit 0
  fi
  echo "worktree-base: explicit base '${explicit_base}' does not resolve to a commit" >&2
  echo "fix: pass a valid branch, tag, or commit-ish (run 'git fetch origin main' first if it should live on the remote)" >&2
  exit 1
fi

if git show-ref --verify --quiet refs/remotes/origin/main; then
  printf 'origin origin/main\n'
  exit 0
fi

if git show-ref --verify --quiet refs/heads/main; then
  printf 'local main\n'
  exit 0
fi

echo "warning: neither origin/main nor a local main exists; new branch '${branch}' will start from this checkout's current HEAD" >&2
echo "fix: run 'git fetch origin main' to fetch the intended base, then retry" >&2
printf 'head HEAD\n'
