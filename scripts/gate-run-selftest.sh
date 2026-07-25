#!/usr/bin/env bash
# Self-test for scripts/gate-run.sh. The runner exists so a captured gate run
# can be read back without its exit status, so the failure that matters is a
# wrong or missing verdict: a run that failed but ends in `GATE: PASS` is worse
# than no verdict line at all, and nothing downstream could catch it.
#
# The lanes are a fixture justfile in a scratch directory, so every assertion
# exercises the real runner against real `just` invocations without paying for,
# or depending on, any repository lane.
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
runner="${repo_root}/scripts/gate-run.sh"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

failures=0

pass() { printf 'ok   %s\n' "$1"; }
fail() {
  printf 'FAIL %s\n  %s\n' "$1" "$2"
  failures=$((failures + 1))
}

# Stands in for the DSN defaults DB recipes echo: lane output belongs on the
# terminal, but must never reach the run record, which lives outside the
# repository and outlives the run.
sentinel='S3CRET-sentinel-credential'

lanes_dir="${tmpdir}/lanes"
mkdir -p "$lanes_dir"
cat > "${lanes_dir}/justfile" << EOF
set shell := ["bash", "-ueo", "pipefail", "-c"]

ok:
    echo "lane ok ran"

also-ok:
    echo "lane also-ok ran"

boom:
    echo "lane boom ran"; exit 3

leak:
    echo "connect: postgres://user:${sentinel}@localhost/db"

swallow *args:
    echo "swallow got [{{ args }}]"
EOF

state="${tmpdir}/state"
outfile="${tmpdir}/out"

# Run the runner from a fixture directory against an isolated record location,
# leaving the combined output in $out and the status in $rc. Not a command
# substitution: that would run the assignment to $rc in a subshell and every
# exit-status assertion below would silently read a stale value.
out=''
rc=0
run_gate_in() {
  local dir="$1" state_home="$2"
  shift 2
  rc=0
  (cd "$dir" && XDG_STATE_HOME="$state_home" "$runner" "$@") > "$outfile" 2>&1 || rc=$?
  out="$(cat "$outfile")"
}
run_gate() { run_gate_in "$lanes_dir" "$@"; }

last_line() { tail -1 "$outfile"; }

# --- a passing run ends in a PASS verdict, on the last line, exit 0 ----------

run_gate "$state" demo ok also-ok
if [ "$rc" -ne 0 ]; then
  fail 'a passing run exits 0' "exited ${rc}"
else
  pass 'a passing run exits 0'
fi
case "$(last_line)" in
  'GATE: PASS demo (2 lanes, '*'s: ok also-ok)') pass 'the PASS verdict is the last line and lists the lanes' ;;
  *) fail 'the PASS verdict is the last line and lists the lanes' "last line: $(last_line)" ;;
esac
case "$out" in
  *'lane ok ran'*'lane also-ok ran'*) pass 'lanes run in the order given' ;;
  *) fail 'lanes run in the order given' "output: ${out}" ;;
esac

# --- a lane that takes parameters does not consume the lanes after it --------

# This is the reason the runner loops rather than handing `just` the whole
# list, and it is a correctness property, not a preference: a variadic lane on
# a shared command line eats every following name as its own argument, those
# lanes never run, and the gate can still exit 0.

# Control first, so the assertion below cannot pass vacuously: the hazard has
# to be reproducible in this fixture for the fix to be worth asserting.
control="$( (cd "$lanes_dir" && just swallow ok) 2>&1 || true)"
case "$control" in
  *'swallow got [ok]'*) pass 'control: one just call does let a variadic lane swallow the next name' ;;
  *) fail 'control: one just call lets a variadic lane swallow the next name' "output: ${control}" ;;
esac

run_gate "$state" demo swallow ok
if [ "$rc" -ne 0 ]; then
  fail 'a run with a variadic lane exits 0' "exited ${rc}"
elif ! printf '%s' "$out" | grep -qF 'swallow got []'; then
  fail 'a variadic lane receives no arguments' "output: ${out}"
elif ! printf '%s' "$out" | grep -qF 'lane ok ran'; then
  fail 'the lane after a variadic lane still runs' "output: ${out}"
else
  pass 'a variadic lane does not swallow the lane after it'
fi

# --- a failing run names the lane, propagates its status, and stops ----------

run_gate "$state" demo ok boom also-ok
if [ "$rc" -ne 3 ]; then
  fail "a failing lane's exit status propagates" "expected 3, got ${rc}"
