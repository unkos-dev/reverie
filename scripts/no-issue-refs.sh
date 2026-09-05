#!/usr/bin/env bash
#
# Reject issue-tracker references (e.g. UNK- followed by digits) in
# public-facing source and docs. These describe the codebase/product, not a
# private issue tracker; such a reference is usually a symptom that the whole
# comment or passage narrates process or deferred work that does not belong
# there — so the fix is to rewrite it in codebase terms or delete it, not just
# strip the id.
#
# Scope: source under backend/src + frontend/src, plus all Markdown (ADR,
# debt, docs, README, ...). Excluded — references are legitimate here:
#   - agent-process instructions (CLAUDE.md / AGENTS.md / GEMINI.md, any dir),
#   - vendored UI primitives (frontend/src/components/ui),
#   - the archival .claude tree.
# Gitignored files (caches, build output, /plans, node_modules) are never
# reached: the file set comes from `git ls-files`, which only ever lists
# tracked paths.
#
# Whole-tree: every tracked in-scope file is checked on every run, not just a
# caller-supplied candidate list, so a reference left over anywhere in scope
# fails the build even if it did not land in the current diff. Callers
# (lefthook staged files; CI changed files) may still pass a file list to
# decide whether to invoke this script at all; any arguments given to the
# script itself are accepted but ignored. Commit messages and PR bodies may
# cite issues; this guard is files only.
#
# Usage: no-issue-refs.sh [ignored-args...]
set -euo pipefail

pattern='UNK-[0-9]+'

# Decide whether a path is gated by this guard. `case` globs treat '*' as
# matching across '/', so a single '*' spans nested directories.
is_gated() {
  case "$1" in
    # Exclusions first — issue references are allowed in these.
    frontend/src/components/ui/*) return 1 ;;
    .claude/*) return 1 ;;
    CLAUDE.md | AGENTS.md | GEMINI.md | */CLAUDE.md | */AGENTS.md | */GEMINI.md) return 1 ;;
  esac
  case "$1" in
    backend/src/*.rs) return 0 ;;
    frontend/src/*.ts | frontend/src/*.tsx | frontend/src/*.js | frontend/src/*.jsx | frontend/src/*.css) return 0 ;;
    *.md | *.mdx) return 0 ;;
  esac
  return 1
}

# Positive control, run on every invocation. The census below is whole-tree, so
# its emptiness is checked too, but an empty census only catches gross
# breakage: one decayed is_gated arm leaves the census full and stops covering
# a whole file class silently. These assertions hold the guard to its own
# documented scope, and unlike a fixture outside the guard they run on the code
# path every real caller takes.
self_check() {
  grep -qE "$pattern" <<<'see UNK-123 for context' || return 1
  is_gated backend/src/main.rs || return 1
  is_gated frontend/src/App.tsx || return 1
  is_gated docs/adr/some-decision.md || return 1
  ! is_gated AGENTS.md || return 1
  ! is_gated frontend/src/components/ui/button.tsx || return 1
}
if ! self_check; then
  echo "no-issue-refs: self-check failed; the scope rules or the pattern no longer match their own documented examples, so a clean result would mean nothing" >&2
  exit 2
fi

# `git ls-files` lists every tracked path repo-relative to the current
# directory, so callers must run this from the repo root (both lefthook and
# the `just` recipe do). NUL-delimited so a path containing a newline cannot
# split and slip past the filter.
files=()
while IFS= read -r -d '' path; do
  if [ -f "$path" ] && is_gated "$path"; then
    files+=("$path")
  fi
done < <(git ls-files -z)

# The census is whole-tree and this repository always holds gated files, so an
# empty one means the enumeration broke, not that there is nothing to check.
# Reporting success there would be the guard's quietest possible failure.
if [ "${#files[@]}" -eq 0 ]; then
  echo "no-issue-refs: the tracked-file census is empty; refusing to report a clean tree" >&2
  exit 2
fi

# grep exit 1 = no matches (clean); exit >1 = a real error (e.g. an unreadable
# file) that must not read as "clean".
rc=0
matches=$(grep -nE "$pattern" -- "${files[@]}") || rc=$?
if [ "$rc" -gt 1 ]; then
  exit "$rc"
fi

if [ -n "$matches" ]; then
  {
    echo "Issue-tracker references found in public-facing files:"
    echo ""
    echo "$matches"
    echo ""
    echo "Source and docs describe the codebase/product, not a private tracker."
    echo "Such a reference usually means the comment narrates process or"
    echo "deferred work — rewrite it in codebase terms or delete it."
    echo "(Commit messages and PR bodies may cite issues.)"
  } >&2
  exit 1
fi
