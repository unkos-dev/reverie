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
# reached: callers only ever pass tracked / staged / changed files, so there
# is nothing to exclude for performance.
#
# Callers (lint-staged staged files; CI changed files) pass a broad candidate
# list; this script applies the in-scope policy. Commit messages and PR bodies
# may cite issues; this guard is files only.
#
# Usage: no-issue-refs.sh <file>...
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

# lint-staged passes absolute paths to string commands; CI (paths-filter)
# passes repo-relative ones. Normalise to repo-relative so the is_gated()
# include/exclude patterns match in both cases, then keep only in-scope paths
# that still exist — a changed-file list can name deletions.
files=()
for path in "$@"; do
  rel="${path#"$PWD/"}"
  if [ -f "$rel" ] && is_gated "$rel"; then
    files+=("$rel")
  fi
done

if [ "${#files[@]}" -eq 0 ]; then
  exit 0
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
