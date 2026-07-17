#!/usr/bin/env bash
# Guard: docker/compose.dev.yml security and mount invariants.
#
# Asserts the loopback-only port bind (Docker-published ports bypass host
# firewalls, and the dev cluster uses trivial passwords), the digest-pinned
# postgres image, dev/CI image parity (the CI backend job's service
# container must run the exact image compose runs, or "tests passed in CI"
# stops implying "tests pass locally"), and the postgres:18 parent volume
# mount (/var/lib/postgresql, not the legacy .../data child, which errors
# at first init on 18+).
set -euo pipefail

cd "$(dirname "$0")/.."

compose=docker/compose.dev.yml
ci_workflow=.github/workflows/ci.yml
fail=0

port="$(yq '.services.postgres.ports[0]' "$compose")"
if [ "$port" != "127.0.0.1:5432:5432" ]; then
  echo "FAIL: ${compose}: port mapping is '${port}', expected '127.0.0.1:5432:5432' (loopback-only bind)" >&2
  fail=1
fi

image="$(yq '.services.postgres.image' "$compose")"
case "$image" in
postgres:18@sha256:*)
  digest="${image#postgres:18@sha256:}"
  if ! printf '%s' "$digest" | grep -qE '^[0-9a-f]{64}$'; then
    echo "FAIL: ${compose}: image digest '${digest}' is not 64 lowercase hex characters" >&2
    fail=1
  fi
  ;;
*)
  echo "FAIL: ${compose}: image is '${image}', expected a digest-pinned 'postgres:18@sha256:...'" >&2
  fail=1
  ;;
esac

ci_image="$(yq '.jobs.backend.services.postgres.image' "$ci_workflow")"
if [ "$ci_image" != "$image" ]; then
  echo "FAIL: dev/CI postgres image parity broken: ${compose} runs '${image}' but ${ci_workflow} backend job runs '${ci_image}'; bump both together" >&2
  fail=1
fi

volume="$(yq '.services.postgres.volumes[0]' "$compose")"
if [ "$volume" != "pgdata:/var/lib/postgresql" ]; then
  echo "FAIL: ${compose}: data volume is '${volume}', expected 'pgdata:/var/lib/postgresql' (postgres:18 mounts the parent)" >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "OK: ${compose} invariants hold (loopback bind, digest pin, dev/CI image parity, parent mount)"
