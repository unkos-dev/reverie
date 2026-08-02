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

# mise.toml uses [tools."backend:name"] table blocks for every other
# backend-prefixed tool, so converting this entry to that form is a plausible
# edit. Reading only the inline form renders the table as "version: 11.18.0"
# and blames the version string for what is a syntax difference.
write_pins "11.18.0" "11.18.0"
printf '[tools]\n[tools."npm:npm"]\nversion = "11.18.0"\n' >"${tmp}/mise.toml"
expect "table-form mise pin accepted" 0

printf '[tools]\n[tools."npm:npm"]\nversion = "11.19.0"\n' >"${tmp}/mise.toml"
expect "table-form mise pin still compared for drift" 1

# A table entry with no version key pins nothing. Resolving the mapping to a
# scalar without this case reports the whole two-line mapping as the offending
# version and blames the format rather than the missing key.
write_pins "11.18.0" "11.18.0"
printf '[tools]\n[tools."npm:npm"]\nfilter_bins = "npm"\n' >"${tmp}/mise.toml"
expect_message "table entry without a version names the missing key" "has no version key"

# npm accepts a string ("npm@11.18.0", the top-level packageManager spelling)
# and an array of candidates. Both are rejected rather than blessed: the
# Renovate custom manager that keeps this pin in step with mise.toml matches
# only the object form, and a guard whose contract is wider than that manager
# reintroduces the one-visible-copy gap the npm-pin group exists to close.
printf '[tools]\n"npm:npm" = "11.18.0"\n' >"${tmp}/mise.toml"
jq -n '{devEngines: {packageManager: "npm@11.18.0"}}' >"${tmp}/package.json"
expect_message "string-shaped packageManager names the shape" "has type 'string'"

jq -n '{devEngines: {packageManager: [{name: "npm", version: "11.18.0"}]}}' >"${tmp}/package.json"
expect_message "array-shaped packageManager names the shape" "has type 'array'"

# Failing closed is not enough: the message has to name the cause, or a
# maintainer who deliberately pinned a prerelease goes looking for a parse bug.
write_pins "11.18.0-beta" "11.18.0-beta"
expect_message "prerelease names itself as the cause" "prereleases are rejected by policy"

write_pins "11.18.0" "11.18.0"
printf '[tools]\njust = "1.57.0"\n' >"${tmp}/mise.toml"
expect_message "absent mise pin names itself as the cause" "the key is absent"

# Negative controls on the hint itself. `case` arms are ordered, so a
# prerelease glob broader than semver's separator silently wins for any
# hyphen-bearing value and answers a typo with "remove the prerelease". These
# are the arms that compete for a match, which is where message tests pay off.
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

for typo in "11-18-0" "11.18-0" "npm@11.18.0"; do
  write_pins "$typo" "$typo"
  expect_no_message "hyphenated typo (${typo}) is not called a prerelease" "prereleases are rejected"
done

exit "$fail"
