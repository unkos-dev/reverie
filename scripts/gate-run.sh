#!/usr/bin/env bash
# Run a list of `just` lanes as one verification gate, then end with a single
# machine-readable verdict line and record what happened.
#
# Why a verdict line. Without one, a gate's last line of stdout is whatever the
# final lane happened to print, and that looks identical whether the lanes
# before it passed or failed. All three ways a long gate actually gets captured
# lose the exit status that would have answered the question: a pipe reports the
# last command in the pipeline, a truncating `tail` drops the start of the run,
# and a detached run outlives the shell that would have read `$?`. `GATE: PASS`
# and `GATE: FAIL` survive all three and need one grep.
#
# Why one `just` invocation per lane, rather than a single `just a b c`. The
# single form is not merely coarser, it is wrong: a lane that takes parameters,
# such as `rust::test *args`, consumes every following name as its own
# argument, so the lanes after it silently never run while the gate can still
# exit 0. Running each lane alone also lets a failure name the lane that
# produced it. The cost is one just startup per lane, milliseconds against a
# multi-minute gate, plus the loss of just's cross-recipe deduplication: no two
# lanes in the callers' lists share a dependency recipe today, and a future
# pair that did would run the shared recipe twice.
#
# Usage: gate-run.sh <label> [lane...]   # run the lanes, print the verdict
#        gate-run.sh --status            # report the last recorded run
#        gate-run.sh --run-dir           # print the per-checkout record dir
#
# --status exit codes, one per outcome a caller must not confuse: 0 the last
# run passed, 1 it failed, 2 it died unfinished, 3 it is still in progress,
# 4 there is no recorded run at all. The record is still the secondary channel:
# a run whose record location was unwritable leaves nothing here, so the
# `GATE:` line in the captured output remains the authority.
#
# Resource bound: when a systemd user manager is reachable and this is not
# CI, a lane run (never --status or --run-dir, and never a bare query) wraps
# itself once, whole-run rather than per-lane, in a `systemd-run --user
# --scope` under agents.slice, so a runaway lane cannot exhaust the host
# outside its own cgroup budget. See the check further down for the exact
# conditions and why CI cannot be affected by it.
set -ueo pipefail

# Runs older than this, or beyond this many newer runs, are pruned at startup.
readonly KEEP_RUNS=20
readonly MAX_AGE_DAYS=14

die() {
  printf 'gate-run: %s\n' "$1" >&2
  exit 1
}

# Lane and label names reach both the `just` command line and the JSON records
# below without quoting or escaping, so the accepted character set is narrow on
# purpose: it stops a name from being read as a `just` flag, and it removes any
# way for a name to break out of a JSON string. Reject rather than sanitise, so
# a caller passing something unexpected fails loudly instead of running a
# quietly different lane.
valid_name() {
  case "$1" in
    '' | -*) return 1 ;;
    *[!A-Za-z0-9_:.-]*) return 1 ;;
  esac
}

