#!/usr/bin/env bash
# A pin guard that never fires is indistinguishable from agreeing pins, so
# plant each drift it is supposed to catch and assert it rejects them.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
checker="${root}/scripts/vp-pin-drift.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

mkdir -p "${tmp}/.github/workflows" "${tmp}/frontend"

REF="0.2.6"
ALIAS="npm:@voidzero-dev/vite-plus-core@${REF}"

baseline() {
  jq -n --arg v "$REF" --arg a "$ALIAS" \
    '{devDependencies: {"vite-plus": $v, vite: $a}, overrides: {vite: "$vite"}}' \
    >"${tmp}/package.json"
  jq -n --arg v "$REF" --arg a "$ALIAS" \
    '{devDependencies: {"vite-plus": $v, vite: $a}}' \
    >"${tmp}/frontend/package.json"
  local f
  for f in ci docs-build scheduled-audit; do
    printf 'env:\n  VP_VERSION: "%s"\n  NODE_VERSION: "24.18.0"\n' "$REF" \
      >"${tmp}/.github/workflows/${f}.yml"
  done
}

edit_json() {
  jq "$2" "$1" >"${1}.next" && mv "${1}.next" "$1"
}

fail=0
expect() {
  local name="$1" want="$2" got=0
  (cd "$tmp" && "$checker") >/dev/null 2>&1 || got=$?
  if [ "$got" -ne "$want" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${got}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

baseline
expect "agreeing pins pass" 0

baseline
edit_json "${tmp}/frontend/package.json" '.devDependencies["vite-plus"] = "0.2.5"'
expect "frontend vite-plus lagging rejected" 1

baseline
edit_json "${tmp}/package.json" '.devDependencies.vite = "npm:@voidzero-dev/vite-plus-core@0.2.5"'
expect "root vite alias lagging rejected" 1

baseline
edit_json "${tmp}/frontend/package.json" '.devDependencies.vite = "npm:@voidzero-dev/vite-plus-core@0.2.5"'
expect "frontend vite alias lagging rejected" 1

baseline
edit_json "${tmp}/package.json" '.devDependencies.vite = "^8.0.0"'
expect "unaliased vite dependency rejected" 1

# The exact shape that broke the 0.2.6 bump: Renovate moved the direct
# dependency and left the override on the previous version.
baseline
edit_json "${tmp}/package.json" '.overrides.vite = "npm:@voidzero-dev/vite-plus-core@0.2.5"'
expect "literal override spec rejected" 1

baseline
edit_json "${tmp}/package.json" '.overrides.vite = "npm:@voidzero-dev/vite-plus-core@0.2.6"'
expect "literal override matching today rejected" 1

baseline
edit_json "${tmp}/package.json" 'del(.overrides.vite)'
expect "missing override rejected" 1

baseline
printf 'env:\n  VP_VERSION: "0.2.5"\n  NODE_VERSION: "24.18.0"\n' \
  >"${tmp}/.github/workflows/docs-build.yml"
expect "workflow VP_VERSION lagging rejected" 1

baseline
printf 'env:\n  VP_VERSION: "%s"\n  NODE_VERSION: "24.17.0"\n' "$REF" \
  >"${tmp}/.github/workflows/scheduled-audit.yml"
expect "workflow NODE_VERSION disagreeing rejected" 1

baseline
edit_json "${tmp}/package.json" '.devDependencies["vite-plus"] = "workspace:*"'
expect "unparsable reference version rejected" 1

exit "$fail"
