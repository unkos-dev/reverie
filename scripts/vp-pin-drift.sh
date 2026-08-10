#!/usr/bin/env bash
# The vite-plus version is pinned in more places than one Renovate update
# reliably reaches, and the copies fail in different ways when they disagree:
#
#   - `devDependencies.vite-plus` in the root and the frontend package drive
#     the local node_modules/.bin/vp that every just recipe runs, and are the
#     version setup-vp resolves for CI when its `version:` input is omitted.
#   - vp ships its vite fork under an npm alias, so both package files repeat
#     the same version inside a `npm:@voidzero-dev/vite-plus-core@<v>` spec.
#   - `overrides.vite` names the same package again. Renovate does not manage
#     override entries (the others are transitive-only security floors that
#     npm rejects a per-entry bump for), so that copy is the one no dependency
#     update moves. npm refuses the whole tree with EOVERRIDE when an override
#     disagrees with a direct dependency on the same package, which fails the
#     Renovate artifacts step and then every job that installs. Expressing it
#     as npm's `$vite` reference makes the two impossible to desync; this
#     guard keeps it in that form.
#
# This guard covers only the two package.json copies of the vite-plus
# version, the vite alias spec each carries, and the overrides.vite
# reference. node no longer has a copy here: it is pinned once in mise.toml
# and tool-pin-drift.sh rejects any workflow re-pin of it.
set -euo pipefail

fail=0
err() {
  echo "::error file=$1::$2" >&2
  fail=1
}

# Anchored on both ends deliberately. A trailing-wildcard glob is a prefix
# match, not an exact one: it accepts `0.2.6-beta`, `0x.2.6`, and `0.2.6.7`.
# Every pin below is compared against this reference, so a malformed value
# admitted here is a malformed value the whole guard then agrees with.
#
# Rejecting prereleases is a policy, not a resolution constraint: npm
# publishes prereleases and resolves them fine, and this repo depends on one
# (`react-data-grid` is pinned to a beta in frontend/package.json). The build
# toolchain is held to stable releases because a prerelease vp or node is a
# deliberate temporary decision that should be reviewed rather than waved
# through by a drift guard. A runtime dependency and the toolchain that builds
# it carry different risk.
exact_version() {
  [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

# Report a value the shape check rejected, naming a prerelease as itself. The
# generic wording sends a maintainer who deliberately pinned one looking for a
# parse bug rather than at the policy above.
#
# The prerelease arm matches semver's prerelease separator, a `-` after three
# numeric components, not a bare hyphen. `case` arms are ordered, so `*-*`
# would win for `0-2-6` and for the `npm:@voidzero-dev/vite-plus-core@0.2.6`
# alias spec both package files already carry, answering a malformed pin with
# an instruction to remove a prerelease that was never there.
shape_hint() {
  case "$1" in
    [0-9]*.[0-9]*.[0-9]*-*) echo "prereleases are rejected by policy; pin a stable release" ;;
    "" | null) echo "the pin is absent" ;;
    *) echo "expected an exact x.y.z version" ;;
  esac
}

# The root vite-plus devDependency is the reference: it is the vp the local
# recipes actually execute, so every other copy is checked against it.
ref="$(jq -r '.devDependencies["vite-plus"] // ""' package.json)"
if ! exact_version "$ref"; then
  err package.json "the vite-plus devDependency version is '${ref}': $(shape_hint "$ref"). If the dependency shape itself changed, update this guard"
  exit 1
fi

for f in package.json frontend/package.json; do
  pkg="$(jq -r '.devDependencies["vite-plus"] // ""' "$f")"
  if [ "$pkg" != "$ref" ]; then
    err "$f" "vite-plus (${pkg}) != the root package.json pin (${ref}); the grouped vite-plus Renovate PR moves every copy together"
  fi
  spec="$(jq -r '.devDependencies.vite // ""' "$f")"
  aliased="${spec#npm:@voidzero-dev/vite-plus-core@}"
  if [ "$aliased" = "$spec" ]; then
    err "$f" "the vite devDependency is '${spec}', not a vite-plus-core alias spec; update this guard if vp stopped aliasing vite"
  elif [ "$aliased" != "$ref" ]; then
    err "$f" "the vite alias pins ${aliased} but vite-plus pins ${ref}; npm would resolve a core the vp CLI was not released against"
  fi
done

override="$(jq -r '.overrides.vite // ""' package.json)"
# "\$vite" is npm's reference syntax for the direct dependency's spec, not a
# shell variable; escaping it inside double quotes keeps that literal.
if [ "$override" != "\$vite" ]; then
  err package.json "overrides.vite is '${override}', not the '\$vite' reference to the direct dependency; a literal spec lags the next vite-plus bump and npm then rejects the tree with EOVERRIDE"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "vp pins agree (${ref}) across both package.json files, the vite alias, and the npm override"
