#!/usr/bin/env bash
# Background dev-server lifecycle: start | stop | status.
#
# The server runs in its own session (setsid) so the pidfile names a process
# group covering the whole npm/node tree; stop signals the group, because
# killing only the npm wrapper would orphan the actual Vite process. The
# leader writes its own pid before exec'ing the server command, so the
# pidfile is correct whether or not setsid had to fork.
#
# A numeric pgid can be reused by the kernel, so no path here trusts the
# pidfile alone: the group leader's /proc cmdline must also look like the
# dev server before the group is treated as ours. A mismatch is handled as
# a stale pidfile, never signalled.
#
# start and stop serialize on a lock file so two concurrent starts cannot
# both pass the free-port probe and race Vite onto different ports.
#
# The DEV_SERVER_* overrides exist for the selftest, which drives the same
# code paths against a fake server on a scratch port.
set -euo pipefail

port="${DEV_SERVER_PORT:-5173}"
start_ticks="${DEV_SERVER_START_TICKS:-60}"
stop_ticks="${DEV_SERVER_STOP_TICKS:-20}"
# strictPort makes a lost port race fatal to our Vite instead of letting it
# drift to the next free port, where the pidfile and probes would describe
# the wrong server.
cmd="${DEV_SERVER_CMD:-npm run dev -- --strictPort}"
leader_pattern="${DEV_SERVER_LEADER_PATTERN:-node|npm|vite}"
# Advice strings name the caller's recipes, so the rust plane's recipes can
# point at rust::dev-stop instead of the js defaults.
stop_hint="${DEV_SERVER_STOP_HINT:-just js::dev-stop}"
fg_hint="${DEV_SERVER_FG_HINT:-just js::dev}"

cd "${DEV_SERVER_DIR:-$(git rev-parse --show-toplevel)/frontend}" || exit 1

pidfile=.dev-server.pid
lockfile=.dev-server.lock
log=.dev-server.log
url="http://localhost:${port}/"
log_path="${PWD}/${log}"

for tool in setsid flock; do
  if ! command -v "$tool" >/dev/null; then
    echo "$tool (util-linux) is required but not on PATH" >&2
    exit 1
  fi
done

probe_port() {
  (exec 3<> "/dev/tcp/127.0.0.1/${port}") 2>/dev/null
}

# True only when the pidfile names a live process group whose leader looks
# like the dev server. Any pid that vanished, was reused by something else,
# or never parsed is reported as not-ours and cleaned up as stale.
owned_alive() {
  local pid
  [ -s "$pidfile" ] || return 1
  pid="$(cat "$pidfile")"
  kill -0 -- "-$pid" 2>/dev/null || return 1
  grep -qazE "$leader_pattern" "/proc/${pid}/cmdline" 2>/dev/null || return 1
  # A pattern match alone is not ownership: a reused pgid can land on any
  # process that resembles a dev server (any cargo invocation, any node
  # tree). The leader must also be rooted in this server directory before
  # the group is treated as ours.
  [ "$(readlink "/proc/${pid}/cwd" 2>/dev/null)" = "$(pwd -P)" ]
}

take_lock() {
  exec 9>"$lockfile"
  if ! flock -n 9; then
    echo "another dev-server start/stop is in progress; retry shortly" >&2
    exit 1
  fi
}

# TERM the group, wait, then KILL survivors. Vite normally exits promptly,
# so a survivor is wedged, and leaving it would strand the port. Returns 1
# when escalation to KILL was needed. The redundant-looking guards absorb a
# group that dies between the aliveness check and the signal.
stop_group() {
  local pid="$1"
  kill -TERM -- "-$pid" 2>/dev/null || true
  for _ in $(seq 1 "$stop_ticks"); do
    if ! kill -0 -- "-$pid" 2>/dev/null; then
      rm -f "$pidfile"
      return 0
    fi
    sleep 0.5
  done
  kill -KILL -- "-$pid" 2>/dev/null || true
  rm -f "$pidfile"
  return 1
}

do_start() {
  take_lock
  if owned_alive; then
    if probe_port; then
      echo "dev server already running (pid $(cat "$pidfile"), ${url})"
      exit 0
    fi
    echo "dev server process group $(cat "$pidfile") is alive but not serving on port ${port}; run '${stop_hint}' first" >&2
    exit 1
  fi
  rm -f "$pidfile"
  # Port probe via the bash /dev/tcp builtin: no lsof/ss dependency.
  if probe_port; then
    echo "port ${port} is already serving but was not started by dev-start; stop that process first" >&2
    exit 1
  fi
  # 9>&- keeps the long-lived server from inheriting the lock fd, which
  # would block every later start/stop for its whole lifetime.
  setsid bash -c "echo \"\$\$\" > ${pidfile}; exec ${cmd}" >"$log" 2>&1 </dev/null 9>&- &
  for _ in $(seq 1 "$start_ticks"); do
    if [ -s "$pidfile" ] && ! kill -0 -- "-$(cat "$pidfile")" 2>/dev/null; then
      echo "dev server exited during startup; last log lines:" >&2
      tail -n 20 "$log" >&2
      rm -f "$pidfile"
      exit 1
    fi
    if probe_port; then
      echo "dev server running (pid $(cat "$pidfile"), ${url}, log ${log_path})"
      exit 0
    fi
    sleep 0.5
  done
  # A server that never came up is not left half-started: kill it so the
  # failure is total and the next start begins clean.
  if [ -s "$pidfile" ]; then
    stop_group "$(cat "$pidfile")" || true
  fi
  rm -f "$pidfile"
  echo "dev server did not start listening on port ${port} within $((start_ticks / 2))s and was killed; see ${log_path}" >&2
  exit 1
}

do_stop() {
  take_lock
  if [ ! -f "$pidfile" ]; then
    echo "no dev server started by dev-start (no ${pidfile}); a foreground '${fg_hint}' stops with Ctrl-C"
    exit 0
  fi
  local pid
  pid="$(cat "$pidfile")"
  if ! owned_alive; then
    rm -f "$pidfile"
    echo "removed stale pidfile (process group ${pid} is gone or is not the dev server)"
    exit 0
  fi
  if stop_group "$pid"; then
    echo "dev server stopped (pid ${pid})"
  else
    echo "dev server killed after TERM timeout (pid ${pid})"
  fi
}

do_status() {
  if owned_alive; then
    if probe_port; then
      echo "dev server running via dev-start (pid $(cat "$pidfile"), ${url}, log ${log_path})"
      exit 0
    fi
    echo "dev server process group $(cat "$pidfile") is alive but port ${port} is not serving; run '${stop_hint}'"
    exit 1
  fi
  if probe_port; then
    echo "port ${port} is serving, but not via dev-start (foreground '${fg_hint}' or another process)"
    exit 0
  fi
  if [ -f "$pidfile" ]; then
    echo "dev server is down (stale pidfile from pid $(cat "$pidfile"); dev-start will clean it up)"
  else
    echo "dev server is down"
  fi
  exit 1
}

case "${1:-}" in
  start) do_start ;;
  stop) do_stop ;;
  status) do_status ;;
  *)
    echo "usage: $0 {start|stop|status}" >&2
    exit 2
    ;;
esac