# Runs are recorded outside the checkout: this is machine state, not repository
# content, and nothing untracked should land in the tree. The directory is keyed
# per checkout so concurrent worktrees never write to the same place, and within
# a checkout every run owns one file named after itself, so concurrent runs need
# no locking either. The count prune only ever deletes records that carry a
# verdict: one without may belong to a run that is merely slow, and deleting it
# under its writer would recreate it headless on the next append. Records that
# never get a verdict are left to the age prune.
checkout="$(git rev-parse --show-toplevel 2> /dev/null || pwd -P)"
key="$(printf '%s' "$checkout" | sha256sum | cut -c1-12)"
slug="$(printf '%s' "${checkout##*/}" | tr -c 'A-Za-z0-9._-' '-')"
run_dir="${XDG_STATE_HOME:-${HOME}/.local/state}/reverie/gate/${slug}-${key}"

# Newest first. Run ids lead with a UTC timestamp and the paths share a prefix,
# so a plain reverse sort of the paths is chronological. A missing directory is
# an empty list, not an error: `--status` before the first run is a normal
# question, and pipefail would otherwise turn find's exit status into one.
list_runs() {
  [ -d "$run_dir" ] || return 0
  find "$run_dir" -maxdepth 1 -type f -name '*.jsonl' -print 2> /dev/null | sort -r || true
}

# The commit the checkout sits on, empty outside a git repository (a fixture,
# a plain directory). Recorded with each run and compared on --status, so a
# verdict can be tied to the tree that earned it rather than whatever the
# checkout holds by the time someone asks.
current_head() {
  git -C "$checkout" rev-parse HEAD 2> /dev/null || true
}

# Bound resource usage: wrap the whole run in one transient scope rather than
# trusting every lane's own tooling to behave, so a runaway lane cannot
# exhaust the host outside its own cgroup budget. Excluded from the wrap:
# --status and --run-dir (pure queries, nothing to bound), and any run
# already inside one (GATE_RUN_SCOPED, set just before the exec below, guards
# against wrapping the re-invocation a second time once it lands back here as
# the scope's own command).
#
# CI must see zero behavioural change, so the CI environment variable
# disables the wrap outright rather than trusting `systemctl` to also come
# back unreachable on every CI runner; GitHub Actions exports CI=true on
# every job. Off CI, `systemctl --user show-environment` is the direct
# reachability probe: no session, no bus, or no systemd at all fail it the
# same way, and the run proceeds exactly as it did before this existed.
if [ "${1:-}" != '--status' ] && [ "${1:-}" != '--run-dir' ] \
  && [ -z "${GATE_RUN_SCOPED:-}" ] && [ -z "${CI:-}" ] \
  && command -v systemd-run > /dev/null 2>&1 \
  && systemctl --user show-environment > /dev/null 2>&1; then
  export GATE_RUN_SCOPED=1
  exec systemd-run --user --scope --slice=agents.slice --quiet --collect -- "$0" "$@"
fi

if [ "${1:-}" = '--run-dir' ]; then
  [ "$#" -eq 1 ] || die 'usage: gate-run.sh --run-dir'
  printf '%s\n' "$run_dir"
  exit 0
fi

if [ "${1:-}" = '--status' ]; then
  [ "$#" -eq 1 ] || die 'usage: gate-run.sh --status'
  latest="$(list_runs | head -1)"
  if [ -z "$latest" ]; then
    # Nonzero on purpose: "no record" and "the last run passed" must never
    # share an exit status. A record can be missing for benign reasons (first
    # run on a machine) and for the dangerous one (the run that mattered could
    # not write here), and this script cannot tell those apart.
    printf 'gate-run: no recorded gate run for %s\n' "$checkout" >&2
    exit 4
  fi
  printf 'last gate run: %s\n' "$latest"
  cat "$latest"
  # A verdict answers for the tree it ran on. If the checkout has moved since,
  # say so out loud rather than letting an old green pass for the current
  # tree; the exit status is left alone because committing verified work moves
  # HEAD by construction, and a warning is the honest strength of the signal.
  recorded_head="$(sed -n 's/.*"head":"\([0-9a-f]*\)".*/\1/p' "$latest" | head -1)"
  checkout_head="$(current_head)"
  if [ -n "$recorded_head" ] && [ -n "$checkout_head" ] &&
    [ "$recorded_head" != "$checkout_head" ]; then
    printf 'gate-run: the checkout has moved since that run (was %.12s, now %.12s); its verdict does not cover the current tree\n' \
      "$recorded_head" "$checkout_head" >&2
  fi
  # The dirty flag gets the same warning-strength treatment as HEAD. A verdict
  # earned on a clean tree stops covering the checkout at the first
  # uncommitted edit, and one earned on a dirty tree answers for content no
  # later reader can reconstruct. The exit status is left alone in both cases:
  # an edit on an already-dirty tree is undetectable without hashing the whole
  # tree, and an exit code that failed one unverified case while passing its
  # twin would be a guarantee the record cannot keep.
  if [ -n "$checkout_head" ]; then
    if grep -q '"dirty":true' "$latest"; then
      printf 'gate-run: that run verified a dirty tree; its verdict answers for uncommitted content that cannot be re-checked\n' >&2
    elif grep -q '"dirty":false' "$latest" &&
      [ -n "$(git -C "$checkout" status --porcelain 2> /dev/null | head -1)" ]; then
      printf 'gate-run: the tree has uncommitted changes that run never saw; its verdict does not cover them\n' >&2
    fi
  fi
  # A run with no verdict either died mid-flight (Ctrl-C, a dropped
  # connection, a reboot) or is still going. Reporting either as a pass would
  # be the same false green this script exists to prevent, so each gets a
  # nonzero status of its own, decided by whether the recorded runner pid is
  # alive. kill -0 answers liveness, not identity: a pid recycled since the
  # run can make a dead run read as live, which is accepted for the newest
  # same-user record over anything unportable like procfs. The lane in
  # question is the one after the last that finished, which the plan recorded
  # at the top of the file can name outright.
  if ! grep -q '"verdict":' "$latest"; then
    completed="$(grep -c '"status":"pass"' "$latest" || true)"
    stopped_in="$(sed -n 's/.*"plan":\[\(.*\)\].*/\1/p' "$latest" \
      | head -1 | tr ',' '\n' | tr -d '"' | sed -n "$((completed + 1))p")"
    runner_pid="$(sed -n 's/.*"pid":\([0-9]*\).*/\1/p' "$latest" | head -1)"
    if [ -n "$runner_pid" ] && kill -0 "$runner_pid" 2> /dev/null; then
      printf 'gate-run: that run is still in progress%s\n' \
        "${stopped_in:+, currently in ${stopped_in}}" >&2
      exit 3
    fi
    printf 'gate-run: that run never finished (no verdict recorded)%s\n' \
      "${stopped_in:+, it stopped during ${stopped_in}}" >&2
    exit 2
  fi
  if grep -q '"verdict":"pass"' "$latest"; then
    exit 0
  fi
  exit 1
fi

[ "$#" -ge 1 ] || die 'usage: gate-run.sh <label> [lane...]'
label="$1"
shift
valid_name "$label" || die "invalid label: ${label}"
lanes=("$@")
for lane in ${lanes[@]+"${lanes[@]}"}; do
  valid_name "$lane" || die "invalid lane name: ${lane}"
done

# Two runs can start inside the same second (a scripted retry, a test harness),
# and both pruning and `--status` order runs by name, so the id carries
# sub-second precision. The second and the sub-second both come from the one
# EPOCHREALTIME reading: stamping the second with a separate clock read let a
# run that captured .9999 cross the boundary before the stamp, sort itself
# above every run started later in the new second, and hand --status the wrong
# file as newest. EPOCHREALTIME and %()T are bash builtins, so this costs no
# process; the pid keeps names unique if a shell leaves EPOCHREALTIME unset or
# renders it for a locale whose decimal separator is not a dot.
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
run_id="${stamp}-${subsecond}-$$"
run_log="${run_dir}/${run_id}.jsonl"
if mkdir -p "$run_dir" 2> /dev/null && : >> "$run_log" 2> /dev/null; then
  # Pruning is deliberately non-fatal: an unpruned directory is cosmetic, and
  # nothing about housekeeping should be able to stop a gate from running.
  find "$run_dir" -maxdepth 1 -type f -name '*.jsonl' -mtime "+${MAX_AGE_DAYS}" -delete 2> /dev/null || true
  kept=0
  while IFS= read -r stale; do
    kept=$((kept + 1))
    if [ "$kept" -gt "$KEEP_RUNS" ] && grep -q '"verdict":' "$stale" 2> /dev/null; then
      rm -f "$stale"
    fi
  done < <(list_runs)
else
  # A read-only or sandboxed HOME must never fail the gate: the record is a
  # convenience, the verdict line on stdout is the contract.
  printf 'gate-run: cannot write %s; continuing without a run record\n' "$run_dir" >&2
  run_log=''
fi

# Records carry lane names, statuses, and integers only, never lane output: a
# recipe line can legitimately contain a DSN default, and a log that quietly
# accumulated one would defeat scripts/recipe-secret-echo-test.sh.
record() {
  [ -n "$run_log" ] || return 0
  printf '%s\n' "$1" >> "$run_log" || {
    printf 'gate-run: lost the run record at %s; continuing\n' "$run_log" >&2
    run_log=''
  }
}

total="${#lanes[@]}"
started="$SECONDS"
TZ=UTC0 printf -v started_at '%(%Y-%m-%dT%H:%M:%SZ)T' "$whole"
index=0

# The plan goes in before the first lane runs, so a run that is killed rather
# than finished can still say what it was going to do and how far it got. The
# names passed valid_name above, so they need no JSON escaping.
plan=''
for lane in ${lanes[@]+"${lanes[@]}"}; do
  plan="${plan:+${plan},}\"${lane}\""
done
# The start event also pins down what this run answers for: the commit and
# whether the tree was dirty, so --status can flag a verdict the checkout has
# outgrown, and the runner pid, so --status can tell a run that died from one
# still going. rev-parse output is hex by construction; anything else (or no
# repository at all) records null rather than an unquoted surprise.
tree_head="$(current_head)"
case "$tree_head" in
  *[!0-9a-f]* | '') tree='"head":null,"dirty":null' ;;
  *)
    if [ -n "$(git -C "$checkout" status --porcelain 2> /dev/null | head -1)" ]; then
      tree="\"head\":\"${tree_head}\",\"dirty\":true"
    else
      tree="\"head\":\"${tree_head}\",\"dirty\":false"
    fi
    ;;
