#!/usr/bin/env bash
# Self-test scripts/gate-detach.sh: the mechanics behind `just preflight-detach`.
# Exercises the real script against a fixture justfile that wraps
# scripts/gate-run.sh, so the assertions cover the actual detach-and-log path
# without paying for a real preflight lane.
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"

failures=0
pass() { printf 'ok   %s\n' "$1"; }
fail() {
  printf 'FAIL %s\n  %s\n' "$1" "$2"
  failures=$((failures + 1))
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

fixture="${tmpdir}/fixture"
mkdir -p "${fixture}/scripts"
cp "${repo_root}/scripts/gate-run.sh" "${fixture}/scripts/gate-run.sh"
cp "${repo_root}/scripts/gate-detach.sh" "${fixture}/scripts/gate-detach.sh"

# A real, isolated git repo: gate-run.sh's own commit/dirty bookkeeping
# should not leak the actual checkout's state into this fixture's records.
git -C "$fixture" init -q -b main
git -C "$fixture" config user.name "Gate Detach Selftest"
git -C "$fixture" config user.email "selftest@example.invalid"
git -C "$fixture" config commit.gpgsign false

cat > "${fixture}/justfile" << 'EOF'
set shell := ["bash", "-ueo", "pipefail", "-c"]

fixture-gate:
    scripts/gate-run.sh demo ok

ok:
    echo "lane ok ran"

boom:
    echo "lane boom ran"; exit 3

fixture-gate-fail:
    scripts/gate-run.sh demo boom

upstream-fail:
    echo "dying before gate-run.sh ever starts"; exit 7
EOF
git -C "$fixture" add justfile scripts
git -C "$fixture" commit -qm fixture

state="${tmpdir}/state"

# Poll for the detached job to finish rather than sleeping a fixed amount:
# the lane is a trivial echo, so this normally resolves in well under a
# second, but a slow or loaded CI runner must not turn that into a flake.
wait_for_status() { # <state_home> <timeout_tenths_of_a_second>
  local state_home="$1" timeout="$2" waited=0 rc
  while [ "$waited" -lt "$timeout" ]; do
    rc=0
    XDG_STATE_HOME="$state_home" bash -c "cd '$fixture' && '${fixture}/scripts/gate-run.sh' --status" > /dev/null 2>&1 || rc=$?
    # A recorded failure is also "finished"; only "no record yet" (exit 4)
    # or "still in progress" (exit 3) should keep polling.
    if [ "$rc" -ne 3 ] && [ "$rc" -ne 4 ]; then
      return 0
    fi
    sleep 0.1
    waited=$((waited + 1))
  done
  return 1
}

# The record and the log are two channels for one event, and they flush
# differently: gate-run.sh appends each record line in its own open-write-
# close, so records land immediately, while the detached child's stdout is a
# block-buffered stream that may not surface the verdict line until process
# teardown. Any assertion on log CONTENT therefore waits on the log itself;
# synchronising on the record and then reading the log is a race that only
# fails on a slow machine.
wait_for_log_verdict() { # <log_path> <timeout_tenths_of_a_second>
  local log_path="$1" timeout="$2" waited=0
  while [ "$waited" -lt "$timeout" ]; do
    if grep -q '^GATE: ' "$log_path" 2> /dev/null; then
      return 0
    fi
    sleep 0.1
    waited=$((waited + 1))
  done
  return 1
}

# --- a detached run produces a log and a gate-status-readable record --------

state_ok="${state}/ok"
out="$(XDG_STATE_HOME="$state_ok" bash -c "cd '$fixture' && '${fixture}/scripts/gate-detach.sh' fixture-gate" 2>&1)"

case "$out" in
  *"gate-detach: running 'just fixture-gate' detached, log at "*) pass 'the immediate announcement names the recipe and the log path' ;;
  *) fail 'the immediate announcement names the recipe and the log path' "output: ${out}" ;;
esac
case "$out" in
  *"gate-detach: replay the verdict with 'just gate-status'"*) pass 'the announcement reminds the caller to replay with gate-status' ;;
  *) fail 'the announcement reminds the caller to replay with gate-status' "output: ${out}" ;;
esac

