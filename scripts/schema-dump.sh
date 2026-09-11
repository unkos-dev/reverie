#!/usr/bin/env bash
# Writes or checks backend/schema.sql, the pg_dump of a database with every
# migration applied. The dump comes from a scratch database, never reverie_dev,
# so nothing done to a developer's own database can reach it. pg_dump runs
# inside the Postgres container so its version is the server's, and the two
# header lines naming those versions are dropped.
#
# Usage: schema-dump.sh write|check
#
# REVERIE_PG_CONTAINER names the Postgres container (default: the dev compose
# service). REVERIE_PG_HOST is where sqlx-cli reaches the same server, as a
# socket directory or a hostname (default: the dev socket directory).
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

die() {
  printf 'schema-dump: %s\n' "$*" >&2
  exit 1
}

mode="${1:-}"
case "$mode" in
  write | check) ;;
  *) die "usage: $0 write|check" ;;
esac

container="${REVERIE_PG_CONTAINER:-$(docker compose -f docker/compose.dev.yml ps -q postgres)}"
[ -n "$container" ] || die "no dev Postgres container is running; start it with \`just db-up\`"
host="${REVERIE_PG_HOST:-${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock}"
db="reverie_schema_dump_$$"
dump="$(mktemp)"

psql_owner() {
  docker exec -i "$container" psql -X -q -v ON_ERROR_STOP=1 -U reverie "$@"
}

cleanup() {
  rm -f "$dump"
  psql_owner -d postgres -c "DROP DATABASE IF EXISTS $db WITH (FORCE)" || true
}
trap cleanup EXIT

psql_owner -d postgres -c "CREATE DATABASE $db TEMPLATE template0"
# From its DO block on, init-roles.sql grants per database rather than per
# cluster; the dump records the part of that it applies to schema public.
sed -n '/^DO \$\$$/,$p' docker/init-roles.sql | psql_owner -d "$db"
DATABASE_URL="postgres:///${db}?host=${host}&user=reverie_migrator&password=${REVERIE_MIGRATOR_PASSWORD:-reverie_migrator}" \
  sqlx migrate run --source backend/migrations

docker exec "$container" pg_dump --schema-only --restrict-key=reverie -U reverie -d "$db" \
  | sed -e '/^-- Dumped from database version /d' -e '/^-- Dumped by pg_dump version /d' > "$dump"
grep -q '^CREATE POLICY ' "$dump" || die "the dump defines no policy, so the migrations did not apply"

if [ "$mode" = write ]; then
  cp "$dump" backend/schema.sql
elif ! diff -u backend/schema.sql "$dump"; then
  die "backend/schema.sql does not match the migrations; run \`just rust::schema-dump\` and commit the result"
fi
