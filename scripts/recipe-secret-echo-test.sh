#!/usr/bin/env bash
# Guard: overridden DB credentials never echo into terminal/CI logs.
#
# DB recipes inject DSNs via shell parameter expansion (${VAR:-default}),
# which just echoes unexpanded; a just-variable DSN would be interpolated
# into the echoed recipe line before the shell runs. The backend dev recipes
# reach the same guarantee by the other route: their DSN resolution lives in
# scripts/backend-dev-env.sh, and just does not echo a shebang recipe's body.
# Both routes are asserted against the same sentinel, because a refactor that
# moves a DSN back onto an echoed recipe line is the regression this guard
# exists to catch.
#
# Set every override var to a sentinel credential and assert it appears
# nowhere in the dry-run output (dry-run prints recipe lines exactly as real
# runs echo them).
set -euo pipefail

cd "$(dirname "$0")/.."

sentinel='S3CRET-sentinel-credential'
recipes=(
  rust::test rust::cov rust::sqlx-check rust::doctests db-migrate
  rust::dev rust::dev-start rust::dev-stop rust::dev-status
)

output="$(
  REVERIE_DEV_DB_URL="postgres://leak:${sentinel}@x/x" \
    DATABASE_URL_MIGRATION="postgres://leak:${sentinel}@x/x" \
    DATABASE_URL="postgres://leak:${sentinel}@x/x" \
    just --dry-run "${recipes[@]}" 2>&1
)"

if printf '%s' "$output" | grep -qF "$sentinel"; then
  echo "FAIL: sentinel credential leaked into recipe echo output:" >&2
  printf '%s' "$output" | grep -nF "$sentinel" >&2
  exit 1
fi

echo "OK: overridden DSN credentials do not echo (${recipes[*]})"
