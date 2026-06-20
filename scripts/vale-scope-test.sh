#!/usr/bin/env bash
#
# Test for the scope policy in scripts/vale-lint.sh. Sourcing that script loads
# in_scope() without running Vale, so the include/exclude classification is
# unit-tested directly (no Vale, no files needing to exist on disk). A second
# block stubs `vale` to confirm leading ./ is normalised before matching.
#
# Run from anywhere; it resolves the repo root. Exits non-zero on any mismatch
# so it can gate in CI.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# shellcheck source=scripts/vale-lint.sh
source scripts/vale-lint.sh

fail=0

# check <path> <in|out>
check() {
  local path="$1" want="$2" got
  if in_scope "$path"; then got=in; else got=out; fi
  if [ "$got" = "$want" ]; then
    echo "ok   $want  $path"
  else
    echo "FAIL expected $want, got $got for: $path" >&2
    fail=1
  fi
}

# In scope: docs/src (recursive), adr (recursive), repo-root Markdown.
check "docs/src/content/docs/guides/cover-images.md" in
check "docs/src/reference/index.mdx" in
check "adr/2026-01-01-example.md" in
check "adr/superseded/old-decision.md" in
check "README.md" in

# Excluded: agent-process docs in any directory.
check "CLAUDE.md" out
check "AGENTS.md" out
check "GEMINI.md" out
check "adr/CLAUDE.md" out
check "docs/src/CLAUDE.md" out

# Excluded: the archival .claude tree.
check ".claude/commands/foo.md" out

# Excluded: non-scope nested paths and root non-Markdown.
check "frontend/README.md" out
check "debt/README.md" out
check "backend/src/main.rs" out
check "package.json" out

# Normalisation: a leading ./ on an in-scope path must still be linted. Stub
# `vale` so the wrapper's selection is observable without invoking the binary.
stub=$(mktemp -d)
trap 'rm -rf "$stub"' EXIT
# shellcheck disable=SC2016  # literal $@/$f belong in the generated stub
printf '#!/usr/bin/env bash\nfor f in "$@"; do echo "$f"; done\n' >"$stub/vale"
chmod +x "$stub/vale"

selected=$(PATH="$stub:$PATH" scripts/vale-lint.sh ./README.md ./CLAUDE.md)
if [ "$selected" = "README.md" ]; then
  echo "ok   ./-prefixed in-scope path normalised and selected"
else
  echo "FAIL ./ normalisation: expected [README.md], got [$selected]" >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "vale scope test: FAILED" >&2
  exit 1
fi
echo "vale scope test: passed"
