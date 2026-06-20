#!/usr/bin/env bash
#
# Run the Reverie prose linter (Vale) over in-scope Markdown.
#
# Scope: the Starlight docs (docs/src), Architecture Decision Records (adr),
# and repo-root Markdown. The following are excluded from prose linting:
#   - agent-process instructions (CLAUDE.md / AGENTS.md / GEMINI.md, any dir),
#   - the archival .claude tree.
#
# Posture is advisory: the rules emit at `warning`, so Vale exits zero and this
# wrapper does not block a commit. The scope policy lives here so lint-staged
# (staged files) and CI (`--all`) share one source of truth, mirroring
# scripts/no-issue-refs.sh.
#
# Usage:
#   vale-lint.sh <file>...   # lint the given files that fall in scope
#   vale-lint.sh --all       # lint every tracked in-scope file
set -euo pipefail

# Decide whether a path is linted. `case` globs treat '*' as matching across
# '/', so a single '*' spans nested directories.
in_scope() {
  case "$1" in
    # Exclusions first. Prose linting does not apply to these.
    .claude/*) return 1 ;;
    CLAUDE.md | AGENTS.md | GEMINI.md | */CLAUDE.md | */AGENTS.md | */GEMINI.md) return 1 ;;
  esac
  case "$1" in
    docs/src/*.md | docs/src/*.mdx) return 0 ;;
    adr/*.md) return 0 ;;
    */*) return 1 ;; # any other nested path is out of scope
    *.md | *.mdx) return 0 ;; # repo-root Markdown
  esac
  return 1
}

candidates=()
if [ "${1:-}" = "--all" ]; then
  while IFS= read -r -d '' path; do
    candidates+=("$path")
  done < <(git ls-files -z -- docs/src adr ':(glob)*.md' ':(glob)*.mdx')
else
  candidates=("$@")
fi

# lint-staged passes absolute paths to string commands; CI passes repo-relative
# ones. Normalise to repo-relative so in_scope() matches in both cases, then
# keep only in-scope paths that still exist (a changed-file list can name
# deletions).
files=()
for path in "${candidates[@]+"${candidates[@]}"}"; do
  rel="${path#"$PWD/"}"
  rel="${rel#./}"
  if [ -f "$rel" ] && in_scope "$rel"; then
    files+=("$rel")
  fi
done

if [ "${#files[@]}" -eq 0 ]; then
  exit 0
fi

exec vale "${files[@]}"
