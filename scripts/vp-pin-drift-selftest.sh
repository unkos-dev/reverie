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

NODE="24.18.0"

# A workflow the guard should discover: it installs vp and carries both pins.
workflow() {
  printf 'env:\n  VP_VERSION: "%s"\n  NODE_VERSION: "%s"\njobs:\n  build:\n    steps:\n      - uses: voidzero-dev/setup-vp@250f29c\n' \
    "$1" "$2" >"$3"
}

baseline() {
  jq -n --arg v "$REF" --arg a "$ALIAS" \
    '{devDependencies: {"vite-plus": $v, vite: $a}, overrides: {vite: "$vite"}}' \
    >"${tmp}/package.json"
  jq -n --arg v "$REF" --arg a "$ALIAS" \
    '{devDependencies: {"vite-plus": $v, vite: $a}}' \
    >"${tmp}/frontend/package.json"
  rm -f "${tmp}"/.github/workflows/*.yml "${tmp}"/.github/workflows/*.yaml
  local f
  for f in ci docs-build scheduled-audit; do
    workflow "$REF" "$NODE" "${tmp}/.github/workflows/${f}.yml"
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
workflow "0.2.5" "$NODE" "${tmp}/.github/workflows/docs-build.yml"
expect "workflow VP_VERSION lagging rejected" 1

baseline
workflow "$REF" "24.17.0" "${tmp}/.github/workflows/scheduled-audit.yml"
expect "workflow NODE_VERSION disagreeing rejected" 1

# The census is discovered, not listed, so a workflow added later is covered
# without touching this guard.
baseline
workflow "0.2.5" "$NODE" "${tmp}/.github/workflows/newcomer.yml"
expect "newly added workflow discovered and its lagging pin rejected" 1

baseline
workflow "$REF" "$NODE" "${tmp}/.github/workflows/newcomer.yml"
expect "newly added workflow with agreeing pins passes" 0

# Discovery must not sweep in workflows that neither install vp nor pin it.
baseline
printf 'jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v7\n' \
  >"${tmp}/.github/workflows/unrelated.yml"
expect "workflow that neither installs nor pins vp ignored" 0

# yq prints "null" for an absent key, so pins missing everywhere would agree
# with each other; setup-vp then resolves the latest LTS node instead.
baseline
for f in ci docs-build scheduled-audit; do
  printf 'env:\n  VP_VERSION: "%s"\njobs:\n  build:\n    steps:\n      - uses: voidzero-dev/setup-vp@250f29c\n' \
    "$REF" >"${tmp}/.github/workflows/${f}.yml"
done
expect "NODE_VERSION missing from every workflow rejected" 1

baseline
printf 'env:\n  VP_VERSION: "%s"\njobs:\n  build:\n    steps:\n      - uses: voidzero-dev/setup-vp@250f29c\n' \
  "$REF" >"${tmp}/.github/workflows/docs-build.yml"
expect "NODE_VERSION missing from one workflow rejected" 1

baseline
workflow "$REF" "24" "${tmp}/.github/workflows/ci.yml"
expect "NODE_VERSION without a patch level rejected" 1

# An empty census means the discovery pattern went stale, not that the pins
# agree, so the guard must fail rather than report success having checked
# nothing.
baseline
rm -f "${tmp}"/.github/workflows/*.yml
expect "empty census rejected" 1

baseline
edit_json "${tmp}/package.json" '.devDependencies["vite-plus"] = "workspace:*"'
expect "unparsable reference version rejected" 1

# Malformed vite-plus versions planted in EVERY copy, so all the equality
# comparisons agree and the shape check is the only thing left that can
# reject. Anything less isolating passes for the wrong reason: a bad value in
# one file alone is caught by the mismatch it creates, which a prefix-matching
# glob would also catch, so the case would not distinguish the two matchers.
consistent_tree() {
  local v="$1" alias="npm:@voidzero-dev/vite-plus-core@$1" f
  jq -n --arg v "$v" --arg a "$alias" \
    '{devDependencies: {"vite-plus": $v, vite: $a}, overrides: {vite: "$vite"}}' \
    >"${tmp}/package.json"
  jq -n --arg v "$v" --arg a "$alias" \
    '{devDependencies: {"vite-plus": $v, vite: $a}}' \
    >"${tmp}/frontend/package.json"
  rm -f "${tmp}"/.github/workflows/*.yml "${tmp}"/.github/workflows/*.yaml
  for f in ci docs-build scheduled-audit; do
    workflow "$v" "${2:-$NODE}" "${tmp}/.github/workflows/${f}.yml"
  done
}

for bad in "0.2.6-beta" "0.2.6junk" "0x.2.6" "0.2.6.7" "0.2" "v0.2.6" "0-2-6"; do
  consistent_tree "$bad"
  expect "malformed vite-plus version (${bad}) rejected in every copy" 1
done

# The same shapes in NODE_VERSION, which setup-vp hands to actions verbatim
# and which resolves to nothing once it stops naming a published release.
for bad in "24.18.0-rc" "24.18.0junk" "24x.18.0" "24.18.0.1" "v24.18.0"; do
  baseline
  for f in ci docs-build scheduled-audit; do
    workflow "$REF" "$bad" "${tmp}/.github/workflows/${f}.yml"
  done
  expect "malformed NODE_VERSION (${bad}) rejected" 1
done

# Positive controls. A matcher tightened past the accepted set would reject
# these too while every rejection case above still passed, so rejection alone
# does not prove the shape check is right.
consistent_tree "10.200.3000" "100.0.1"
expect "multi-digit components accepted on both pins" 0

consistent_tree "0.0.0" "0.0.0"
expect "all-zero versions accepted on both pins" 0

exit "$fail"
