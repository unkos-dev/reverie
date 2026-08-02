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

manager="$(jq -r '.devEngines.packageManager.name // ""' package.json)"
if [ "$manager" != "npm" ]; then
  err package.json "devEngines.packageManager.name is '${manager}', not npm; this guard compares an npm pin, so update it if the package manager changed"
  exit 1
fi

declared="$(jq -r '.devEngines.packageManager.version // ""' package.json)"
# Shape-check before comparing. Two absent keys would read as equal and pass
# having verified nothing, and a range would let mise and npm resolve to
# different builds while still agreeing textually.
case "$declared" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *)
    err package.json "devEngines.packageManager.version is '${declared}', not an exact x.y.z pin; npm compares the running binary against it verbatim"
    exit 1
    ;;
esac

provisioned="$(yq -p toml -oy '.tools."npm:npm" // ""' mise.toml)"
case "$provisioned" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *)
    err mise.toml "the \"npm:npm\" pin is '${provisioned}', not an exact x.y.z version; a checkout without it inherits whatever npm the ambient config supplies"
    exit 1
    ;;
esac

if [ "$provisioned" != "$declared" ]; then
  err mise.toml "\"npm:npm\" pins ${provisioned} but package.json declares ${declared}; npm rejects every direct invocation with EBADDEVENGINES when the two disagree"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "npm pins agree (${declared}) across package.json devEngines and mise.toml"
