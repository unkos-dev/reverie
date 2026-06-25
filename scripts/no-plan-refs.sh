#!/usr/bin/env bash
#
# Reject plan-artifact references in source-code comments and docstrings.
# Phase tags like `(S2)`, plan-internal numbering like `(decision 10)` /
# `(invariant 2)`, and `Phase 1` / `Phase 2` rollout labels point into a
# planning document a future reader does not have. A docstring describes the
# codebase's current behavior, so such a label is a dangling pointer: the fix
# is to keep the substantive prose and strip the label, or rewrite it in
# codebase terms.
#
# Scope: source under backend/src + frontend/src only. Excluded — these terms
# are legitimate there:
#   - Markdown (ADRs, /plans, debt, docs, README) describe process and may cite
#     plan steps,
#   - agent-process instructions (CLAUDE.md / AGENTS.md / GEMINI.md, any dir),
#   - vendored UI primitives (frontend/src/components/ui),
#   - the archival .claude tree.
#
# Callers (lint-staged staged files; CI changed files) pass a broad candidate
# list; this script applies the in-scope policy. Commit messages and PR bodies
# may cite plan steps; this guard is files only.
#
# Usage: no-plan-refs.sh <file>...
set -euo pipefail

# Parenthetical phase tags `(S2)`; `decision N` / `invariant N` numbering;
# capitalised `Phase N` rollout labels. Lowercase `phase N` is left alone so a
# genuine runtime-phase description does not trip the guard.
pattern='\(S[0-9]+\)|\b(decision|invariant) [0-9]+\b|\bPhase [0-9]+\b'

# Decide whether a path is gated by this guard. `case` globs treat '*' as
# matching across '/', so a single '*' spans nested directories.
is_gated() {
  case "$1" in
    frontend/src/components/ui/*) return 1 ;;
    .claude/*) return 1 ;;
    CLAUDE.md | AGENTS.md | GEMINI.md | */CLAUDE.md | */AGENTS.md | */GEMINI.md) return 1 ;;
  esac
  case "$1" in
    backend/src/*.rs) return 0 ;;
    frontend/src/*.ts | frontend/src/*.tsx | frontend/src/*.js | frontend/src/*.jsx | frontend/src/*.css) return 0 ;;
  esac
  return 1
}

# lint-staged passes absolute paths; CI (paths-filter) passes repo-relative
# ones. Normalise to repo-relative, then keep only in-scope paths that exist.
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
    echo "Plan-artifact references found in source comments:"
    echo ""
    echo "$matches"
    echo ""
    echo "Docstrings describe the codebase, not a planning document. Keep the"
    echo "substantive prose and strip the label: '(decision 10)' -> delete it;"
    echo "'Phase 2 enforces X' -> 'the validating middleware enforces X'."
    echo "(Plans, ADRs, docs, and commit messages may cite plan steps.)"
  } >&2
  exit 1
fi
