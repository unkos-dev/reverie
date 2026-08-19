#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
lifecycle="${root}/scripts/dev-server.sh"
tmp="$(mktemp -d)"

# Derived from the checkout path rather than fixed. A literal port made two
# checkouts running this at once collide, and the precondition below then
# reported the second one as a failure of the tree it was testing. Hashing the
# path gives each worktree its own port and keeps reruns from one checkout
# reproducible. A collision within a single checkout is a different thing,
# usually a fake server an earlier run leaked, and still fails loudly below.
# The band is where the old literal sat.
port=$((45000 + $(cksum <<<"${root}" | cut -d' ' -f1) % 1000))

# The identity-mismatch case leaves a live foreign group behind on purpose;
# every group this test starts is torn down here.
# shellcheck disable=SC2329  # invoked via the EXIT trap
cleanup() {
  for pgid_file in "${tmp}"/*.cleanup-pgid; do
    [ -f "$pgid_file" ] || continue
    kill -KILL -- "-$(cat "$pgid_file")" 2>/dev/null || true
  done
  rm -rf "${tmp}"
}
trap cleanup EXIT

if (exec 3<> "/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then
  echo "FAIL precondition: scratch port ${port} is already in use"
  exit 1
fi

cat >"${tmp}/fake-server.mjs" <<'EOF'
import { createServer } from "node:http";
createServer((req, res) => res.end("ok")).listen(Number(process.argv[2]), "127.0.0.1");
EOF
printf 'setInterval(() => {}, 1000);\n' >"${tmp}/fake-hang.mjs"

export DEV_SERVER_DIR="$tmp"
export DEV_SERVER_PORT="$port"
export DEV_SERVER_CMD="node fake-server.mjs ${port}"
export DEV_SERVER_START_TICKS=20
export DEV_SERVER_STOP_TICKS=10

fail=0
expect() {
  local name="$1" want="$2" needle="$3" got=0 out
  shift 3
  out="$("$lifecycle" "$@" 2>&1)" || got=$?
  if [ "$got" -ne "$want" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${got}; output: ${out}"
    fail=1
  elif [ -n "$needle" ] && ! grep -qF "$needle" <<<"$out"; then
    echo "FAIL ${name}: output missing '${needle}'; output: ${out}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

assert() {
  local name="$1"
  shift
  if "$@"; then
    echo "ok   ${name}"
  else
    echo "FAIL ${name}"
    fail=1
  fi
}

port_serving() {
  (exec 3<> "/dev/tcp/127.0.0.1/${port}") 2>/dev/null
}

# The bracket keeps the pattern from matching this script's own arguments.
# shellcheck disable=SC2329  # invoked indirectly through assert
no_hang_server() {
  ! pgrep -f 'fake-hang[.]mjs' >/dev/null
}

expect "status while down exits 1" 1 "dev server is down" status
expect "cold start succeeds" 0 "dev server running" start
assert "start leaves a pidfile" test -s "${tmp}/.dev-server.pid"
expect "restart is idempotent" 0 "already running" start
expect "status while up exits 0" 0 "running via dev-start" status
expect "stop terminates the server" 0 "dev server stopped" stop
assert "stop removes the pidfile" test ! -e "${tmp}/.dev-server.pid"
assert "stop closes the port" bash -c "! (exec 3<> /dev/tcp/127.0.0.1/${port}) 2>/dev/null"

expect "stop without pidfile is a no-op" 0 "no dev server started by dev-start" stop

# A dead pid in the pidfile must be treated as stale, not signalled.
bash -c 'echo "$$"' >"${tmp}/.dev-server.pid"
expect "status with dead-pid pidfile exits 1" 1 "stale pidfile" status
expect "stop cleans a dead-pid pidfile" 0 "removed stale pidfile" stop

# A live reused pgid whose leader is not the dev server must survive stop
# untouched; only the pidfile goes away.
(setsid bash -c "echo \"\$\$\" > ${tmp}/.dev-server.pid; exec sleep 60" &)
for _ in $(seq 1 20); do
  [ -s "${tmp}/.dev-server.pid" ] && break
  sleep 0.1
done
foreign_pgid="$(cat "${tmp}/.dev-server.pid")"
echo "$foreign_pgid" >"${tmp}/sleep.cleanup-pgid"
expect "stop refuses a non-dev-server group" 0 "removed stale pidfile" stop
assert "foreign group survives stop" kill -0 -- "-${foreign_pgid}"

# A foreign listener on the port is refused, never adopted.
(setsid bash -c "echo \"\$\$\" > ${tmp}/listener.cleanup-pgid; exec node ${tmp}/fake-server.mjs ${port}" \
  >/dev/null 2>&1 </dev/null &)
for _ in $(seq 1 20); do
  port_serving && break
  sleep 0.1
done
expect "start refuses a foreign listener" 1 "not started by dev-start" start
expect "status reports a foreign listener as not owned" 0 "not via dev-start" status
kill -KILL -- "-$(cat "${tmp}/listener.cleanup-pgid")" 2>/dev/null || true
for _ in $(seq 1 20); do
  port_serving || break
  sleep 0.1
done

# start holds a lock, so a concurrent start/stop backs off instead of racing.
(
  exec 9>"${tmp}/.dev-server.lock"
  flock 9
  sleep 3
) &
lock_holder=$!
sleep 0.3
expect "start backs off while locked" 1 "in progress" start
kill "$lock_holder" 2>/dev/null || true
wait "$lock_holder" 2>/dev/null || true

export DEV_SERVER_CMD="node --definitely-not-a-flag"
expect "startup failure reports the exit and log" 1 "exited during startup" start
assert "startup failure leaves no pidfile" test ! -e "${tmp}/.dev-server.pid"

# A server that never listens is killed at the deadline, not left running.
export DEV_SERVER_CMD="node fake-hang.mjs"
export DEV_SERVER_START_TICKS=2
expect "startup timeout fails" 1 "was killed" start
assert "startup timeout leaves no pidfile" test ! -e "${tmp}/.dev-server.pid"
# Settle before asserting: pgrep can catch the KILLed group before the
# kernel reaps it, which is not a survival.
for _ in $(seq 1 20); do
  no_hang_server && break
  sleep 0.1
done
assert "startup timeout leaves no server" no_hang_server

# A reused pgid whose leader looks like a dev server but runs somewhere else
# (another checkout, an unrelated build) must be treated as stale, never
# signalled: cmdline pattern alone is not ownership.
#
# Waiting on the pidfile alone would be racy in the wrong direction: it appears
# one statement before `cd /`, so until the exec the leader is still the bash,
# whose cwd is the server directory. Wait for the state this case is actually
# about rather than for the pidfile.
(setsid bash -c "echo \"\$\$\" > ${tmp}/.dev-server.pid; cd /; exec node ${tmp}/fake-hang.mjs" >/dev/null 2>&1 </dev/null &)
for _ in $(seq 1 40); do
  [ -s "${tmp}/.dev-server.pid" ] &&
    [ "$(readlink "/proc/$(cat "${tmp}/.dev-server.pid")/cwd" 2>/dev/null)" = "/" ] && break
  sleep 0.1
done
cp "${tmp}/.dev-server.pid" "${tmp}/elsewhere.cleanup-pgid"
assert "elsewhere fixture reached its foreign working directory" \
  test "$(readlink "/proc/$(cat "${tmp}/elsewhere.cleanup-pgid")/cwd" 2>/dev/null)" = "/"
expect "stop refuses a pattern-matching group rooted elsewhere" 0 "removed stale pidfile" stop
assert "group rooted elsewhere survives stop" kill -0 -- "-$(cat "${tmp}/elsewhere.cleanup-pgid")"
kill -KILL -- "-$(cat "${tmp}/elsewhere.cleanup-pgid")" 2>/dev/null || true

# Presents as a genuinely owned but non-serving server: cd first, because the
# owned-alive check requires the leader to be rooted in the server directory.
# shellcheck disable=SC2329  # invoked below, after each hint expectation
start_owned_hang() {
  (setsid bash -c "cd ${tmp}; echo \"\$\$\" > .dev-server.pid; exec node ${tmp}/fake-hang.mjs" >/dev/null 2>&1 </dev/null &)
  for _ in $(seq 1 40); do
    [ -s "${tmp}/.dev-server.pid" ] && break
    sleep 0.1
  done
  cp "${tmp}/.dev-server.pid" "${tmp}/$1.cleanup-pgid"
}

# Advice strings are caller-parameterized so one script serves both planes'
# recipes. Both the defaults and an override are asserted: a typo in either
# `:-` default would otherwise ship silently, pointing frontend users at a
# recipe that does not exist.
expect "stop no-op advice defaults to the js foreground recipe" 0 "just js::dev" stop
start_owned_hang js-hint
expect "not-serving status advice defaults to the js stop recipe" 1 "just js::dev-stop" status
kill -KILL -- "-$(cat "${tmp}/js-hint.cleanup-pgid")" 2>/dev/null || true
rm -f "${tmp}/.dev-server.pid"

export DEV_SERVER_STOP_HINT="just rust::dev-stop"
export DEV_SERVER_FG_HINT="just rust::dev"
expect "stop no-op advice uses the caller's foreground hint" 0 "just rust::dev" stop
start_owned_hang rust-hint
expect "not-serving status advice uses the caller's stop hint" 1 "just rust::dev-stop" status
kill -KILL -- "-$(cat "${tmp}/rust-hint.cleanup-pgid")" 2>/dev/null || true
rm -f "${tmp}/.dev-server.pid"

exit "$fail"
