#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export TEST_ROOT="$tmp/checkout with spaces" TEST_TOOLS="$tmp/tool installs" TEST_LOG="$tmp/commands"
mkdir -p "$TEST_ROOT/scripts" "$TEST_ROOT/backend" "$TEST_TOOLS/bin" "$tmp/bin" "$tmp/registry"
ln -s "$(command -v just)" "$tmp/bin/just"
cp "$repo_root/scripts/rust-exec.sh" "$TEST_ROOT/scripts/"
export HOME="$tmp/home"
mkdir -p "$HOME"
unset CI RUSTC_WRAPPER KACHE_SOCKET_PATH
export PATH="$tmp/bin:/usr/bin:/bin"
export MISE_AUTO_INSTALL=1
cat >"$tmp/bin/mise" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
[[ $MISE_AUTO_INSTALL == 0 && $1 == -C && $2 == "$TEST_ROOT" ]]
shift 2
printf 'mise %s\n' "$*" >>"$TEST_LOG"
[[ ${TEST_MISSING:-0} == 0 ]] || exit 19
case "$1" in
  where) printf '%s\n' "$TEST_TOOLS" ;;
  exec)
    shift
    while [[ $1 != -- ]]; do shift; done
    shift
    export PATH="$TEST_TOOLS/bin:$PATH"
    cd "$TEST_ROOT"
    exec "$@"
    ;;
  *) exit 99 ;;
esac
STUB
cat >"$tmp/bin/kache-lifecycle" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
[[ $# == 4 && $1 == inspect && $2 == --client && $3 == "$TEST_TOOLS/bin/kache" && $4 == --json ]]
printf 'inspect\n' >>"$TEST_LOG"
[[ ${TEST_STATE:-compatible} != malformed ]] || { echo '{'; exit 0; }
code=0
case "${TEST_STATE:-compatible}" in
  incompatible|unhealthy) code=1 ;;
  unknown) code=2 ;;
esac
jq -n --arg client "$3" --arg state "${TEST_STATE:-compatible}" --arg reason "${TEST_REASON:-accepted identity}" '
  {schema_version:1,state:$state,reason:$reason,client_path:$client,client_epoch:100,
   daemon_path:$client,daemon_epoch:100,main_pid:42,start_ticks:50,n_restarts:0,
   socket_path:"/tmp/managed.sock",endpoint:"untested",remedy_argv:["kache-lifecycle","update"]}
  | if env.TEST_BAD == "schema" then .schema_version=2
    elif env.TEST_BAD == "client" then .client_path="/wrong/kache"
    elif env.TEST_BAD == "socket" then .socket_path=null
    elif env.TEST_BAD == "pid" then .main_pid=0
    elif env.TEST_BAD == "endpoint" then .endpoint="healthy"
    else . end'
exit "${TEST_EXIT:-$code}"
STUB
cat >"$TEST_TOOLS/bin/kache" <<'STUB'
#!/usr/bin/env bash
printf 'KACHE CONTACT\n' >>"$TEST_LOG"
exit 99
STUB
cat >"$tmp/bin/cargo" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'cargo\n' >>"$TEST_LOG"
jq -n --args '{argv:$ARGS.positional,cwd:env.PWD,wrapper:env.RUSTC_WRAPPER,socket:env.KACHE_SOCKET_PATH,
  sqlx:env.SQLX_OFFLINE,dsn:env.DATABASE_URL,migrator:env.DATABASE_URL_MIGRATION,toolchain:env.RUSTUP_TOOLCHAIN,
  target:env.CARGO_TARGET_DIR,build_target:env.CARGO_BUILD_TARGET_DIR,auto:env.MISE_AUTO_INSTALL}' -- "$@" >"$TEST_RESULT"
exit "${TEST_CARGO_EXIT:-0}"
STUB
for tool in mold cargo-nextest cargo-machete cargo-deny; do
  cp "$tmp/bin/cargo" "$TEST_TOOLS/bin/$tool"
done
chmod +x "$tmp/bin/mise" "$tmp/bin/cargo" "$tmp/bin/kache-lifecycle" "$TEST_TOOLS/bin/"*
export TEST_RESULT="$tmp/result"
helper="$TEST_ROOT/scripts/rust-exec.sh"
count=0
absent() { if grep -q "$1" "$2"; then echo "unexpected invocation" >&2; exit 1; fi; }
run() {
  local expected=$1
  shift
  : >"$TEST_LOG"
  rm -f "$TEST_RESULT"
  local rc=0
  "$@" >"$tmp/out" 2>&1 || rc=$?
  if [[ $rc != "$expected" ]]; then
    cat "$tmp/out"
    echo "FAIL: expected $expected, got $rc" >&2
    exit 1
  fi
  count=$((count + 1))
}
rejected() {
  run 1 "$helper" compile -- cargo build --locked
  [[ ! -e $TEST_RESULT ]]
  absent 'KACHE CONTACT' "$TEST_LOG"
}
cd "$TEST_ROOT/backend"
export SQLX_OFFLINE=true DATABASE_URL="fixture dsn with spaces;\$literal"
export DATABASE_URL_MIGRATION="fixture migrator dsn;\$literal"
export RUSTUP_TOOLCHAIN=fixture CARGO_TARGET_DIR="$tmp/target" CARGO_BUILD_TARGET_DIR="$tmp/build target"
run 0 "$helper" compile -- cargo build --locked 'arg with spaces' "\$literal" ''
jq -e --arg cwd "$PWD" --arg wrapper "$TEST_TOOLS/bin/kache" --arg target "$CARGO_TARGET_DIR" --arg build "$CARGO_BUILD_TARGET_DIR" '
  .argv == ["build","--locked","arg with spaces","$literal",""] and .cwd==$cwd and
  .wrapper==$wrapper and .socket=="/tmp/managed.sock" and .sqlx=="true" and
  .dsn=="fixture dsn with spaces;$literal" and .migrator=="fixture migrator dsn;$literal" and .toolchain=="fixture" and
  .target==$target and .build_target==$build and .auto=="0"' "$TEST_RESULT" >/dev/null
