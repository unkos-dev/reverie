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
# them a pair. CI is not a backstop for the mismatch either: the JS jobs
# provision node and this npm through mise-action and then run bare `npx`,
# which does consult devEngines, so the same drift breaks CI too.
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
#
# The prerelease arm matches semver's prerelease separator, a `-` after three
# numeric components, not a bare hyphen. `case` arms are ordered and `*-*`
# would win for `11-18-0` or an `npm:pkg@1.2.3` alias spec, answering a typo
# with an instruction to remove a prerelease that was never there.
shape_hint() {
  case "$1" in
    [0-9]*.[0-9]*.[0-9]*-*) echo "prereleases are rejected by policy; pin a stable release" ;;
    "" | null) echo "the key is absent" ;;
    *) echo "expected an exact x.y.z version" ;;
  esac
}

# devEngines.packageManager is read as a single object, the shape this repo
# uses. npm also accepts a string ("npm@11.18.0", the top-level
# packageManager spelling) and an array of candidate objects, and both are
# rejected here by name rather than either crashing jq or being quietly
# blessed.
#
# Accepting a shape the Renovate custom manager cannot parse would be worse
# than rejecting it: that manager is the only thing keeping this pin in step
# with mise.toml, its regex targets the object form, and a regex cannot
# reliably pick the npm entry out of a multi-candidate array. A guard whose
# contract is wider than the machinery around it produces a silent gap, so
# the two are held to one shape and this message names the other side.
pm_type="$(jq -r '.devEngines.packageManager | type' package.json)"
if [ "$pm_type" != "object" ]; then
  err package.json "devEngines.packageManager has type '${pm_type}', and this guard reads the single-object form. npm accepts a string or an array too, but the Renovate custom manager that keeps this pin in step with mise.toml matches only the object form, so change both together if the shape must move"
  exit 1
fi

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
  err package.json "devEngines.packageManager version is '${declared}': $(shape_hint "$declared"). npm compares the running binary against it on every invocation"
  exit 1
fi

# Accept both the inline form (`"npm:npm" = "11.18.0"`) and the table form
# (`[tools."npm:npm"]` with a `version` key) that every other backend-prefixed
# entry in mise.toml uses, since converting this entry to a table to add
# `filter_bins` is a plausible edit. Reading only the inline form renders a
# table as `version: 11.18.0` and blames the version string for what is a
# syntax difference.
#
# Each shape is resolved to a scalar before it reaches the version check.
# Indexing `.version` on a mapping that lacks the key yields a node that
# renders as multiple lines, so without the explicit cases below a table
# missing its version reports the whole mapping as the offending version.
pin_kind="$(yq -p toml -oy '.tools."npm:npm" | type' mise.toml)"
case "$pin_kind" in
  "!!str") provisioned="$(yq -p toml -oy '.tools."npm:npm"' mise.toml)" ;;
  "!!map")
    if [ "$(yq -p toml -oy '.tools."npm:npm" | has("version")' mise.toml)" != "true" ]; then
      err mise.toml "the [tools.\"npm:npm\"] table has no version key; a table entry pins nothing without one, so a checkout inherits whatever npm the ambient config supplies"
      exit 1
    fi
    provisioned="$(yq -p toml -oy '.tools."npm:npm".version' mise.toml)"
    ;;
  *) provisioned="" ;;
esac

if ! exact_version "$provisioned"; then
  err mise.toml "the \"npm:npm\" pin is '${provisioned}': $(shape_hint "$provisioned"). Without a usable pin a checkout inherits whatever npm the ambient config supplies"
  exit 1
fi

if [ "$provisioned" != "$declared" ]; then
  err mise.toml "\"npm:npm\" pins ${provisioned} but package.json declares ${declared}; npm rejects every direct invocation with EBADDEVENGINES when the two disagree"
fi

# Both node and "npm:npm" ship npm and npx bins, and mise resolves a
# contested bin to the first tool listed in mise.toml's [tools] table, for
# shims and for the PATH order `mise env` exports. If node is listed first,
# its bundled npm shadows this pin and every direct npm/npx invocation fails
# with EBADDEVENGINES, so the order matters as much as the version agreement
# checked above. The node entry is absent from a fixture that only exercises
# the npm pin, and that is fine: with no node entry there is nothing to
# shadow the pin, so the check passes silently.
node_line="$( (grep -n '^node[[:space:]]*=' mise.toml || true) | head -n1 | cut -d: -f1)"
npm_line="$( (grep -n -E '^"npm:npm"[[:space:]]*=|^\[tools\."npm:npm"\]' mise.toml || true) | head -n1 | cut -d: -f1)"
if [ -n "$node_line" ] && [ -n "$npm_line" ] && [ "$node_line" -lt "$npm_line" ]; then
  err mise.toml "node lists before \"npm:npm\"; mise resolves the contested npm and npx bins to the first tool in table order, so node's bundled npm shadows the pin and direct npm invocations fail with EBADDEVENGINES. Keep \"npm:npm\" listed before node"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "npm pins agree (${declared}) across package.json devEngines and mise.toml"
