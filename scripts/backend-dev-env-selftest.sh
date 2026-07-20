#!/usr/bin/env bash
# Exercise scripts/backend-dev-env.sh against fixture `.env` files.
#
# The property under test is the one the recipes get wrong if it regresses:
# a value the developer put in `.env` must reach the server, which means the
# helper must NOT export a default over it. dotenvy does not overwrite an
# already-set variable, so an exported default is an override, not a default.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd -P)"
helper="${root}/scripts/backend-dev-env.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

fail=0

# Source the helper in a clean subshell rooted at DIR and print one variable's
# post-resolution state: its value, or the marker for "deliberately unset".
resolve() {
  local dir="$1" var="$2"
  shift 2
  # shellcheck disable=SC2016  # $1/$2/$3 are the inner shell's own arguments
  env -i PATH="$PATH" HOME="$HOME" "$@" bash -c '
    cd "$1" || exit 1
    set -ueo pipefail
    # shellcheck source=/dev/null
    source "$2" || exit 1
    if [ -z "${!3+set}" ]; then echo "<unset>"; else echo "${!3}"; fi
  ' _ "$dir" "$helper" "$var"
}

check() {
  local name="$1" want="$2" got="$3"
  if [ "$got" = "$want" ]; then
    echo "ok   ${name}"
  else
    echo "FAIL ${name}: expected '${want}', got '${got}'"
    fail=1
  fi
}

bare="${tmp}/bare"
mkdir -p "${bare}/backend"

dotenv="${tmp}/dotenv"
mkdir -p "${dotenv}/backend"
cat >"${dotenv}/.env" <<'EOF'
DATABASE_URL=postgres://someone_else:pw@db.example:5432/other
REVERIE_PUBLIC_URL=https://reverie.example.com/
REVERIE_PORT="3101"
EOF

exported="${tmp}/exported"
mkdir -p "$exported"
printf 'export DATABASE_URL=postgres://exported:pw@localhost/x\n' >"${exported}/.env"

badport="${tmp}/badport"
mkdir -p "$badport"
printf 'REVERIE_PORT=not-a-port\n' >"${badport}/.env"

# No `.env` anywhere: the dev defaults are what make `just dev-up` work on a
# fresh clone, so they must still be exported.
check "default DSN with no .env" \
  "postgres://reverie_app:reverie_app@localhost:5432/reverie_dev" \
  "$(resolve "$bare" DATABASE_URL)"
check "default public URL is this server's own origin" \
  "http://localhost:3000" "$(resolve "$bare" REVERIE_PUBLIC_URL)"
check "default port with no .env" "3000" "$(resolve "$bare" REVERIE_PORT)"

# The regression this file exists for: a `.env` value must be left alone.
check "a .env DSN is left for dotenvy, not overridden" \
  "<unset>" "$(resolve "$dotenv" DATABASE_URL)"
check "a .env public URL is left for dotenvy, not overridden" \
  "<unset>" "$(resolve "$dotenv" REVERIE_PUBLIC_URL)"
check "the export form is recognised as a definition" \
  "<unset>" "$(resolve "$exported" DATABASE_URL)"

# dotenvy walks up from the working directory, so the recipes' backend/ CWD
# must still find the repository-root `.env`.
check "a parent directory's .env is found from backend/" \
  "<unset>" "$(resolve "${dotenv}/backend" DATABASE_URL)"
check "the fresh-clone default still applies from backend/" \
  "postgres://reverie_app:reverie_app@localhost:5432/reverie_dev" \
  "$(resolve "${bare}/backend" DATABASE_URL)"

# The port is the exception: the readiness probe and the server must agree on
# one resolved value, so a `.env` port is read and re-exported rather than
# left implicit.
check "a .env port is resolved for the probe, quotes stripped" \
  "3101" "$(resolve "$dotenv" REVERIE_PORT)"
check "an environment port wins over .env" \
  "3202" "$(resolve "$dotenv" REVERIE_PORT REVERIE_PORT=3202)"
check "an environment DSN wins over .env" \
  "postgres://env:pw@localhost/x" \
  "$(resolve "$dotenv" DATABASE_URL DATABASE_URL=postgres://env:pw@localhost/x)"

# Guessing 3000 here would probe the wrong port and then kill a healthy server
# at the start deadline, so an unparsable port must fail loudly.
if resolve "$badport" REVERIE_PORT >/dev/null 2>&1; then
  echo "FAIL an unparsable .env port is rejected: expected failure, got success"
  fail=1
else
  echo "ok   an unparsable .env port is rejected"
fi
if resolve "$bare" REVERIE_PORT REVERIE_PORT=99999 >/dev/null 2>&1; then
  echo "FAIL an out-of-range port is rejected: expected failure, got success"
  fail=1
else
  echo "ok   an out-of-range port is rejected"
fi

exit "$fail"