grep -q '^inspect$' "$TEST_LOG"
absent 'KACHE CONTACT' "$TEST_LOG"
cd "$tmp/registry"
run 0 bash --noprofile --norc -ic 'exec "$@"' _ "$helper" compile -- cargo build --locked
jq -e --arg cwd "$PWD" --arg wrapper "$TEST_TOOLS/bin/kache" '.cwd==$cwd and .wrapper==$wrapper' "$TEST_RESULT" >/dev/null
run 0 bash --noprofile --norc "$helper" nextest -- cargo nextest run --locked
jq -e --arg cwd "$PWD" '.cwd==$cwd' "$TEST_RESULT" >/dev/null
grep -q 'exec mold github:kunobi-ninja/kache github:nextest-rs/nextest --' "$TEST_LOG"
for state in incompatible unhealthy unknown malformed; do
  export TEST_STATE=$state
  rejected
done
unset TEST_STATE
for bad in schema client socket pid endpoint; do
  export TEST_BAD=$bad
  rejected
done
unset TEST_BAD
export TEST_EXIT=1
rejected
unset TEST_EXIT
export TEST_REASON='accepted older pair'
run 0 "$helper" compile -- cargo build --locked
unset TEST_REASON
for reason in 'newer client' 'equal unrecognised client'; do
  export TEST_REASON=$reason TEST_STATE=incompatible
  rejected
done
unset TEST_REASON TEST_STATE
export KACHE_SOCKET_PATH=/tmp/other.sock
rejected
unset KACHE_SOCKET_PATH
export TEST_MISSING=1
rejected
unset TEST_MISSING
mv "$tmp/bin/kache-lifecycle" "$tmp/inspector"
rejected
for wrapper in '' '/ci/wrapper with spaces'; do
  export CI=true RUSTC_WRAPPER="$wrapper"
  run 0 "$helper" compile -- cargo build --locked
  jq -e --arg wrapper "$wrapper" '.wrapper==$wrapper' "$TEST_RESULT" >/dev/null
  absent 'inspect\|kunobi-ninja/kache' "$TEST_LOG"
done
unset RUSTC_WRAPPER
rejected
unset CI
mv "$tmp/inspector" "$tmp/bin/kache-lifecycle"
for mode in machete deny tools; do
  run 0 "$helper" "$mode" -- cargo "$mode"
  absent 'inspect\|kunobi-ninja/kache' "$TEST_LOG"
done
export TEST_CARGO_EXIT=37
run 37 "$helper" compile -- cargo build --locked
unset TEST_CARGO_EXIT

cp "$repo_root/justfile" "$repo_root/"*.just "$TEST_ROOT/"
cat >"$TEST_ROOT/scripts/backend-dev-env.sh" <<'STUB'
export REVERIE_PORT=9000
STUB
cat >"$TEST_ROOT/scripts/dev-server.sh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'dev-server %s\n' "$1" >>"$TEST_LOG"
if [[ $1 == start ]]; then
  exec setsid --wait bash -c "$DEV_SERVER_CMD"
fi
STUB
chmod +x "$TEST_ROOT/scripts/dev-server.sh"
for recipe in clippy doc-lint drift test doctests sqlx-check sqlx-prepare regen build dev migrate dev-start; do
  run 0 just --justfile "$TEST_ROOT/justfile" "rust::$recipe"
  [[ -f $TEST_RESULT ]]
  grep -q '^inspect$' "$TEST_LOG"
  jq -e --arg cwd "$TEST_ROOT/backend" --arg wrapper "$TEST_TOOLS/bin/kache" '
    .cwd==$cwd and .wrapper==$wrapper and .socket=="/tmp/managed.sock" and (.argv|index("--locked")!=null)' "$TEST_RESULT" >/dev/null
done
run 0 just --justfile "$TEST_ROOT/justfile" db-migrate
grep -q '^inspect$' "$TEST_LOG"
jq -e '.argv==["run","--locked","--","migrate"] and .migrator=="fixture migrator dsn;$literal"' "$TEST_RESULT" >/dev/null
for recipe in rust::machete rust::deny db-migrate-raw; do
  run 0 just --justfile "$TEST_ROOT/justfile" "$recipe"
  [[ -f $TEST_RESULT ]]
  absent '^inspect$' "$TEST_LOG"
done
export TEST_MISSING=1
for recipe in rust::dev-stop rust::dev-status; do
  run 0 just --justfile "$TEST_ROOT/justfile" "$recipe"
  absent '^mise ' "$TEST_LOG"
  [[ ! -e $TEST_RESULT ]]
done
run 0 just --justfile "$TEST_ROOT/justfile" --list
[[ ! -s $TEST_LOG ]]
unset TEST_MISSING
if (TEST_STATE=compatible rejected) >/dev/null 2>&1; then
  echo "FAIL: refusal assertion accepts successful compilation" >&2
  exit 1
fi
count=$((count + 1))

[[ $count -gt 0 ]]
printf 'rust-exec-selftest: PASS (%s cases)\n' "$count"
