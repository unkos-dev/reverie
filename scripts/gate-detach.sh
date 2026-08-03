#!/usr/bin/env bash
# Run a `just` recipe detached from the invoking terminal, so a long gate can
# survive a session or turn boundary without a hand-rolled setsid-plus-log
# pipeline. The recipe is expected to end in a scripts/gate-run.sh verdict
# (`just preflight` or `just preflight-full`); the full output, not just the
# verdict line, lands in a log file under the same $XDG_STATE_HOME/reverie/
# gate/ directory scripts/gate-run.sh already keys per checkout, so the log
# sits next to the run record `just gate-status` reads back.
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
mkdir -p "$run_dir"

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
log="${run_dir}/detached-${stamp}-$$.log"

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

echo "gate-detach: running 'just ${recipe}' detached, log at ${log}"
echo "gate-detach: replay the verdict with 'just gate-status'"