else
  pass "a failing lane's exit status propagates"
fi
case "$(last_line)" in
  'GATE: FAIL demo at boom (exit 3, '*'s)') pass 'the FAIL verdict names the failing lane' ;;
  *) fail 'the FAIL verdict names the failing lane' "last line: $(last_line)" ;;
esac
case "$out" in
  *'lane also-ok ran'*) fail 'a failing lane stops the run' 'a later lane still ran' ;;
  *) pass 'a failing lane stops the run' ;;
esac

# --- a run with no lanes is still a run, and still reports a verdict ---------

run_gate "$state" demo
if [ "$rc" -ne 0 ]; then
  fail 'an empty lane list exits 0' "exited ${rc}"
elif [ "$(last_line)" != 'GATE: PASS demo (0 lanes, 0s)' ]; then
  fail 'an empty lane list still reports a verdict' "last line: $(last_line)"
else
  pass 'an empty lane list still reports a verdict'
fi

# --- names that are not lane names are rejected rather than run --------------

for bad in '--version' 'rust::check;id' 'a b' ''; do
  run_gate "$state" demo "$bad"
  label="an invalid lane name is rejected (${bad:-<empty>})"
  if [ "$rc" -eq 0 ]; then
    fail "$label" 'exited 0'
  elif ! printf '%s' "$out" | grep -q 'invalid lane name'; then
    fail "$label" "output: ${out}"
  else
    pass "$label"
  fi
done

# --- the record: one line per executed lane, plus a summary ------------------

record_dir="$(find "${state}/reverie/gate" -mindepth 1 -maxdepth 1 -type d -print -quit 2> /dev/null || true)"
if [ -z "$record_dir" ]; then
  fail 'runs are recorded under XDG_STATE_HOME' 'no record directory was created'
  printf '\ncannot continue without a record directory\n' >&2
  exit 1
fi
pass 'runs are recorded under XDG_STATE_HOME'

run_gate "$state" demo ok boom
latest="$(find "$record_dir" -maxdepth 1 -type f -name '*.jsonl' -print | sort | tail -1)"
if [ "$(grep -c '"lane":' "$latest")" -ne 2 ]; then
  fail 'the record carries one line per executed lane' "$(cat "$latest")"
else
  pass 'the record carries one line per executed lane'
fi
if ! grep -q '"verdict":"fail","failed_lane":"boom"' "$latest"; then
  fail 'the record names the failing lane' "$(cat "$latest")"
else
  pass 'the record names the failing lane'
fi
if ! grep -q '"event":"start","lanes":2,"plan":\["ok","boom"\]' "$latest"; then
  fail 'the record opens with the plan' "$(cat "$latest")"
else
  pass 'the record opens with the plan'
fi
if ! grep -q '"head":null,"dirty":null,"pid":[0-9][0-9]*}' "$latest"; then
  fail 'a run outside a git repository records a null tree and its pid' "$(cat "$latest")"
else
  pass 'a run outside a git repository records a null tree and its pid'
fi

# --- lane output never reaches the record ------------------------------------

run_gate "$state" demo leak
if ! printf '%s' "$out" | grep -qF "$sentinel"; then
  fail 'the fixture actually printed the sentinel' 'the leak lane produced no sentinel'
elif grep -rqF "$sentinel" "$record_dir"; then
  fail 'lane output stays out of the record' "$(grep -rlF "$sentinel" "$record_dir")"
else
  pass 'lane output stays out of the record'
fi

# --- --status reads back the last run ----------------------------------------

run_gate "$state" --status
if [ "$rc" -ne 0 ]; then
  fail '--status exits 0 after a passing run' "exited ${rc}"
elif ! printf '%s' "$out" | grep -q '"verdict":"pass"'; then
  fail '--status reports the last run' "output: ${out}"
else
  pass '--status exits 0 after a passing run'
fi

run_gate "$state" demo boom
run_gate "$state" --status
if [ "$rc" -ne 1 ]; then
  fail '--status exits 1 after a failing run' "expected 1, got ${rc}"
else
  pass '--status exits 1 after a failing run'
fi

