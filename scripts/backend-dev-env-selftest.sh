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

stub_bin="$tmp/bin"
state_root="$tmp/state"
state_dir="$state_root/reverie/postgres/env_fixture"
mkdir -p "$stub_bin" "$state_dir"
mise_shims="${MISE_SHIMS_DIR:-$(mise settings --all --json | jq -r '.shims_dir // empty')}"
mise_shims="${mise_shims:-${MISE_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/mise}/shims}"
case "$mise_shims" in \~/*) mise_shims="$HOME/${mise_shims#\~/}" ;; esac
jq_path=''
IFS=: read -r -a path_entries <<< "$PATH"
for path_entry in "${path_entries[@]}"; do
  [[ "${path_entry%/}" == "${mise_shims%/}" ]] || jq_path+="$path_entry:"
done
jq_real="$(PATH="${jq_path%:}" command -v jq)"
ln -s "$jq_real" "$stub_bin/jq"
chmod 700 "$state_dir"
fixture_app="$(printf '%048d' 1)"
fixture_migrator="$(printf '%048d' 2)"
fixture_ingestion="$(printf '%048d' 3)"
fixture_readonly="$(printf '%048d' 4)"
fixture_bootstrap="$(printf '%048d' 5)"
printf '%s\n' "POSTGRES_PASSWORD=$fixture_bootstrap" "REVERIE_APP_PASSWORD=$fixture_app" \
  "REVERIE_MIGRATOR_PASSWORD=$fixture_migrator" "REVERIE_INGESTION_PASSWORD=$fixture_ingestion" \
  "REVERIE_READONLY_PASSWORD=$fixture_readonly" > "$state_dir/credentials.env"
chmod 600 "$state_dir/credentials.env"
cat > "$stub_bin/docker" <<'DOCKER'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  compose)
    case "$*" in
      *config*)
        [[ "${ENV_STUB_CONFIG_FAIL:-0}" == 0 ]] || exit 1
        [[ "$*" == *--no-env-resolution* ]] || exit 1
        jq -n --arg socket "$XDG_STATE_HOME/reverie/pgsock" \
          '{name:"env_fixture",volumes:{pgdata:{name:"env_fixture_pgdata"}},services:{postgres:{image:("postgres:18@sha256:" + ("0" * 64)),container_name:"env-fixture-postgres",volumes:[{target:"/var/run/postgresql",source:$socket}]}}}'
        ;;
      *"up -d --wait"*)
        [[ ! -e /proc/$$/fd/9 ]] || exit 91
        touch "$ENV_STUB_VOLUME_MARKER"
        printf 'up\n' >> "$ENV_STUB_EVENTS"
        ;;
      *down*) printf 'down\n' >> "$ENV_STUB_EVENTS" ;;
      *) exit 92 ;;
    esac
    ;;
  info) exit 0 ;;
  volume)
    case "$2" in
      inspect) [[ -e "$ENV_STUB_VOLUME_MARKER" ]] ;;
      *) exit 93 ;;
    esac
    ;;
  create) printf 'create\n' >> "$ENV_STUB_EVENTS" ;;
  start) exit 0 ;;
  inspect)
    if [[ "$*" == *HostPort* ]]; then printf '34567\n'; else printf 'fixture-container\n'; fi
    ;;
  exec)
    [[ ! -e /proc/$$/fd/9 ]] || exit 94
    [[ "${ENV_STUB_LOGIN_FAIL:-0}" == 0 ]]
    ;;
  rm)
    printf 'cleanup\n' >> "$ENV_STUB_EVENTS"
    [[ "${ENV_STUB_CLEANUP_FAIL:-0}" == 0 ]]
    ;;
  *) exit 95 ;;
esac
DOCKER
cat > "$stub_bin/psql" <<'PSQL'
#!/usr/bin/env bash
set -euo pipefail
[[ ! -e /proc/$$/fd/9 ]] || exit 94
if [[ -n "${ENV_STUB_HOST_EVENTS:-}" ]]; then
  [[ "$*" == *host=127.0.0.1* ]] || exit 96
  printf 'tcp\n' >> "$ENV_STUB_HOST_EVENTS"
fi
[[ "${ENV_STUB_HOST_LOGIN_FAIL:-0}" == 0 ]]
[[ "${ENV_STUB_LOGIN_FAIL:-0}" == 0 ]]
PSQL
chmod +x "$stub_bin/docker" "$stub_bin/psql"
test_path="$stub_bin:$PATH"

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
  env -i PATH="$test_path" TMPDIR="${TMPDIR:-/tmp}" HOME="$fake_home" XDG_STATE_HOME="$state_root" "$@" bash -c '
    set -ueo pipefail
    # shellcheck source=/dev/null
    source "$1" DATABASE_URL DATABASE_URL_INGESTION DATABASE_URL_MIGRATION || exit 1
    if [ -z "${!2+set}" ]; then echo "<unset>"; else echo "${!2}"; fi
  ' _ "$helper" "$var"
}

# Like resolve(), but reports the exit status instead of a value; used for the
# cases that must fail loudly rather than resolve to anything.
resolve_status() {
  # shellcheck disable=SC2016  # $1 is the inner shell's own argument
  env -i PATH="$test_path" TMPDIR="${TMPDIR:-/tmp}" HOME="$fake_home" XDG_STATE_HOME="$state_root" "$@" bash -c '
    set -ueo pipefail
    # shellcheck source=/dev/null
    source "$1" DATABASE_URL DATABASE_URL_INGESTION DATABASE_URL_MIGRATION
  ' _ "$helper" >/dev/null 2>&1
}

check() {
  local name="$1" want="$2" got="$3"
  if [ "$got" = "$want" ]; then
    echo "ok   ${name}"
  else
    echo "FAIL ${name}: values differ"
    fail=1
  fi
}

fixture="${tmp}/fixture"
cat >"$fixture" <<'EOF'
# a comment line, and a blank line below

DATABASE_URL=postgres://someone_else:pw@db.example:5432/other
DATABASE_URL_INGESTION=postgres://custom_ingestion:pw@localhost:5432/custom
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
  "postgres://reverie_app:$fixture_app@localhost:5432/reverie_dev" \
  "$(resolve DATABASE_URL)"
check "default public URL is this server's own origin" \
  "http://localhost:3000" "$(resolve REVERIE_PUBLIC_URL)"
check "default port with no file" "3000" "$(resolve REVERIE_PORT)"
check "default migration DSN with no file" \
  "postgres://reverie_migrator:$fixture_migrator@localhost:5432/reverie_dev" \
  "$(resolve DATABASE_URL_MIGRATION)"
check "default ingestion DSN with no file" \
  "postgres://reverie_ingestion:$fixture_ingestion@localhost:5432/reverie_dev" \
  "$(resolve DATABASE_URL_INGESTION)"
check "a file ingestion DSN is exported" \
  "postgres://custom_ingestion:pw@localhost:5432/custom" \
  "$(resolve DATABASE_URL_INGESTION REVERIE_DEV_ENV="$fixture")"
check "an environment ingestion DSN wins over the file" \
  "postgres://env_ingestion:pw@localhost/x" \
  "$(resolve DATABASE_URL_INGESTION REVERIE_DEV_ENV="$fixture" DATABASE_URL_INGESTION=postgres://env_ingestion:pw@localhost/x)"
check "explicit empty ingestion credentials are preserved" \
  "" "$(resolve DATABASE_URL_INGESTION REVERIE_DEV_ENV="$fixture" DATABASE_URL_INGESTION=)"
check "explicit whitespace ingestion credentials are preserved" \
  "   " "$(resolve DATABASE_URL_INGESTION DATABASE_URL_INGESTION='   ')"

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

check_status() {
  local name="$1" expected="$2" got="$3"
  if [[ "$expected" == "$got" ]]; then echo "ok   $name"; else echo "FAIL $name: status $got, expected $expected"; fail=1; fi
}

cp "$state_dir/credentials.env" "$tmp/saved-state"
rm "$state_dir/credentials.env"
if resolve_status; then echo 'FAIL missing state must refuse defaults'; fail=1; else echo 'ok   missing state refuses defaults'; fi
check 'fully explicit external DSNs need no local state' 'postgres://someone_else:pw@db.example:5432/other' \
  "$(resolve DATABASE_URL REVERIE_DEV_ENV="$fixture" ENV_STUB_CONFIG_FAIL=1)"
for command in migration runtime auto-migration; do
  command_status=0
  env -i PATH="$test_path" TMPDIR="${TMPDIR:-/tmp}" HOME="$fake_home" XDG_STATE_HOME="$state_root" \
    ENV_STUB_CONFIG_FAIL=1 bash -s -- "$helper" "$command" > "$tmp/external-$command.log" 2>&1 <<'EXTERNAL' || command_status=$?
case "$2" in
  migration)
    export DATABASE_URL_MIGRATION=external
    source "$1" DATABASE_URL_MIGRATION || exit 1
    [[ ! -v DATABASE_URL && ! -v DATABASE_URL_INGESTION && "$DATABASE_URL_MIGRATION" == external ]]
    ;;
  runtime | auto-migration)
    export DATABASE_URL=external DATABASE_URL_INGESTION=external
    [[ "$2" != auto-migration ]] || export REVERIE_AUTO_MIGRATE=true
    source "$1" DATABASE_URL DATABASE_URL_INGESTION || exit 1
    [[ ! -v DATABASE_URL_MIGRATION ]]
    ;;
esac
EXTERNAL
  if [[ "$command" == auto-migration ]]; then
    check_status 'automatic migration still requires a migration DSN' 1 "$command_status"
  else
    check_status "external $command needs only its own DSNs" 0 "$command_status"
  fi
done
printf 'POSTGRES_PASSWORD=private-credential-marker\n' > "$state_dir/credentials.env"
chmod 600 "$state_dir/credentials.env"
malformed_output="$(resolve DATABASE_URL 2>&1)" && malformed_status=0 || malformed_status=$?
[[ "$malformed_status" != 0 && "$malformed_output" == *malformed* && "$malformed_output" != *private-credential-marker* ]] || {
  echo 'FAIL malformed state diagnostics'; fail=1;
}
cp "$tmp/saved-state" "$state_dir/credentials.env"
for command in runtime migration auto-migration; do
  for password_state in absent local exported empty; do
    consumer_status=0
    env -i PATH="$test_path" TMPDIR="${TMPDIR:-/tmp}" HOME="$fake_home" XDG_STATE_HOME="$state_root" \
      bash -s -- "$helper" "$command" "$password_state" "$fixture_bootstrap" \
      > "$tmp/consumer-$command-$password_state.log" 2>&1 <<'CONSUMER' || consumer_status=$?
set -euo pipefail
passwords=(POSTGRES_PASSWORD REVERIE_APP_PASSWORD REVERIE_MIGRATOR_PASSWORD REVERIE_INGESTION_PASSWORD REVERIE_READONLY_PASSWORD)
declare -A before=()
for key in "${passwords[@]}"; do
  case "$3" in
    absent) ;;
    local | exported)
      printf -v "$key" '%s' developer-supplied-password
      [[ "$key" != POSTGRES_PASSWORD ]] || POSTGRES_PASSWORD="$4"
      [[ "$3" != exported ]] || export "$key"
      ;;
    empty)
      [[ "$key" != POSTGRES_PASSWORD ]] || continue
      export "$key="
      ;;
  esac
  before["$key"]="$(declare -p "$key" 2>/dev/null || true)"
done
if [[ "$2" == migration ]]; then
  source "$1" DATABASE_URL_MIGRATION
  [[ "$DATABASE_URL_MIGRATION" == "postgres:///reverie_dev?host=$XDG_STATE_HOME/reverie/pgsock&user=reverie_migrator&password="* ]]
  [[ ! -v DATABASE_URL && ! -v DATABASE_URL_INGESTION ]]
else
  [[ "$2" != auto-migration ]] || export REVERIE_AUTO_MIGRATE=true
  source "$1" DATABASE_URL DATABASE_URL_INGESTION
  [[ "$DATABASE_URL" == postgres://reverie_app:*@localhost:5432/reverie_dev ]]
  [[ "$DATABASE_URL_INGESTION" == postgres://reverie_ingestion:*@localhost:5432/reverie_dev ]]
  if [[ "$2" == auto-migration ]]; then
    [[ "$DATABASE_URL_MIGRATION" == postgres://reverie_migrator:*@localhost:5432/reverie_dev ]]
  else
    [[ ! -v DATABASE_URL_MIGRATION ]]
  fi
fi
for key in "${passwords[@]}"; do
  [[ "$(declare -p "$key" 2>/dev/null || true)" == "${before[$key]-}" ]]
done
CONSUMER
    check_status "$command preserves $password_state password inputs without adding credentials" 0 "$consumer_status"
  done
done
for variable in DATABASE_URL DATABASE_URL_INGESTION DATABASE_URL_MIGRATION; do
  check "explicit empty $variable" '' "$(resolve "$variable" "$variable=")"
done
if resolve_status POSTGRES_PASSWORD=private-bootstrap-marker; then
  echo 'FAIL bootstrap override conflicts'; fail=1
else
  echo 'ok   conflicting bootstrap override refuses state'
fi

lifecycle_state="$tmp/lifecycle-state"
lifecycle_events="$tmp/lifecycle.events"
volume_marker="$tmp/volume"
lifecycle() {
  env -i PATH="$test_path" TMPDIR="${TMPDIR:-/tmp}" HOME="$fake_home" XDG_STATE_HOME="$lifecycle_state" \
    ENV_STUB_EVENTS="$lifecycle_events" ENV_STUB_VOLUME_MARKER="$volume_marker" "$@" \
    bash "$root/scripts/postgres-provision.sh" dev-up
}
lifecycle > "$tmp/lifecycle-one.log" 2>&1 &
first=$!
lifecycle > "$tmp/lifecycle-two.log" 2>&1 &
second=$!
wait "$first" && first_status=0 || first_status=$?
wait "$second" && second_status=0 || second_status=$?
check_status 'first concurrent dev-up' 0 "$first_status"
check_status 'second concurrent dev-up reuses state' 0 "$second_status"
retained="$lifecycle_state/reverie/postgres/env_fixture/credentials.env"
cp "$retained" "$tmp/retained-before"
lifecycle > "$tmp/lifecycle-restart.log" 2>&1 && restart_status=0 || restart_status=$?
check_status 'restart uses retained credentials' 0 "$restart_status"
cmp -s "$retained" "$tmp/retained-before" || { echo 'FAIL credentials rotated'; fail=1; }
[[ "$(stat -c '%a' "$retained")" == 600 && "$(stat -c '%a' "${retained%/*}")" == 700 ]] || {
  echo 'FAIL persistent permissions'; fail=1;
}
lifecycle ENV_STUB_LOGIN_FAIL=1 > "$tmp/lifecycle-login.log" 2>&1 && login_status=0 || login_status=$?
[[ "$login_status" != 0 ]] || { echo 'FAIL failed login accepted'; fail=1; }
cmp -s "$retained" "$tmp/retained-before" || { echo 'FAIL failed login rotated credentials'; fail=1; }
lifecycle ENV_STUB_HOST_EVENTS="$tmp/host.events" > "$tmp/lifecycle-tcp.log" 2>&1 && tcp_status=0 || tcp_status=$?
check_status 'host authentication uses TCP without a socket' 0 "$tcp_status"
grep -qx tcp "$tmp/host.events" || { echo 'FAIL host TCP authentication not executed'; fail=1; }
lifecycle ENV_STUB_HOST_LOGIN_FAIL=1 > "$tmp/lifecycle-host-login.log" 2>&1 && host_status=0 || host_status=$?
check_status 'host authentication failure is not bypassed' 1 "$host_status"
no_client_status=0
env -i PATH="$test_path" bash -s -- "$root/scripts/postgres-provision.sh" "$tmp/no-client.events" > "$tmp/no-client.log" 2>&1 <<'NOCLIENT' || no_client_status=$?
source "$1"
POSTGRES_PASSWORD=fixture pg_dev_container=fixture
events="$2"
docker() { printf 'container-auth\n' > "$events"; }
PATH=/nonexistent
pg_dev_auth
NOCLIENT
check_status 'container authentication needs no host client' 0 "$no_client_status"
grep -qx container-auth "$tmp/no-client.events" || { echo 'FAIL container authentication not executed'; fail=1; }
rm "$retained"
lifecycle > "$tmp/lifecycle-missing.log" 2>&1 && missing_status=0 || missing_status=$?
[[ "$missing_status" != 0 && ! -e "$retained" && -e "$volume_marker" ]] || {
  echo 'FAIL existing volume without state regenerated or removed'; fail=1;
}

for child_status in 0 2 3 23; do
  for cleanup_failure in 0 1; do
    events="$tmp/status-$child_status-$cleanup_failure.events"
    log="$tmp/status-$child_status-$cleanup_failure.log"
    wrapper_status=0
    env -i PATH="$test_path" TMPDIR="${TMPDIR:-/tmp}" HOME="$fake_home" XDG_STATE_HOME="$state_root" \
      ENV_STUB_EVENTS="$events" ENV_STUB_CLEANUP_FAIL="$cleanup_failure" \
      bash "$root/scripts/postgres-provision.sh" test -- bash -s -- "$child_status" "$events" > "$log" 2>&1 <<'CHILD' || wrapper_status=$?
printf 'child\n' >> "$2"
exit "$1"
CHILD
    if ! grep -qx child "$events" || ! grep -qx cleanup "$events"; then
      echo 'FAIL wrapper status case did not execute child and cleanup'; fail=1
    fi
    if [[ "$child_status" == 2 || "$child_status" == 3 ]]; then
      if [[ "$cleanup_failure" == 0 ]]; then
        check_status "mutation findings $child_status without cleanup failure" "$child_status" "$wrapper_status"
      else
        check_status "mutation findings $child_status with cleanup failure" 1 "$wrapper_status"
      fi
    elif [[ "$child_status" == 23 ]]; then
      check_status "child 23, cleanup failure $cleanup_failure" 23 "$wrapper_status"
    elif [[ "$cleanup_failure" == 0 ]]; then
      check_status 'child success and cleanup success' 0 "$wrapper_status"
    elif [[ "$wrapper_status" == 0 || "$wrapper_status" == 2 || "$wrapper_status" == 3 ]]; then
      echo 'FAIL cleanup-only failure classified as success or mutation finding'; fail=1
    else
      echo 'ok   cleanup-only failure returns infrastructure status'
    fi
    if [[ "$cleanup_failure" == 1 ]] && ! grep -q 'cleanup failed' "$log"; then
      echo 'FAIL cleanup failure was not reported'; fail=1
    fi
  done
done

for cancellation in INT TERM; do
  for response in success ignore; do
    events="$tmp/cancel-$cancellation-$response.events"
    ready="$tmp/cancel-$cancellation-$response.ready"
    env --default-signal=INT,TERM -i PATH="$test_path" TMPDIR="${TMPDIR:-/tmp}" HOME="$fake_home" XDG_STATE_HOME="$state_root" \
      ENV_STUB_EVENTS="$events" \
      bash "$root/scripts/postgres-provision.sh" test -- bash -s -- "$response" "$ready" \
      > "$tmp/cancel-$cancellation-$response.log" 2>&1 <<'CANCEL' &
if [[ "$1" == ignore ]]; then trap "" INT TERM; else trap "exit 0" INT TERM; fi
printf '%s\n' "$$" > "$2"
while :; do sleep 0.1; done
CANCEL
    owner=$!
    for ((attempt=0; attempt<100; attempt++)); do
      [[ ! -s "$ready" ]] || break
      sleep 0.1
    done
    if [[ ! -s "$ready" ]]; then
      echo 'FAIL cancellation child did not start'; fail=1
      kill -TERM "$owner" 2>/dev/null || true
      wait "$owner" || true
      continue
    fi
    child_group="$(cat "$ready")"
    kill -s "$cancellation" "$owner"
    for ((attempt=0; attempt<150; attempt++)); do
      kill -0 "$owner" 2>/dev/null || break
      sleep 0.1
    done
    if kill -0 "$owner" 2>/dev/null; then
      echo 'FAIL cancellation exceeded its deadline'; fail=1
      kill -KILL -- "-$child_group" 2>/dev/null || true
    fi
    wait "$owner" && cancel_status=0 || cancel_status=$?
    if [[ "$response" == ignore ]]; then
      check_status "$cancellation stops an unresponsive child" 137 "$cancel_status"
    elif [[ "$cancellation" == INT ]]; then
      check_status 'successful child cannot hide INT cancellation' 130 "$cancel_status"
    else
      check_status 'successful child cannot hide TERM cancellation' 143 "$cancel_status"
    fi
    if kill -0 -- "-$child_group" 2>/dev/null || ! grep -qx cleanup "$events"; then
      echo 'FAIL cancelled group survived or cleanup did not execute'; fail=1
    fi
  done
done

exit "$fail"
