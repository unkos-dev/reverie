#!/usr/bin/env bash
# Exercise scripts/backend-dev-env.sh against fixture env files.
#
# The property under test is the one the recipes get wrong if it regresses:
# precedence must be strictly environment, then the REVERIE_DEV_ENV file, then
# the dev default. Getting that backwards silently clobbers whatever the
# developer already had set. The suite never reads the real
# ~/reverie/dev/env: every resolve() call runs with HOME pointed at an empty
# directory under mktemp, so the built-in default path never resolves to a
# real file unless a case explicitly overrides REVERIE_DEV_ENV.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd -P)"
helper="${root}/scripts/backend-dev-env.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

fail=0

fake_home="${tmp}/fake-home"
mkdir -p "$fake_home"

# Source the helper in a clean environment and print one variable's
# post-resolution state: its value, or the marker for "deliberately unset".
# Extra "$@" entries are additional KEY=value pairs placed in the sourcing
# environment (e.g. REVERIE_DEV_ENV=... or an override to test precedence).
# REVERIE_DEV_ENV is deliberately left unset here so the "no file" cases
# exercise the genuine default-path-absent behaviour, not the explicit-path
# failure mode; HOME alone keeps them off the real ~/reverie/dev/env.
resolve() {
  local var="$1"
  shift
  # shellcheck disable=SC2016  # $1/$2 are the inner shell's own arguments
  env -i PATH="$PATH" HOME="$fake_home" "$@" bash -c '
    set -ueo pipefail
    # shellcheck source=/dev/null
    source "$1" || exit 1
    if [ -z "${!2+set}" ]; then echo "<unset>"; else echo "${!2}"; fi
  ' _ "$helper" "$var"
}

