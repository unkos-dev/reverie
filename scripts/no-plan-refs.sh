#!/usr/bin/env bash
#
# Reject plan-artifact references in source-code comments and docstrings.
# Phase tags like `(S2)`, plan-internal numbering like `(decision 10)` /
# `(invariant 2)`, `Phase 1` / `Phase 2` rollout labels, and `plans/...` paths
# point into a planning document a future reader does not have (the /plans
# tree is gitignored). A docstring describes the codebase's current behavior,
# so such a label is a dangling pointer: the fix is to keep the substantive
# prose and strip the label, or rewrite it in codebase terms.
#
# Scope: every tracked file that can carry a comment or a string (source,
# TOML, JSON, YAML, shell, SQL, Dockerfiles, the justfiles); binaries are
# skipped by grep. Excluded — these terms are legitimate there:
#   - Markdown (ADRs, /plans, debt, docs, README) describe process and may cite
#     plan steps,
#   - agent-process instructions (CLAUDE.md / AGENTS.md / GEMINI.md, any dir),
#   - vendored UI primitives (frontend/src/components/ui),
#   - the archival .claude tree,
#   - the two reference guards, whose self-checks carry the patterns.
#
# Callers (lefthook staged files; CI changed files) pass a broad candidate
# list; this script applies the in-scope policy. Commit messages and PR bodies
# may cite plan steps; this guard is files only.
#
# Usage: no-plan-refs.sh <file>...
set -euo pipefail

# Parenthetical phase tags `(S2)` in any letter case; singular or plural
# `decision N` / `invariant N` numbering; `plans/` path references into the
# gitignored planning tree (a path into it, so the bare `/plans/` line in
# .gitignore that names the directory itself does not match). Capitalised or
# all-caps `Phase N` rollout labels are matched separately; lowercase
# `phase N` is left alone so a genuine runtime-phase description does not
# trip the guard.
pattern='\(S[0-9]+\)|\b(decisions?|invariants?) [0-9]+\b|\bplans/[^[:space:]]'
phase_pattern='\b(Phase|PHASE) [0-9]+\b'

# Decide whether a path is gated by this guard. `case` globs treat '*' as
# matching across '/', so a single '*' spans nested directories.
is_gated() {
  case "$1" in
    frontend/src/components/ui/*) return 1 ;;
    .claude/*) return 1 ;;
    CLAUDE.md | AGENTS.md | GEMINI.md | */CLAUDE.md | */AGENTS.md | */GEMINI.md) return 1 ;;
    *.md | *.mdx) return 1 ;;
    scripts/no-issue-refs.sh | scripts/no-plan-refs.sh) return 1 ;;
  esac
  return 0
}

# Positive control, run on every invocation. An empty file list is legitimate
# here (a commit touching nothing gated), so there is no census whose emptiness
# could signal breakage: a decayed is_gated arm or a mangled pattern would
# report every commit clean forever with nothing to notice. These assertions
# hold the guard to its own documented examples, and unlike a fixture outside
# the guard they run on the code path every real caller takes.
# Each alternative is probed with a string only that alternative can match. A
# single string carrying all four would keep matching after three of them
# decayed, which is the failure this exists to catch.
self_check() {
  grep -qiE "$pattern" <<<'(S2)' || return 1
  grep -qiE "$pattern" <<<'decision 10' || return 1
  grep -qiE "$pattern" <<<'invariant 2' || return 1
  grep -qiE "$pattern" <<<'plans/x.md' || return 1
  # The bare directory name, as .gitignore writes it, is deliberately left
  # alone; assert that it still is.
  grep -qiE "$pattern" <<<'/plans/' && return 1
  grep -qE "$phase_pattern" <<<'Phase 2 enforces the check' || return 1
  # Lowercase `phase 2` is deliberately left alone; assert that it still is.
  grep -qE "$phase_pattern" <<<'phase 2 of the request lifecycle' && return 1
  # The C locale keeps a line with a stray non-UTF-8 byte matchable, and -I
  # still skips NUL-bearing (binary) input; both are probed because a quiet
  # skip is the one failure this guard must never have.
  printf '(S2) \xff\n' | LC_ALL=C grep -qiIE "$pattern" || return 1
  printf '(S2)\0\n' | LC_ALL=C grep -qiIE "$pattern" && return 1
  is_gated backend/src/main.rs || return 1
  is_gated frontend/src/App.tsx || return 1
  is_gated backend/Cargo.toml || return 1
  is_gated Dockerfile || return 1
  ! is_gated AGENTS.md || return 1
  ! is_gated docs/adr/some-decision.md || return 1
  ! is_gated frontend/src/components/ui/button.tsx || return 1
  ! is_gated scripts/no-plan-refs.sh || return 1
}
if ! self_check; then
  echo "no-plan-refs: self-check failed; the scope rules or the patterns no longer match their own documented examples, so a clean result would mean nothing" >&2
  exit 2
fi

# lefthook and CI (paths-filter) both pass repo-relative paths. Normalise
# anyway (the strip tolerates an absolute path), then keep only paths that exist.
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
# file) that must not read as "clean". -I skips binary files (images, fonts);
# LC_ALL=C keeps a line with a stray non-UTF-8 byte from counting as binary too.
rc=0
matches=$(LC_ALL=C grep -niIE "$pattern" -- "${files[@]}") || rc=$?
if [ "$rc" -gt 1 ]; then
  exit "$rc"
fi

phase_rc=0
phase_matches=$(LC_ALL=C grep -nIE "$phase_pattern" -- "${files[@]}") || phase_rc=$?
if [ "$phase_rc" -gt 1 ]; then
  exit "$phase_rc"
fi
if [ -n "$phase_matches" ]; then
  matches="${matches}${matches:+$'\n'}${phase_matches}"
fi

if [ -n "$matches" ]; then
  {
    echo "Plan-artifact references found in source comments:"
    echo ""
    echo "$matches"
    echo ""
    echo "Docstrings describe the codebase, not a planning document. Keep the"
    echo "substantive prose and strip the label: '(decision 10)' -> delete it;"
    echo "'Phase 2 enforces X' -> 'the validating middleware enforces X';"
    echo "'Spec: plans/<doc>.md' -> delete it (the /plans tree is gitignored,"
    echo "so the path is unreachable for readers of shipped source)."
    echo "(Plans, ADRs, docs, and commit messages may cite plan steps.)"
  } >&2
  exit 1
fi
