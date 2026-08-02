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
# reports success. Prereleases are rejected rather than tolerated: npm compares
# `devEngines` by semver, where a prerelease does not satisfy a plain range,
# and mise resolves `npm:npm` to a concrete published version.
exact_version() {
  [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

manager="$(jq -r '.devEngines.packageManager.name // ""' package.json)"
if [ "$manager" != "npm" ]; then
  err package.json "devEngines.packageManager.name is '${manager}', not npm; this guard compares an npm pin, so update it if the package manager changed"
  exit 1
fi

declared="$(jq -r '.devEngines.packageManager.version // ""' package.json)"
# Shape-check before comparing. Two absent keys would read as equal and pass
# having verified nothing, and a range would let mise and npm resolve to
# different builds while still agreeing textually.
if ! exact_version "$declared"; then
  err package.json "devEngines.packageManager.version is '${declared}', not an exact x.y.z pin; npm compares the running binary against it verbatim"
  exit 1
fi

provisioned="$(yq -p toml -oy '.tools."npm:npm" // ""' mise.toml)"
if ! exact_version "$provisioned"; then
  err mise.toml "the \"npm:npm\" pin is '${provisioned}', not an exact x.y.z version; a checkout without it inherits whatever npm the ambient config supplies"
  exit 1
fi

if [ "$provisioned" != "$declared" ]; then
  err mise.toml "\"npm:npm\" pins ${provisioned} but package.json declares ${declared}; npm rejects every direct invocation with EBADDEVENGINES when the two disagree"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "npm pins agree (${declared}) across package.json devEngines and mise.toml"
