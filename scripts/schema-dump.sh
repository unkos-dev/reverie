#!/usr/bin/env bash
# Writes or checks the migrator-owned schema inside a disposable cluster.
set -euo pipefail

cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

die() {
  printf 'schema-dump: %s\n' "$*" >&2
  exit 1
}

mode="${1:-}"
case "$mode" in
  write | check) ;;
  *) die "usage: $0 write|check" ;;
esac

for input in REVERIE_PG_CONTAINER REVERIE_PG_HOST REVERIE_PG_PORT POSTGRES_PASSWORD REVERIE_MIGRATOR_PASSWORD; do
  [[ -n "${!input:-}" ]] || die "$input is required from the provisioning owner"
done
container="$REVERIE_PG_CONTAINER"
host="$REVERIE_PG_HOST"
port="$REVERIE_PG_PORT"
[[ "$host" != /* ]] || port=5432
db="reverie_schema_dump_$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
dump="$(mktemp)"
created=0

psql_owner() {
  PGPASSWORD="$POSTGRES_PASSWORD" docker exec -i -e PGPASSWORD "$container" psql -X -q -h localhost -v ON_ERROR_STOP=1 -U reverie "$@"
}

cleanup() {
  local status=$?
  trap - EXIT
  if [ "$created" = 1 ] && ! psql_owner -d postgres -c "DROP DATABASE IF EXISTS $db WITH (FORCE)" >/dev/null 2>&1; then
    printf 'schema-dump: scratch database cleanup failed\n' >&2
    [[ "$status" != 0 ]] || status=1
  fi
  if ! rm -f "$dump"; then
    [[ "$status" != 0 ]] || status=1
  fi
  exit "$status"
}
trap cleanup EXIT

psql_owner -d postgres -c "CREATE DATABASE $db TEMPLATE template0" >/dev/null 2>&1 || die "scratch database creation failed"
created=1
# From its DO block on, init-roles.sql grants per database rather than per
# cluster; the dump records the part of that it applies to schema public.
sed -n '/^DO \$\$$/,$p' docker/init-roles.sql | psql_owner -d "$db" >/dev/null 2>&1 || die "scratch database grants failed"
PGPASSWORD="$REVERIE_MIGRATOR_PASSWORD" \
  DATABASE_URL="postgres:///${db}?host=${host}&port=${port}&user=reverie_migrator" \
  sqlx migrate run --source backend/migrations >/dev/null 2>&1 || die "scratch database migration failed"

PGPASSWORD="$POSTGRES_PASSWORD" docker exec -e PGPASSWORD "$container" pg_dump --schema-only --restrict-key=reverie -h localhost -U reverie -d "$db" 2>/dev/null \
  | sed -e '/^-- Dumped from database version /d' -e '/^-- Dumped by pg_dump version /d' > "$dump" || die "scratch schema dump failed"
grep -q '^CREATE POLICY ' "$dump" || die "the dump defines no policy, so the migrations did not apply"

if [ "$mode" = write ]; then
  cp "$dump" backend/schema.sql
elif ! diff -u backend/schema.sql "$dump"; then
  die "backend/schema.sql does not match the migrations; run \`just rust::schema-dump\` and commit the result"
fi
