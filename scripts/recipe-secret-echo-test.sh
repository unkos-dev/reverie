#!/usr/bin/env bash
# Guard: overridden DB credentials never echo into terminal/CI logs.
#
# DB recipes inject DSNs via shell parameter expansion (${VAR:-default}),
# which just echoes unexpanded; a just-variable DSN would be interpolated
# into the echoed recipe line before the shell runs. Set both override vars
# to a sentinel credential and assert it appears nowhere in the dry-run
# output (dry-run prints recipe lines exactly as real runs echo them).
set -euo pipefail

cd "$(dirname "$0")/.."

sentinel='S3CRET-sentinel-credential'
recipes=(rust::test rust::cov rust::sqlx-check rust::doctests db-migrate)

output="$(
  REVERIE_DEV_DB_URL="postgres://leak:${sentinel}@x/x" \
    DATABASE_URL_MIGRATION="postgres://leak:${sentinel}@x/x" \
    just --dry-run "${recipes[@]}" 2>&1
)"

if printf '%s' "$output" | grep -qF "$sentinel"; then
  echo "FAIL: sentinel credential leaked into recipe echo output:" >&2
  printf '%s' "$output" | grep -nF "$sentinel" >&2
  exit 1
fi

echo "OK: overridden DSN credentials do not echo (${recipes[*]})"
