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

echo "OK: compose dev invariant guard rejects drift in both backend postgres pins"