# Like resolve(), but reports the exit status instead of a value; used for the
# cases that must fail loudly rather than resolve to anything.
resolve_status() {
  # shellcheck disable=SC2016  # $1 is the inner shell's own argument
  env -i PATH="$PATH" HOME="$fake_home" "$@" bash -c '
    set -ueo pipefail
    # shellcheck source=/dev/null
    source "$1"
  ' _ "$helper" >/dev/null 2>&1
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

fixture="${tmp}/fixture"
cat >"$fixture" <<'EOF'
# a comment line, and a blank line below

DATABASE_URL=postgres://someone_else:pw@db.example:5432/other
REVERIE_PUBLIC_URL="https://reverie.example.com/"
REVERIE_PORT='3101'
DATABASE_URL_MIGRATION=postgres://custom_migrator:pw@localhost:5432/custom
EOF

exported_fixture="${tmp}/exported-fixture"
printf 'export DATABASE_URL=postgres://exported:pw@localhost/x\n' >"$exported_fixture"

last_wins_fixture="${tmp}/last-wins-fixture"
printf 'DATABASE_URL=first\nDATABASE_URL=second\n' >"$last_wins_fixture"

badport_fixture="${tmp}/badport-fixture"
printf 'REVERIE_PORT=not-a-port\n' >"$badport_fixture"

# A key with no recipe-known dev default (the regression this suite exists to
# catch: the file pass must export every key it parses, not only the handful
# dev_env_default names).
arbitrary_key_fixture="${tmp}/arbitrary-key-fixture"
printf 'REVERIE_LIBRARY_PATH=/somewhere\n' >"$arbitrary_key_fixture"

# Lines that are not a valid KEY=value assignment must be skipped, not fail
# the source: a space in the key and a leading digit are both invalid shell
# identifiers.
garbage_fixture="${tmp}/garbage-fixture"
printf 'not a var=x\n2FOO=bar\nDATABASE_URL=postgres://after-garbage/x\n' >"$garbage_fixture"

# No file at REVERIE_DEV_ENV (the default-path-absent case): the dev defaults
# make a fresh clone work out of the box.
check "default DSN with no file" \
  "postgres://reverie_app:reverie_app@localhost:5432/reverie_dev" \
  "$(resolve DATABASE_URL)"
check "default public URL is this server's own origin" \
  "http://localhost:3000" "$(resolve REVERIE_PUBLIC_URL)"
check "default port with no file" "3000" "$(resolve REVERIE_PORT)"
check "default migration DSN with no file" \
  "postgres://reverie_migrator:reverie_migrator@localhost:5432/reverie_dev" \
  "$(resolve DATABASE_URL_MIGRATION)"

# A fixture file's values are exported (not merely left for something else to
# apply, since dotenvy is gone and this script is now the only loader).
check "a file DSN is exported" \
  "postgres://someone_else:pw@db.example:5432/other" \
  "$(resolve DATABASE_URL REVERIE_DEV_ENV="$fixture")"
check "a file public URL is exported, quotes stripped" \
  "https://reverie.example.com/" \
  "$(resolve REVERIE_PUBLIC_URL REVERIE_DEV_ENV="$fixture")"
check "a file migration DSN is exported" \
  "postgres://custom_migrator:pw@localhost:5432/custom" \
  "$(resolve DATABASE_URL_MIGRATION REVERIE_DEV_ENV="$fixture")"
check "the export form is accepted" \
  "postgres://exported:pw@localhost/x" \
  "$(resolve DATABASE_URL REVERIE_DEV_ENV="$exported_fixture")"
check "last assignment for a key wins" \
  "second" "$(resolve DATABASE_URL REVERIE_DEV_ENV="$last_wins_fixture")"

# A key with no dev-default call is still exported from the file: the file
# pass, not dev_env_default, is what makes every file key reach the server.
check "a file key with no dev default is exported" \
  "/somewhere" \
  "$(resolve REVERIE_LIBRARY_PATH REVERIE_DEV_ENV="$arbitrary_key_fixture")"

# Environment wins over the file, and over the default.
check "an environment DSN wins over the file" \
  "postgres://env:pw@localhost/x" \
  "$(resolve DATABASE_URL REVERIE_DEV_ENV="$fixture" DATABASE_URL=postgres://env:pw@localhost/x)"
check "an environment port wins over the file" \
  "3202" "$(resolve REVERIE_PORT REVERIE_DEV_ENV="$fixture" REVERIE_PORT=3202)"
check "an environment value wins over a file key with no dev default" \
  "/fromenv" \
  "$(resolve REVERIE_LIBRARY_PATH REVERIE_DEV_ENV="$arbitrary_key_fixture" REVERIE_LIBRARY_PATH=/fromenv)"

# A garbage line (invalid shell identifier as the key) is skipped rather than
# aborting the source, and a valid assignment after it still resolves.
check "an invalid-identifier line is ignored, later valid lines still resolve" \
  "postgres://after-garbage/x" \
  "$(resolve DATABASE_URL REVERIE_DEV_ENV="$garbage_fixture")"
if ! resolve_status REVERIE_DEV_ENV="$garbage_fixture"; then
  echo "FAIL a garbage line does not fail the source: expected success, got failure"
  fail=1
else
  echo "ok   a garbage line does not fail the source"
fi

# The port is read from the file and re-exported (not merely left alone),
# because the readiness probe and the server must agree on one resolved
# value.
check "a file port is resolved for the probe, quotes stripped" \
  "3101" "$(resolve REVERIE_PORT REVERIE_DEV_ENV="$fixture")"

# Guessing 3000 here would probe the wrong port and then kill a healthy server
# at the start deadline, so an unparsable or out-of-range port must fail
# loudly rather than fall back to a default.
if resolve_status REVERIE_DEV_ENV="$badport_fixture"; then
  echo "FAIL an unparsable port is rejected: expected failure, got success"
  fail=1
else
  echo "ok   an unparsable port is rejected"
fi
if resolve_status REVERIE_PORT=99999; then
  echo "FAIL an out-of-range port is rejected: expected failure, got success"
  fail=1
else
  echo "ok   an out-of-range port is rejected"
fi

# An explicitly set REVERIE_DEV_ENV naming a missing file is a misconfiguration
# (a typo'd path silently falling back to defaults would be worse than a loud
# failure), distinct from the default path simply being absent.
if resolve_status REVERIE_DEV_ENV="${tmp}/typo-does-not-exist"; then
  echo "FAIL an explicitly-set but missing REVERIE_DEV_ENV is rejected: expected failure, got success"
  fail=1
else
  echo "ok   an explicitly-set but missing REVERIE_DEV_ENV is rejected"
fi

exit "$fail"