# An unfinished run either died or is still going, told apart by whether the
# recorded runner pid is alive. Both fixtures share one far-future id so they
# sort last, ahead of any run this test has already recorded; the dead pid is
# a real, reaped child so the kernel cannot still be running it.
( : ) &
dead_pid=$!
wait "$dead_pid"
killed="${record_dir}/29991231T235959Z-0.jsonl"
cat > "$killed" << EOF
{"run":"29991231T235959Z-0","label":"demo","event":"start","lanes":3,"plan":["ok","boom","also-ok"],"started":"2999-12-31T23:59:59Z","head":null,"dirty":null,"pid":${dead_pid}}
{"run":"29991231T235959Z-0","lane":"ok","status":"pass","exit":0,"seconds":1}
EOF
run_gate "$state" --status
if [ "$rc" -ne 2 ]; then
  fail '--status distinguishes a run that never finished' "expected 2, got ${rc}"
elif ! printf '%s' "$out" | grep -q 'it stopped during boom'; then
  fail '--status names the lane a killed run stopped in' "output: ${out}"
else
  pass '--status names the lane a killed run stopped in, and exits 2'
fi

# The same record with a live pid (this test's own shell) is a run in
# progress, and must not read as killed: a poller that cannot tell the two
# apart abandons or restarts a gate that is minutes from finishing.
sed "s/\"pid\":${dead_pid}}/\"pid\":$$}/" "$killed" > "${killed}.tmp"
mv "${killed}.tmp" "$killed"
run_gate "$state" --status
if [ "$rc" -ne 3 ]; then
  fail '--status distinguishes a run still in progress' "expected 3, got ${rc}"
elif ! printf '%s' "$out" | grep -q 'still in progress, currently in boom'; then
  fail '--status names the lane a live run is in' "output: ${out}"
else
  pass '--status reports a live run as in progress, and exits 3'
fi

# A record written before the pid and tree fields existed has no liveness to
# check and must fall back to the killed-run answer, not a crash or a false
# green. The sed is asserted to have actually stripped the field, so this can
# never quietly re-test the live-pid case above.
sed 's/,"head":null,"dirty":null,"pid":[0-9]*}/}/' "$killed" > "${killed}.tmp"
mv "${killed}.tmp" "$killed"
if grep -q '"pid":' "$killed"; then
  fail '--status treats a legacy record without a pid as unfinished' 'fixture still carries a pid field'
else
  run_gate "$state" --status
  if [ "$rc" -ne 2 ]; then
    fail '--status treats a legacy record without a pid as unfinished' "expected 2, got ${rc}"
  else
    pass '--status treats a legacy record without a pid as unfinished'
  fi
fi
rm -f "$killed"

run_gate "${tmpdir}/empty-state" --status
if [ "$rc" -ne 4 ] || ! printf '%s' "$out" | grep -q 'no recorded gate run'; then
  fail '--status with no history says so and exits 4' "rc=${rc} output: ${out}"
else
  pass '--status with no history says so and exits 4'
fi

# --- an unwritable record location degrades, it does not fail the gate -------

: > "${tmpdir}/not-a-dir"
run_gate "${tmpdir}/not-a-dir" demo ok
if [ "$rc" -ne 0 ]; then
  fail 'an unwritable record location does not fail the gate' "exited ${rc}"
elif ! printf '%s' "$out" | grep -q 'continuing without a run record'; then
  fail 'an unwritable record location warns' "output: ${out}"
elif [ "$(last_line)" != 'GATE: PASS demo (1 lane, 0s: ok)' ]; then
  fail 'an unwritable record location still reports a verdict' "last line: $(last_line)"
else
  pass 'an unwritable record location degrades without failing the gate'
fi

# --- the record ties a verdict to the tree that earned it --------------------

# A verdict answers for the commit it ran on. Inside a git repository the
# start event carries HEAD and a dirty flag, and --status warns when the
# checkout has moved since, so an old green cannot quietly stand in for the
# current tree. The identity and hook overrides keep the fixture immune to
# whatever global git config the host carries.
git_fixture="${tmpdir}/gitlanes"
mkdir -p "$git_fixture"
cp "${lanes_dir}/justfile" "${git_fixture}/justfile"
git_q() { git -C "$git_fixture" -c user.name=fixture -c user.email=fixture@invalid -c commit.gpgsign=false -c core.hooksPath=/dev/null "$@"; }
git_q init -q
git_q add justfile
git_q commit -qm fixture
git_state="${tmpdir}/git-state"

