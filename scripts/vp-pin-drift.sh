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

WORKFLOWS=(
  .github/workflows/ci.yml
  .github/workflows/docs-build.yml
  .github/workflows/scheduled-audit.yml
)

fail=0
err() {
  echo "::error file=$1::$2" >&2
  fail=1
}

# The root vite-plus devDependency is the reference: it is the vp the local
# recipes actually execute, so every other copy is checked against it.
ref="$(jq -r '.devDependencies["vite-plus"] // ""' package.json)"
case "$ref" in
  [0-9]*.[0-9]*) ;;
  *)
    err package.json "could not parse the vite-plus devDependency version (got '${ref}'); the dependency shape may have changed, so update this guard"
    exit 1
    ;;
esac

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

node_ref=""
for f in "${WORKFLOWS[@]}"; do
  vp="$(yq '.env.VP_VERSION' "$f")"
  node_pin="$(yq '.env.NODE_VERSION' "$f")"
  if [ "$vp" != "$ref" ]; then
    err "$f" "VP_VERSION (${vp}) != vite-plus in package.json (${ref}); keep every workflow's vp pin in lockstep"
  fi
  if [ -z "$node_ref" ]; then
    node_ref="$node_pin"
  elif [ "$node_pin" != "$node_ref" ]; then
    err "$f" "NODE_VERSION (${node_pin}) != the pin in the earlier workflows (${node_ref}); keep the node pins in lockstep"
  fi
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "vp pins agree (${ref}) across both package.json files, the vite alias, the npm override, and ${#WORKFLOWS[@]} workflows; node pins (${node_ref}) consistent"
