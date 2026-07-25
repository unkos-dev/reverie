#!/usr/bin/env bash
# Guard: docker/compose.dev.yml security and mount invariants.
#
# Asserts the pinned project and container names (stack identity must not
# hang on the compose file's directory name), the loopback-only port bind (Docker-published ports bypass host
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
# compose file's directory) couples every checkout's stack identity, and
# the generated pgdata volume name, to a directory name that a rename
# would change out from under a running stack. The pinned expression keeps
# one stack per environment: `reverie` by default, `reverie_<env>` when
# REVERIE_COMPOSE_ENV is set.
name="$(yq '.name' "$compose")"
# shellcheck disable=SC2016  # the expected value is a compose interpolation literal
expected_name='reverie${REVERIE_COMPOSE_ENV:+_${REVERIE_COMPOSE_ENV}}'
if [ "$name" != "$expected_name" ]; then
  echo "FAIL: ${compose}: project name is '${name}', expected '${expected_name}' (pinned stack identity)" >&2
  fail=1
fi

# container_name is a global Docker namespace, unscoped by the compose
# project, so it needs the same suffix or an alternate-env stack collides
# with the default stack's container and the per-environment separation
# the pinned project buys is silently lost.
container_name="$(yq '.services.postgres.container_name' "$compose")"
# shellcheck disable=SC2016  # the expected value is a compose interpolation literal
expected_container_name='reverie-postgres${REVERIE_COMPOSE_ENV:+_${REVERIE_COMPOSE_ENV}}'
if [ "$container_name" != "$expected_container_name" ]; then
  echo "FAIL: ${compose}: container name is '${container_name}', expected '${expected_container_name}' (scoped to the compose project)" >&2
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

# The socket bind-mount serves the cluster's unix socket to host tooling:
# the DB-backed just recipes' committed DSN defaults point at this exact
# host path (see rust.just). Losing the mount silently strands those
# recipes; moving it breaks every committed socket DSN default, so both
# sides are pinned.
# shellcheck disable=SC2016  # the expected value is a compose interpolation literal
socket_mount="$(yq '.services.postgres.volumes[2]' "$compose")"
# shellcheck disable=SC2016
expected_socket_mount='${XDG_STATE_HOME:-${HOME}/.local/state}/reverie/pgsock:/var/run/postgresql'
if [ "$socket_mount" != "$expected_socket_mount" ]; then
  echo "FAIL: ${compose}: socket mount is '${socket_mount}', expected '${expected_socket_mount}' (host unix socket the DB-backed recipes connect to)" >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "OK: ${compose} invariants hold (project name, container name, loopback bind, digest pin, dev/CI image parity, parent mount, socket mount)"
