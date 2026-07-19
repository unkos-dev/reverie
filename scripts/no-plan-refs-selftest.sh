#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
checker="${root}/scripts/no-plan-refs.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

mkdir -p "${tmp}/backend/src/nested" "${tmp}/frontend/src/nested" "${tmp}/frontend/src/components/ui"

fail=0
expect() {
  local name="$1" want="$2" path="$3" text="$4" got=0
  printf '%s\n' "$text" >"${tmp}/${path}"
  (cd "$tmp" && "$checker" "$path") >/dev/null 2>&1 || got=$?
  if [ "$got" -ne "$want" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${got}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

expect "ordinary source passes" 0 backend/src/lib.rs '// Validates the request.'
expect "lowercase runtime phase passes" 0 frontend/src/view.ts '// The loading phase ends here.'
expect "nested phase tag rejected" 1 backend/src/nested/mod.rs '// (S2) Validate input.'
expect "capitalized decision rejected" 1 frontend/src/nested/view.ts '// Decision 3 owns this.'
expect "lowercase invariant rejected" 1 backend/src/lib.rs '// invariant 4'
expect "plural decisions rejected" 1 frontend/src/view.ts '// decisions 2 and 3'
expect "plural invariants rejected" 1 frontend/src/view.ts '// Invariants 1-3'
expect "UI primitive remains exempt" 0 frontend/src/components/ui/button.tsx '// Phase 2 style token.'
expect "missing path ignored" 0 backend/src/missing.rs ''
expect "filename with spaces handled" 1 'backend/src/file name.rs' '// Decision 8'

exit "$fail"
