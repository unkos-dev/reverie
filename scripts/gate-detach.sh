#!/usr/bin/env bash
# Run a `just` recipe detached from the invoking terminal, so a long gate can
# survive a session or turn boundary without a hand-rolled setsid-plus-log
# pipeline. The recipe is expected to end in a scripts/gate-run.sh verdict
# (`just preflight` or `just preflight-full`); the full output, not just the
# verdict line, lands in a log file under the same $XDG_STATE_HOME/reverie/
# gate/ directory scripts/gate-run.sh already keys per checkout, so the log
# sits next to the run record `just gate-status` reads back.
#
# The log accepts what the run records refuse: gate-run.sh keeps lane output
# out of its records because a recipe line can legitimately echo a secret
# default, but a detached run's output has to land somewhere or the run is
# unobservable. The log is the terminal a detached run does not have, so it
# is created 0600 and reaped by gate-run.sh's age prune.
#
# Usage: scripts/gate-detach.sh <just-recipe> [recipe-arg...]
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$repo_root"

recipe="${1:?usage: scripts/gate-detach.sh <just-recipe> [recipe-arg...]}"
shift

# The recipe name lands in the marker record below without escaping, so it
# gets the same narrow character set gate-run.sh enforces for labels: reject
# rather than sanitise.
case "$recipe" in
  '' | -* | *[!A-Za-z0-9_:.-]*)
    echo "gate-detach: invalid recipe name: ${recipe}" >&2
    exit 1
    ;;
esac

# Reuses gate-run.sh's own directory derivation (checkout path, hashed and
# slugged) rather than recomputing it here, so the log can never land
# somewhere gate-status does not also look.
run_dir="$(scripts/gate-run.sh --run-dir)"

# Same id derivation as gate-run.sh, for the same reason: records sort by
# name, newest first, and the marker written below must sort under any record
# the detached run goes on to write in the same second. See gate-run.sh for
# why both parts come from one EPOCHREALTIME reading.
epoch="${EPOCHREALTIME:-}"
subsecond="${epoch#*[.,]}"
whole="${epoch%%[.,]*}"
case "$subsecond" in
  '' | *[!0-9]*) subsecond='000000' ;;
esac
case "$whole" in
  '' | *[!0-9]*) whole='-1' ;;
esac
TZ=UTC0 printf -v stamp '%(%Y%m%dT%H%M%SZ)T' "$whole"

# An unwritable state dir must never stop the gate: gate-run.sh states that
# contract for its records, and it holds here too. The log is not optional
# the way a record is (a detached run with no log is unobservable), so it
# falls back to a mktemp path, which mktemp creates 0600 just like the
# umask-guarded primary path; the marker is skipped with a warning naming
# the consequence. The background job's redirect below truncates the
# pre-created file without changing its mode.
log="${run_dir}/detached-${stamp}-$$.log"
if ! { mkdir -p "$run_dir" && (umask 077 && : > "$log"); } 2> /dev/null; then
  log="$(mktemp "${TMPDIR:-/tmp}/reverie-gate-detach-XXXXXX.log")" || {
    echo "gate-detach: nowhere writable for the log (state dir and TMPDIR both failed)" >&2
    exit 1
  }
  echo "gate-detach: cannot write ${run_dir}; logging to ${log} instead, and 'just gate-status' will not track this run" >&2
  run_dir=''
fi

# `setsid` detaches the child from this shell's controlling terminal so a
# later SIGHUP (the shell exiting, the terminal closing) cannot reach it;
# stdin is closed and stdout/stderr redirected to the log file before the
# background job starts, so command substitution around this script returns
# as soon as this shell exits rather than waiting on an inherited pipe.
setsid just "${recipe}" "$@" > "${log}" 2>&1 < /dev/null &
child=$!
disown

# A verdict-less marker record, written the moment the run is detached. The
# gate runner writes its own start record only once it is running, so
# without this, a detached run that dies upstream of gate-run.sh (a scoper
# failure on a bad --base, a missing tool) leaves the previous run's record
# as the newest one, and `just gate-status` replays a stale verdict for a
# gate that never ran. The marker carries the detached child's pid and no
# verdict, so gate-run.sh --status applies its existing no-verdict split:
# child alive reads as "still in progress", child dead with nothing newer
# reads as "never finished", and the run's own records supersede the marker
# by sorting newer the moment gate-run.sh starts.
if [ -n "$run_dir" ]; then
  tree_head="$(git rev-parse HEAD 2> /dev/null || true)"
  case "$tree_head" in
    *[!0-9a-f]* | '') tree='"head":null,"dirty":null' ;;
    *)
      if [ -n "$(git status --porcelain 2> /dev/null | head -1)" ]; then
        tree="\"head\":\"${tree_head}\",\"dirty\":true"
      else
        tree="\"head\":\"${tree_head}\",\"dirty\":false"
      fi
      ;;
  esac
  marker="${run_dir}/${stamp}-${subsecond}-${child}.jsonl"
  printf '%s\n' "{\"run\":\"${stamp}-${subsecond}-${child}\",\"label\":\"${recipe}\",\"event\":\"detach-start\",${tree},\"pid\":${child}}" > "$marker" 2> /dev/null ||
    echo "gate-detach: cannot record the detach at ${marker}; gate-status will not see this run until gate-run.sh starts" >&2
fi

echo "gate-detach: running 'just ${recipe}' detached, log at ${log}"
# The closing hint must match the channel that actually answers for this
# run: on the fallback path there is no marker and no record coming, so
# pointing at gate-status would replay whatever older verdict the unwritable
# directory already holds, the exact stale green the marker exists to
# prevent.
if [ -n "$run_dir" ]; then
  echo "gate-detach: replay the verdict with 'just gate-status'"
else
  echo "gate-detach: follow the run with 'tail -f ${log}'; gate-status will not track it"
fi