run_gate_in "$git_fixture" "$git_state" demo ok
git_record_dir="$(find "${git_state}/reverie/gate" -mindepth 1 -maxdepth 1 -type d -print -quit 2> /dev/null || true)"
git_latest() { find "$git_record_dir" -maxdepth 1 -type f -name '*.jsonl' -print | sort | tail -1; }
head_before="$(git_q rev-parse HEAD)"
if ! grep -q "\"head\":\"${head_before}\",\"dirty\":false" "$(git_latest)"; then
  fail 'a clean run records the commit it was earned on' "$(cat "$(git_latest)")"
else
  pass 'a clean run records the commit it was earned on'
fi

# Dirty the tree after the clean run: HEAD is unchanged, so only the dirty
# comparison can notice that the verdict no longer covers the checkout.
: > "${git_fixture}/scratch"
run_gate_in "$git_fixture" "$git_state" --status
if [ "$rc" -ne 0 ]; then
  fail '--status keeps a passed verdict when the tree dirties' "exited ${rc}"
elif ! printf '%s' "$out" | grep -q 'uncommitted changes that run never saw'; then
  fail '--status warns when a clean-verified tree has been edited' "output: ${out}"
else
  pass '--status warns when a clean-verified tree has been edited'
fi

run_gate_in "$git_fixture" "$git_state" demo ok
if ! grep -q "\"head\":\"${head_before}\",\"dirty\":true" "$(git_latest)"; then
  fail 'an uncommitted tree records dirty' "$(cat "$(git_latest)")"
else
  pass 'an uncommitted tree records dirty'
fi

# A run recorded on a dirty tree can never be re-checked against anything:
# the uncommitted content it verified is gone the moment it changes. The
# reader is told so every time that verdict is replayed.
run_gate_in "$git_fixture" "$git_state" --status
if [ "$rc" -ne 0 ]; then
  fail '--status keeps a passed dirty-tree verdict' "exited ${rc}"
elif ! printf '%s' "$out" | grep -q 'that run verified a dirty tree'; then
  fail '--status notes a verdict earned on a dirty tree' "output: ${out}"
else
  pass '--status notes a verdict earned on a dirty tree'
fi

# Move HEAD past the recorded run: the verdict still stands (exit 0), but the
# reader is told it no longer describes this tree.
git_q add scratch
git_q commit -qm moved
run_gate_in "$git_fixture" "$git_state" --status
if [ "$rc" -ne 0 ]; then
  fail '--status keeps a passed verdict after HEAD moves' "exited ${rc}"
elif ! printf '%s' "$out" | grep -q 'the checkout has moved since that run'; then
  fail '--status warns when the checkout has outgrown the recorded run' "output: ${out}"
else
  pass '--status warns when the checkout has outgrown the recorded run'
fi

# --- pruning bounds the record directory -------------------------------------

before="$(find "$record_dir" -maxdepth 1 -type f -name '*.jsonl' | wc -l)"
for i in $(seq 1 25); do
  printf '%s\n' "{\"run\":\"20000101T00000${i}Z-0\",\"verdict\":\"pass\"}" \
    > "${record_dir}/20000101T0000$(printf '%02d' "$i")Z-0.jsonl"
done
# A record with no verdict may belong to a run that is still writing it. The
# count prune must step over it however far down the order it sits, or a
# gate would delete a slower neighbour's record out from under it; named
# oldest so only the verdict gate can be what spares it.
unfinished_seed="${record_dir}/19990101T000000Z-000000-1.jsonl"
printf '%s\n' '{"run":"19990101T000000Z-000000-1","label":"demo","event":"start","lanes":1,"plan":["ok"],"started":"1999-01-01T00:00:00Z"}' \
  > "$unfinished_seed"
run_gate "$state" demo ok
if [ ! -f "$unfinished_seed" ]; then
  fail 'the count prune spares a record that has no verdict yet' 'the unfinished record was deleted'
else
  pass 'the count prune spares a record that has no verdict yet'
fi
rm -f "$unfinished_seed"
after="$(find "$record_dir" -maxdepth 1 -type f -name '*.jsonl' | wc -l)"
if [ "$after" -gt 20 ]; then
  fail 'pruning bounds the record directory' "kept ${after} runs (was ${before} before seeding)"
else
  pass 'pruning bounds the record directory'
fi

if [ "$failures" -ne 0 ]; then
  printf '\n%d gate-run assertion(s) failed\n' "$failures" >&2
  exit 1
fi
echo 'OK: gate-run verdict, record, status, and pruning behaviour'