esac
record "{\"run\":\"${run_id}\",\"label\":\"${label}\",\"event\":\"start\",\"lanes\":${total},\"plan\":[${plan}],\"started\":\"${started_at}\",${tree},\"pid\":$$}"

for lane in ${lanes[@]+"${lanes[@]}"}; do
  index=$((index + 1))
  printf '[gate %d/%d] %s\n' "$index" "$total" "$lane"
  lane_started="$SECONDS"
  rc=0
  just "$lane" || rc=$?
  lane_seconds=$((SECONDS - lane_started))
  if [ "$rc" -ne 0 ]; then
    record "{\"run\":\"${run_id}\",\"lane\":\"${lane}\",\"status\":\"fail\",\"exit\":${rc},\"seconds\":${lane_seconds}}"
    record "{\"run\":\"${run_id}\",\"label\":\"${label}\",\"verdict\":\"fail\",\"failed_lane\":\"${lane}\",\"exit\":${rc},\"lanes\":${total},\"completed\":$((index - 1)),\"started\":\"${started_at}\",\"seconds\":$((SECONDS - started))}"
    printf 'GATE: FAIL %s at %s (exit %d, %ds)\n' "$label" "$lane" "$rc" "$((SECONDS - started))"
    exit "$rc"
  fi
  record "{\"run\":\"${run_id}\",\"lane\":\"${lane}\",\"status\":\"pass\",\"exit\":0,\"seconds\":${lane_seconds}}"
done

record "{\"run\":\"${run_id}\",\"label\":\"${label}\",\"verdict\":\"pass\",\"failed_lane\":null,\"exit\":0,\"lanes\":${total},\"completed\":${total},\"started\":\"${started_at}\",\"seconds\":$((SECONDS - started))}"
# The lane list rides on the verdict line, not only on the announcement the
# caller printed before the run: the announcement is the first thing a
# truncated log loses, and "which lanes did this actually cover" is half the
# question a scoped gate has to answer.
summary=''
if [ "$total" -gt 0 ]; then
  summary=": ${lanes[*]}"
fi
noun='lanes'
if [ "$total" -eq 1 ]; then
  noun='lane'
fi
printf 'GATE: PASS %s (%d %s, %ds%s)\n' "$label" "$total" "$noun" "$((SECONDS - started))" "$summary"
