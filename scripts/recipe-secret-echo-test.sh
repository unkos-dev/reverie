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
#
# Each recipe is dry-run in its own just invocation. Several of these
# recipes are variadic, and a single `just --dry-run a b c` call lets the
# first variadic recipe consume the remaining names as its arguments, so
# only one recipe line would ever be expanded and asserted. Every
# per-recipe dry-run must produce output; a recipe that expands to nothing
# has asserted nothing and fails the guard.
set -euo pipefail

cd "$(dirname "$0")/.."

sentinel='S3CRET-sentinel-credential'
recipes=(
  rust::test rust::cov rust::sqlx-check rust::doctests db-migrate db-migrate-raw
  rust::dev rust::dev-start rust::dev-stop rust::dev-status
)

output=''
for recipe in "${recipes[@]}"; do
  recipe_output="$(
    REVERIE_DEV_DB_URL="postgres://leak:${sentinel}@x/x" \
      DATABASE_URL_MIGRATION="postgres://leak:${sentinel}@x/x" \
      DATABASE_URL="postgres://leak:${sentinel}@x/x" \
      just --dry-run "$recipe" 2>&1
  )"
  if [ -z "$recipe_output" ]; then
    echo "FAIL: dry-run of '${recipe}' produced no output, so nothing was asserted" >&2
    exit 1
  fi
  output+="${recipe_output}"$'\n'
done

if printf '%s' "$output" | grep -qF "$sentinel"; then
  echo "FAIL: sentinel credential leaked into recipe echo output:" >&2
  printf '%s' "$output" | grep -nF "$sentinel" >&2
  exit 1
fi

# db-migrate and db-migrate-raw each inline the migrator DSN default
# (lifting it into a just variable would echo an overridden credential),
# so the copies are deliberate and nothing in just enforces their
# equality. This assertion does: both recipe lines must carry a
# byte-identical quoted ${DATABASE_URL_MIGRATION:-...} expansion.
dsn_matches="$(grep -oE '"\$\{DATABASE_URL_MIGRATION:-[^"]*"' justfile)"
dsn_count="$(printf '%s\n' "$dsn_matches" | wc -l)"
dsn_unique="$(printf '%s\n' "$dsn_matches" | sort -u | wc -l)"
if [ "$dsn_count" -ne 2 ] || [ "$dsn_unique" -ne 1 ]; then
  echo "FAIL: expected two byte-identical migrator DSN defaults in the justfile, found ${dsn_count} (${dsn_unique} distinct):" >&2
  printf '%s\n' "$dsn_matches" >&2
  exit 1
fi

echo "OK: overridden DSN credentials do not echo (${recipes[*]}); migrator DSN defaults are byte-identical"
