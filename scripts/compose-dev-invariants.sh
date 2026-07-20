#!/usr/bin/env bash
# Guard: docker/compose.dev.yml security and mount invariants.
#
# Asserts the loopback-only port bind (Docker-published ports bypass host
# firewalls, and the dev cluster uses trivial passwords), the digest-pinned
# postgres image, dev/CI image parity (both CI backend jobs' service
# containers must run the exact image compose runs, or "tests passed in CI"
# stops implying "tests pass locally"), and the postgres:18 parent volume
# mount (/var/lib/postgresql, not the legacy .../data child, which errors
# at first init on 18+).
set -euo pipefail

cd "$(dirname "$0")/.."

compose=docker/compose.dev.yml
ci_workflow=.github/workflows/ci.yml
fail=0

# The project name must be pinned: the compose default (basename of the
# compose file's directory) couples every checkout's stack identity to a
# directory name, and a later rename or COMPOSE_PROJECT_NAME pin would
# collide with the running container and provision a fresh empty data
# volume that reads as data loss. The pinned expression keeps one stack
# per environment: `reverie` by default, `reverie_<env>` when
# REVERIE_COMPOSE_ENV is set.
name="$(yq '.name' "$compose")"
# shellcheck disable=SC2016  # the expected value is a compose interpolation literal
expected_name='reverie${REVERIE_COMPOSE_ENV:+_${REVERIE_COMPOSE_ENV}}'
if [ "$name" != "$expected_name" ]; then
  echo "FAIL: ${compose}: project name is '${name}', expected '${expected_name}' (pinned stack identity)" >&2
  fail=1
fi

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

for job in backend backend-checks; do
  ci_image="$(yq ".jobs.${job}.services.postgres.image" "$ci_workflow")"
  if [ "$ci_image" != "$image" ]; then
    echo "FAIL: dev/CI postgres image parity broken: ${compose} runs '${image}' but ${ci_workflow} ${job} job runs '${ci_image}'; bump all three together" >&2
    fail=1
  fi
done

volume="$(yq '.services.postgres.volumes[0]' "$compose")"
if [ "$volume" != "pgdata:/var/lib/postgresql" ]; then
  echo "FAIL: ${compose}: data volume is '${volume}', expected 'pgdata:/var/lib/postgresql' (postgres:18 mounts the parent)" >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "OK: ${compose} invariants hold (project name, loopback bind, digest pin, dev/CI image parity, parent mount)"
