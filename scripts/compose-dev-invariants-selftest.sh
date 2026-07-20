#!/usr/bin/env bash
# Self-test the dev compose invariants against isolated fixture copies.
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

mkdir -p "$fixture/scripts" "$fixture/docker" "$fixture/.github/workflows"
cp "$repo_root/scripts/compose-dev-invariants.sh" "$fixture/scripts/"
cp "$repo_root/docker/compose.dev.yml" "$fixture/docker/"
cp "$repo_root/.github/workflows/ci.yml" "$fixture/.github/workflows/"

guard="$fixture/scripts/compose-dev-invariants.sh"
workflow="$fixture/.github/workflows/ci.yml"

"$guard" >/dev/null

assert_job_drift_fails() {
  local job=$1
  local original
  original=$(yq ".jobs.${job}.services.postgres.image" "$workflow")
  yq -i ".jobs.${job}.services.postgres.image = \"postgres:18@sha256:$(printf '0%.0s' {1..64})\"" "$workflow"
  if "$guard" >/dev/null 2>&1; then
    echo "FAIL: compose invariant guard accepted drift in the ${job} postgres image" >&2
    exit 1
  fi
  yq -i ".jobs.${job}.services.postgres.image = \"${original}\"" "$workflow"
}

assert_job_drift_fails backend
assert_job_drift_fails backend-checks

# An unpinned project name must fail: the default couples stack identity to
# the compose file's directory name.
compose_fixture="$fixture/docker/compose.dev.yml"
original_name="$(yq '.name' "$compose_fixture")"
yq -i '.name = "docker"' "$compose_fixture"
if "$guard" >/dev/null 2>&1; then
  echo "FAIL: compose invariant guard accepted a wrong project name" >&2
  exit 1
fi
yq -i 'del(.name)' "$compose_fixture"
if "$guard" >/dev/null 2>&1; then
  echo "FAIL: compose invariant guard accepted a missing project name" >&2
  exit 1
fi
# Restoring the pin must make the guard pass again, proving both failures
# above were attributable to the project name and not to unrelated drift.
name="$original_name" yq -i '.name = strenv(name)' "$compose_fixture"
"$guard" >/dev/null

# A container name unscoped from the project must fail: container_name is a
# global Docker namespace, so an alternate-env stack would collide with the
# default stack's container.
yq -i '.services.postgres.container_name = "reverie-postgres"' "$compose_fixture"
if "$guard" >/dev/null 2>&1; then
  echo "FAIL: compose invariant guard accepted an unscoped container name" >&2
  exit 1
fi

echo "OK: compose dev invariant guard rejects drift in both backend postgres pins, an unpinned project name, and an unscoped container name"
