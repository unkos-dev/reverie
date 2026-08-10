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

# A workflow that installs vp with no version: input, carrying no env block.
# node is provisioned through mise now, so a workflow installing vp has
# nothing left for this guard to check.
workflow() {
  printf 'jobs:\n  build:\n    steps:\n      - uses: voidzero-dev/setup-vp@250f29c\n' >"$1"
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
    workflow "${tmp}/.github/workflows/${f}.yml"
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

# Assert on the message, not just the exit code. A guard that fails closed but
# names the wrong cause sends the reader to the wrong fix, which is the whole
# reason the shape hints exist.
expect_message() {
  local name="$1" want="$2" out
  out="$( (cd "$tmp" && "$checker") 2>&1 || true)"
  if [[ $out == *"$want"* ]]; then
    echo "ok   ${name}"
  else
    echo "FAIL ${name}: expected message containing '${want}', got: ${out}"
    fail=1
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
    workflow "${tmp}/.github/workflows/${f}.yml"
  done
}

for bad in "0.2.6-beta" "0.2.6junk" "0x.2.6" "0.2.6.7" "0.2" "v0.2.6" "0-2-6"; do
  consistent_tree "$bad"
  expect "malformed vite-plus version (${bad}) rejected in every copy" 1
done

# Positive control. A matcher tightened past the accepted set would reject
# this too while every rejection case above still passed, so rejection alone
# does not prove the shape check is right.
consistent_tree "10.200.3000"
expect "multi-digit components accepted" 0

consistent_tree "0.0.0"
expect "all-zero version accepted" 0

# Failing closed is not enough: the message has to name the cause, or a
# maintainer who deliberately pinned a prerelease goes looking for a parse bug
# in a guard that is in fact enforcing a policy.
consistent_tree "0.2.6-beta"
expect_message "prerelease vite-plus pin names itself as the cause" \
  "prereleases are rejected by policy"

# Negative controls on the hint itself. `case` arms are ordered, so a
# prerelease glob broader than semver's separator silently wins for any
# hyphen-bearing value. The alias case is not hypothetical: devDependencies
# .vite in both package files already carries that exact shape.
expect_no_message() {
  local name="$1" unwanted="$2" out
  out="$( (cd "$tmp" && "$checker") 2>&1 || true)"
  if [[ $out == *"$unwanted"* ]]; then
    echo "FAIL ${name}: message should not mention '${unwanted}', got: ${out}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

for typo in "0-2-6" "0.2-6" "npm:@voidzero-dev/vite-plus-core@0.2.6"; do
  consistent_tree "$typo"
  expect_no_message "malformed vite-plus pin (${typo}) is not called a prerelease" \
    "prereleases are rejected"
done

exit "$fail"
