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
#
# When neither canonical ref exists the resolver fails rather than guessing:
# the only remaining candidate is the invoking checkout's HEAD, and basing a
# brand-new branch on whatever happens to be checked out is exactly the
# contamination this script exists to prevent. A warning does not enforce
# that invariant (automation and detached logs never read it), so the
# invocation stops instead. Passing HEAD as the explicit base is the
# deliberate opt-in for the rare case where the current checkout is the
# intended start point.
#
# Every failure, including an explicit-base argument that does not resolve
# to a commit, prints nothing to stdout and exits nonzero, so a caller that
# forgets to check the exit status cannot silently receive a start point.
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

echo "worktree-base: neither origin/main nor a local main exists; refusing to guess a base for new branch '${branch}'" >&2
echo "fix: run 'git fetch origin main' to fetch the intended base, or pass an explicit base (pass HEAD to deliberately base on this checkout)" >&2
exit 1
