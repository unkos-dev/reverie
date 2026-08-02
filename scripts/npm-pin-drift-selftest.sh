#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
checker="${root}/scripts/npm-pin-drift.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

write_pins() {
  jq -n --arg v "$1" \
    '{devEngines: {packageManager: {name: "npm", version: $v, onFail: "download"}}}' \
    >"${tmp}/package.json"
  printf '[tools]\n"npm:npm" = "%s"\n' "$2" >"${tmp}/mise.toml"
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

write_pins "11.18.0" "11.18.0"
expect "matching pins pass" 0

write_pins "11.18.0" "11.19.0"
expect "mise ahead of devEngines rejected" 1

write_pins "11.19.0" "11.18.0"
expect "devEngines ahead of mise rejected" 1

# The failure this guard exists to prevent starts as an absent pin, not a
# disagreeing one: the pre-fix state was a checkout with no mise npm entry at
# all, falling through to an ambient version.
write_pins "11.18.0" "11.18.0"
printf '[tools]\njust = "1.57.0"\n' >"${tmp}/mise.toml"
expect "absent mise pin rejected" 1

# A range in either copy resolves to different builds while still reading as
# a pin, so both sides are shape-checked rather than only compared.
write_pins "^11.18.0" "11.18.0"
expect "ranged devEngines pin rejected" 1

write_pins "11.18.0" "11"
expect "ranged mise pin rejected" 1

# Malformed values planted in BOTH copies. These are the cases a
# trailing-wildcard glob accepts: with the same bad value on each side the
# equality check agrees with itself, so only an anchored match rejects them.
for bad in "11.18.0-beta" "11.18.0junk" "11x.18.0" "1.2.3.4" "11.18" "v11.18.0" "11-18-0"; do
  write_pins "$bad" "$bad"
  expect "matching malformed pins (${bad}) rejected" 1
done

# Positive controls. A regex tightened past the accepted set would reject
# these too, and every case above would still pass, so rejection alone does
# not prove the matcher is right.
write_pins "11.18.0" "11.18.0"
expect "single-digit components accepted" 0

write_pins "110.180.1000" "110.180.1000"
expect "multi-digit components accepted" 0

write_pins "0.0.0" "0.0.0"
expect "all-zero version accepted" 0

# Two absent versions would compare equal and pass having checked nothing.
jq -n '{devEngines: {packageManager: {name: "npm", onFail: "download"}}}' >"${tmp}/package.json"
printf '[tools]\njust = "1.57.0"\n' >"${tmp}/mise.toml"
expect "both pins absent rejected" 1

# The guard reads an npm pin specifically; a swapped package manager must say
# so rather than silently comparing the wrong thing.
jq -n '{devEngines: {packageManager: {name: "pnpm", version: "11.18.0"}}}' >"${tmp}/package.json"
printf '[tools]\n"npm:npm" = "11.18.0"\n' >"${tmp}/mise.toml"
expect "non-npm package manager rejected" 1

exit "$fail"