log_path="$(printf '%s\n' "$out" | sed -n 's/.*log at //p')"
if [ -z "$log_path" ]; then
  fail 'a log path was printed' "output: ${out}"
  printf '\ncannot continue without a log path\n' >&2
  exit 1
fi

run_dir="$(XDG_STATE_HOME="$state_ok" bash -c "cd '$fixture' && '${fixture}/scripts/gate-run.sh' --run-dir")"
case "$log_path" in
  "${run_dir}"/*) pass 'the log lands under the same directory the run records use' ;;
  *) fail 'the log lands under the same directory the run records use' "log: ${log_path}, run_dir: ${run_dir}" ;;
esac

# Mode 600, not the ambient umask: the log holds the full lane output the
# run records deliberately refuse, and the umask-guarded pre-creation in
# gate-detach.sh is one refactor away from looking redundant next to the
# redirect. This assertion is what encodes why it exists.
log_mode="$(stat -c '%a' "$log_path" 2> /dev/null || true)"
if [ "$log_mode" = "600" ]; then
  pass 'the primary log is created mode 600'
else
  fail 'the primary log is created mode 600' "mode: ${log_mode:-unreadable}"
fi

if wait_for_status "$state_ok" 100; then
  pass 'gate-status reports the detached run once it finishes'
else
  fail 'gate-status reports the detached run once it finishes' 'timed out waiting for a finished record'
fi

if ! wait_for_log_verdict "$log_path" 100; then
  fail 'the log carries the real verdict line' "no GATE: line appeared; log holds: $(cat "$log_path" 2> /dev/null)"
elif ! grep -qF 'GATE: PASS demo' "$log_path"; then
  fail 'the log carries the real verdict line' "$(cat "$log_path")"
else
  pass 'the log carries the real verdict line'
fi

rc=0
XDG_STATE_HOME="$state_ok" bash -c "cd '$fixture' && '${fixture}/scripts/gate-run.sh' --status" > /dev/null 2>&1 || rc=$?
if [ "$rc" -ne 0 ]; then
  fail 'gate-status exits 0 for the passing detached run' "exited ${rc}"
else
  pass 'gate-status exits 0 for the passing detached run'
fi

# --- a failing detached run is still readable back as a failure -------------

state_fail="${state}/fail"
XDG_STATE_HOME="$state_fail" bash -c "cd '$fixture' && '${fixture}/scripts/gate-detach.sh' fixture-gate-fail" > /dev/null

if ! wait_for_status "$state_fail" 100; then
  fail 'a failing detached run is read back as a failure' 'timed out waiting for a finished record'
else
  rc=0
  XDG_STATE_HOME="$state_fail" bash -c "cd '$fixture' && '${fixture}/scripts/gate-run.sh' --status" > /dev/null 2>&1 || rc=$?
  if [ "$rc" -eq 1 ]; then
    pass 'a failing detached run is read back as a failure'
  else
    fail 'a failing detached run is read back as a failure' "exit ${rc}, expected 1"
  fi
fi

# --- an unwritable state dir must not stop the gate --------------------------

# gate-run.sh's contract: the record layer is a convenience that must never
# stop a gate. The detach path adds a log to that layer, so on an unwritable
# state dir it must still launch the run, with the log falling back to a
# TMPDIR path and a warning naming what gate-status loses.
state_ro="${state}/ro"
mkdir -p "$state_ro"
chmod 500 "$state_ro"
if touch "${state_ro}/probe" 2> /dev/null; then
  rm -f "${state_ro}/probe"
  echo 'skip unwritable-state case: chmod has no effect here (running as root?)'
else
  rc=0
  out="$(XDG_STATE_HOME="$state_ro" bash -c "cd '$fixture' && '${fixture}/scripts/gate-detach.sh' fixture-gate" 2>&1)" || rc=$?
  if [ "$rc" -ne 0 ]; then
    fail 'an unwritable state dir still launches the gate' "gate-detach exited ${rc}: ${out}"
  else
    pass 'an unwritable state dir still launches the gate'
  fi
  case "$out" in
    *"cannot write "*"gate-status' will not track this run"*) pass 'the fallback warns that gate-status will not track the run' ;;
    *) fail 'the fallback warns that gate-status will not track the run' "output: ${out}" ;;
  esac
  # The stale-verdict trap this case exists to close: with no marker written,
  # a gate-status hint here would replay whatever older verdict the state dir
  # already holds, so the closing line must name the log as the only channel.
  case "$out" in
    *"replay the verdict with 'just gate-status'"*) fail 'the fallback does not point at gate-status' "output: ${out}" ;;
    *"follow the run with 'tail -f "*) pass 'the fallback points at the log, not gate-status' ;;
    *) fail 'the fallback points at the log, not gate-status' "output: ${out}" ;;
  esac
  ro_log="$(printf '%s\n' "$out" | sed -n 's/.*log at //p')"
  if [ -z "$ro_log" ]; then
    fail 'the fallback still announces a log path' "output: ${out}"
  elif ! wait_for_log_verdict "$ro_log" 100; then
    fail 'the fallback log still carries the verdict line' "no GATE: line appeared; log holds: $(cat "$ro_log" 2> /dev/null)"
  elif ! grep -qF 'GATE: PASS demo' "$ro_log"; then
    fail 'the fallback log still carries the verdict line' "$(cat "$ro_log")"
  else
    pass 'the fallback log still carries the verdict line'
  fi
  ro_mode="$(stat -c '%a' "$ro_log" 2> /dev/null || true)"
  if [ "$ro_mode" = "600" ]; then
    pass 'the fallback log is created mode 600'
  else
    fail 'the fallback log is created mode 600' "mode: ${ro_mode:-unreadable}"
  fi
  rm -f "$ro_log"
fi
chmod 700 "$state_ro"

# --- a detach that dies upstream of gate-run.sh must not replay a stale PASS

# The regression this pins down: gate-run.sh writes its start record only
# once it is running, so a detached recipe failing before it (a scoper error,
# a missing tool) used to leave the previous run's record as the newest one,
# and gate-status replayed a stale green verdict for a gate that never ran.
# The detach-start marker makes that outcome exit 2, "never finished".
state_stale="${state}/stale"
XDG_STATE_HOME="$state_stale" bash -c "cd '$fixture' && '${fixture}/scripts/gate-run.sh' demo ok" > /dev/null 2>&1
rc=0
XDG_STATE_HOME="$state_stale" bash -c "cd '$fixture' && '${fixture}/scripts/gate-run.sh' --status" > /dev/null 2>&1 || rc=$?
if [ "$rc" -ne 0 ]; then
  fail 'seeding a green record before the upstream-death case' "gate-status exited ${rc} on the seed run"
fi

XDG_STATE_HOME="$state_stale" bash -c "cd '$fixture' && '${fixture}/scripts/gate-detach.sh' upstream-fail" > /dev/null

if ! wait_for_status "$state_stale" 100; then
  fail 'an upstream death settles into a readable status' 'status stayed in progress or unrecorded'
else
  rc=0
  XDG_STATE_HOME="$state_stale" bash -c "cd '$fixture' && '${fixture}/scripts/gate-run.sh' --status" > /dev/null 2>&1 || rc=$?
  if [ "$rc" -eq 2 ]; then
    pass 'a detach dying upstream of gate-run.sh reads as never-finished, not the stale PASS'
  else
    fail 'a detach dying upstream of gate-run.sh reads as never-finished, not the stale PASS' "exit ${rc}, expected 2"
  fi
fi

# --- the justfile recipe delegates to this script, not a re-inlined pipeline

# Guards against the mechanics drifting back into the recipe body, which
# would leave this whole file testing a script the recipe no longer calls.
# shellcheck disable=SC2016  # the pattern is recipe text; $ must stay literal
if grep -q 'exec scripts/gate-detach.sh "\$target" "\$@"' "${repo_root}/justfile"; then
  pass 'preflight-detach delegates to scripts/gate-detach.sh'
else
  fail 'preflight-detach delegates to scripts/gate-detach.sh' 'the justfile recipe no longer calls the script this file tests'
fi

if [ "$failures" -ne 0 ]; then
  printf '\n%d gate-detach assertion(s) failed\n' "$failures" >&2
  exit 1
fi
echo 'OK: gate-detach produces a log and a gate-status-readable record for passing, failing, dying-upstream, and unwritable-state runs'
