#!/usr/bin/env bash
# The npm version is declared twice and the copies fail asymmetrically:
#
#   - `devEngines.packageManager` in package.json is what npm itself enforces.
#     Every direct npm invocation compares the running binary against it and
#     hard-errors with EBADDEVENGINES on a mismatch; the declared
#     `onFail: download` does not rescue it.
#   - `"npm:npm"` in mise.toml is what actually provisions npm for a local
#     checkout. Without it a developer inherits an ambient npm, and the moment
#     that ambient pin moves ahead the whole local toolchain stops: no
#     `npm install`, and no lefthook step that shells out to npm.
#
# Only the pair working together fixes the drift, and only this guard keeps
# them a pair. CI is not a backstop for the mismatch: every job installs via
# `vp install`, which never consults devEngines.
set -euo pipefail

fail=0
err() {
  echo "::error file=$1::$2" >&2
  fail=1
}

# Anchored on both ends deliberately. A trailing-wildcard glob accepts
# `11.18.0-beta`, `11x.18.0`, and `1.2.3.4`, and when the same malformed value
# reaches both files the equality check below agrees with itself and the guard
# reports success.
#
# Rejecting prereleases is a policy, not a resolution constraint: npm publishes
# prereleases and resolves them fine (`react-data-grid` is pinned to a beta in
# frontend/package.json, and npm accepts a prerelease `devEngines` pin when the
# running binary is that same prerelease). The build toolchain is held to
# stable releases because a prerelease npm is a deliberate temporary decision
# that should be reviewed rather than waved through by a drift guard. A runtime
# dependency and the toolchain that builds it carry different risk.
exact_version() {
  [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

# Report a value the shape check rejected, naming a prerelease as itself: the
# generic "not an exact pin" wording sends a maintainer who pinned one looking
# for a parse bug instead of the policy above.
shape_hint() {
  case "$1" in
    *-*) echo "prereleases are rejected by policy; pin a stable release" ;;
    "" | null) echo "the key is absent" ;;
    *) echo "expected an exact x.y.z version" ;;
  esac
}

# devEngines.packageManager accepts an object or an array of objects. Select
# the npm entry from either rather than letting jq abort with "Cannot index
# array with string", which exits before any guard message is printed.
pm="$(jq -c '[.devEngines.packageManager] | flatten | map(select(.name == "npm")) | first // {}' package.json)"
if [ "$pm" = "{}" ]; then
  err package.json "devEngines.packageManager declares no npm entry; this guard compares an npm pin, so update it if the package manager changed"
  exit 1
fi

declared="$(jq -r '.version // ""' <<<"$pm")"
# Shape-check before comparing. Two absent keys would read as equal and pass
# having verified nothing, and a range would let mise and npm resolve to
# different builds while still agreeing textually.
if ! exact_version "$declared"; then
  err package.json "devEngines.packageManager version is '${declared}': $(shape_hint "$declared"). npm compares the running binary against it on every invocation"
  exit 1
fi

# Accept both the inline form (`"npm:npm" = "11.18.0"`) and the table form
# (`[tools."npm:npm"]` with a `version` key) that every other backend-prefixed
# entry in mise.toml uses. Reading only the inline form renders a table as
# `version: 11.18.0` and blames the version string for what is a syntax
# difference. The `select` is load-bearing: indexing `.version` on an absent
# key materialises a `version: null` node rather than yielding null, so a
# bare `//` chain reports a missing pin as a malformed one.
provisioned="$(yq -p toml -oy '[.tools."npm:npm" | select(. != null) | (.version // .)] | .[0] // ""' mise.toml)"
if ! exact_version "$provisioned"; then
  err mise.toml "the \"npm:npm\" pin is '${provisioned}': $(shape_hint "$provisioned"). Without a usable pin a checkout inherits whatever npm the ambient config supplies"
  exit 1
fi

if [ "$provisioned" != "$declared" ]; then
  err mise.toml "\"npm:npm\" pins ${provisioned} but package.json declares ${declared}; npm rejects every direct invocation with EBADDEVENGINES when the two disagree"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "npm pins agree (${declared}) across package.json devEngines and mise.toml"
