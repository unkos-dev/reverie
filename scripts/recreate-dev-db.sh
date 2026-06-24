#!/usr/bin/env bash
#
# Recreate the disposable reverie dev database, and grant admin afterwards.
#
# When a branch rewrites an already-applied migration, the dev database drifts
# from the tree and `reverie-dev migrate` fails on a migration-checksum
# mismatch. `recreate` drops the public schema and re-applies every migration
# from a clean slate.
#
# A recreated database has no administrator: OIDC first-login does not
# auto-promote, and the bootstrap admin-grant path is not built yet. Log in once
# to create your user row, then `promote-admin <email>` grants it.
#
# Usage:
#   ./scripts/recreate-dev-db.sh recreate [--yes] [--fixtures]
#   ./scripts/recreate-dev-db.sh promote-admin <email>
#
#   recreate          drop the public schema and re-run all migrations
#       --yes         skip the destructive-action confirmation prompt
#       --fixtures    also import the test EPUB library afterwards
#   promote-admin     set role=admin on the user with <email> (run after login)

set -euo pipefail

die() {
  printf 'recreate-dev-db: %s\n' "$*" >&2
  exit 1
}

command -v reverie-dev >/dev/null 2>&1 \
  || die "reverie-dev wrapper not on PATH (expected ~/.local/bin/reverie-dev)"

# Reset the schemas to what the migrations build from a clean slate. Superuser
# only: public is owned by pg_database_owner and reverie_migrator is not the
# database owner, so the drop runs as the postgres role. Both migration-owned
# schemas are dropped: public, and tower_sessions (its session table is a plain
# CREATE TABLE, so leaving it makes re-migration fail with "already exists").
# The regrants restore the public schema owner and the migrator CREATE privilege
# the initial migration (USAGE only) omits; migrations recreate tower_sessions.
RESET_SQL="DROP SCHEMA public CASCADE; \
DROP SCHEMA IF EXISTS tower_sessions CASCADE; \
CREATE SCHEMA public; \
ALTER SCHEMA public OWNER TO pg_database_owner; \
GRANT USAGE, CREATE ON SCHEMA public TO reverie_migrator;"

usage() {
  sed -n '/^# Usage:/,/run after login)/p' "$0" | sed 's/^# \{0,1\}//'
}

cmd_recreate() {
  local assume_yes=0 fixtures=0 arg
  for arg in "$@"; do
    case "$arg" in
      --yes) assume_yes=1 ;;
      --fixtures) fixtures=1 ;;
      *) die "unknown recreate option: $arg (try --help)" ;;
    esac
  done

  if [[ "$assume_yes" -ne 1 ]]; then
    printf 'DESTRUCTIVE: this DROPs the entire public schema on the reverie-dev '
    printf 'DB.\nUsers, device tokens, settings, and library rows are all wiped.\n'
    local reply
    read -r -p 'Type "recreate" to proceed: ' reply
    [[ "$reply" == "recreate" ]] || die "aborted"
  fi

  # Target whatever database the wrapper (and so `migrate`) uses, rather than
  # hard-coding a name that differs between dev setups.
  local db_name
  db_name="$(reverie-dev psql -tAc 'SELECT current_database();' | tr -d '[:space:]')"
  [[ -n "$db_name" ]] || die "could not determine the dev database name"

  printf '==> dropping and recreating schemas on %s (postgres superuser)\n' "$db_name"
  printf '%s\n' "$RESET_SQL" \
    | reverie-dev shell sudo -u postgres psql -d "$db_name" -v ON_ERROR_STOP=1 -f -

  printf '==> applying migrations\n'
  reverie-dev migrate

  if [[ "$fixtures" -eq 1 ]]; then
    printf '==> loading the fixture library\n'
    reverie-dev load-fixtures
  fi

  printf '\nDone. The DB is empty and has no administrator.\n'
  printf 'Next:\n'
  printf '  1. Log in once (reverie-dev edge-login, or a browser) to create your user.\n'
  printf '  2. ./scripts/recreate-dev-db.sh promote-admin <your-email>\n'
}

cmd_promote_admin() {
  local email="${1:-}"
  [[ -n "$email" ]] || die "usage: promote-admin <email>"
  [[ "$#" -le 1 ]] || die "promote-admin takes a single <email> argument"

  printf '==> granting admin to %s\n' "$email"
  # :'email' is psql's own quoting, so the address cannot break out of the
  # statement. is_child rows are excluded because chk_child_role_sync forbids an
  # admin child; a zero-row result means the email did not match (log in first).
  reverie-dev psql-rw -v ON_ERROR_STOP=1 -v email="$email" -c \
    "UPDATE public.users SET role = 'admin' \
     WHERE lower(email) = lower(:'email') AND NOT is_child \
     RETURNING id, email, role;"
  printf 'If the result shows UPDATE 0, no matching adult user exists yet.\n'
}

main() {
  local sub="${1:-}"
  [[ "$#" -gt 0 ]] && shift
  case "$sub" in
    recreate) cmd_recreate "$@" ;;
    promote-admin) cmd_promote_admin "$@" ;;
    "" | -h | --help | help) usage ;;
    *) die "unknown subcommand: $sub (try --help)" ;;
  esac
}

main "$@"
