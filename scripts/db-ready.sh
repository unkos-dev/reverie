#!/usr/bin/env bash
# Probe: is the dev Postgres already serving on its unix socket? Exits 0
# iff a SELECT 1 succeeds as the schema-owner role over the socket that
# docker/compose.dev.yml bind-mounts to the host.
#
# `just db-up` runs this first and skips the docker CLI entirely when the
# stack is already up, which keeps the db-up gate lane runnable inside a
# network-isolated dev sandbox (the sandbox blocks TCP loopback and the
# docker socket, but not AF_UNIX connects). Every not-ready condition,
# including a missing psql binary, reports failure so db-up falls through
# to `docker compose up`; the probe must never claim readiness it did not
# observe.
set -ueo pipefail

sock_dir="${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock"

command -v psql >/dev/null 2>&1 || exit 1

exec psql "postgres:///reverie_dev?host=${sock_dir}&user=reverie&password=reverie&connect_timeout=2" \
  -Atc 'SELECT 1' >/dev/null 2>&1
