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

# Reuses gate-run.sh's own directory derivation (checkout path, hashed and
# slugged) rather than recomputing it here, so the log can never land
# somewhere gate-status does not also look.
run_dir="$(scripts/gate-run.sh --run-dir)"
mkdir -p "$run_dir"

TZ=UTC0 printf -v stamp '%(%Y%m%dT%H%M%SZ)T' -1
log="${run_dir}/detached-${stamp}-$$.log"

echo "gate-detach: running 'just ${recipe}' detached, log at ${log}"
echo "gate-detach: replay the verdict with 'just gate-status'"

# `setsid` detaches the child from this shell's controlling terminal so a
# later SIGHUP (the shell exiting, the terminal closing) cannot reach it;
# stdin is closed and stdout/stderr redirected to the log file before the
# background job starts, so command substitution around this script returns
# as soon as this shell exits rather than waiting on an inherited pipe.
setsid just "${recipe}" "$@" > "${log}" 2>&1 < /dev/null &
disown
