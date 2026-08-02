#!/usr/bin/env bash
# The vite-plus version is pinned in more places than one Renovate update
# reliably reaches, and the copies fail in different ways when they disagree:
#
#   - `devDependencies.vite-plus` in the root and the frontend package drive
#     the local node_modules/.bin/vp that every just recipe runs.
#   - Each workflow carries its own VP_VERSION for the global vp setup-vp
#     installs; a reusable or standalone workflow does not inherit ci.yml's
#     env, so the pins are hand-copied.
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
shape_hint() {
  case "$1" in
    *-*) echo "prereleases are rejected by policy; pin a stable release" ;;
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

# Discover the workflows to check rather than listing them: a hand-kept census
# silently excludes the next workflow someone adds, which is the one most
# likely to carry a lagging copy. A file qualifies either by installing vp or
# by declaring the pin, so neither half can hide from the other.
shopt -s nullglob
workflows=()
for f in .github/workflows/*.yml .github/workflows/*.yaml; do
  if grep -q 'voidzero-dev/setup-vp' "$f" || [ "$(yq '.env.VP_VERSION' "$f")" != "null" ]; then
    workflows+=("$f")
  fi
done
shopt -u nullglob

# Fail closed. An empty census would make the loop below a no-op and report
# success having checked nothing, so treat "found none" as the discovery
# pattern having gone stale, not as agreement.
if [ "${#workflows[@]}" -eq 0 ]; then
  echo "::error::no workflow installs vp or declares VP_VERSION, so this guard checked nothing; update the discovery pattern if setup-vp moved" >&2
  exit 1
fi

node_ref=""
node_ref_file=""
for f in "${workflows[@]}"; do
  vp="$(yq '.env.VP_VERSION' "$f")"
  node_pin="$(yq '.env.NODE_VERSION' "$f")"
  if [ "$vp" != "$ref" ]; then
    err "$f" "VP_VERSION (${vp}) != vite-plus in package.json (${ref}); every workflow that installs vp carries its own copy, so keep them in lockstep"
  fi
  # Shape-check before comparing. yq prints the string "null" for an absent
  # key, so pins missing everywhere would agree with each other and pass,
  # and setup-vp falls back to the latest LTS node when given no version:
  # the floating runtime the pin exists to prevent.
  if ! exact_version "$node_pin"; then
    err "$f" "NODE_VERSION is '${node_pin}': $(shape_hint "$node_pin"). Without a usable pin setup-vp resolves whatever node is latest LTS that day"
    continue
  fi
  if [ -z "$node_ref" ]; then
    node_ref="$node_pin"
    node_ref_file="$f"
  elif [ "$node_pin" != "$node_ref" ]; then
    err "$f" "NODE_VERSION (${node_pin}) != the pin in ${node_ref_file} (${node_ref}); keep the node pins in lockstep"
  fi
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "vp pins agree (${ref}) across both package.json files, the vite alias, the npm override, and ${#workflows[@]} discovered workflows; node pins (${node_ref}) consistent"
