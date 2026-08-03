#!/usr/bin/env bash
# Self-test scripts/cargo-locked-guard.sh against fixture files, so the
# guard's own matching cannot drift silently: a guard that stops matching is
# indistinguishable from a clean tree.
set -ueo pipefail

repo_root="$(git rev-parse --show-toplevel)"
guard="${repo_root}/scripts/cargo-locked-guard.sh"

failures=0
pass() { printf 'ok   %s\n' "$1"; }
fail() {
  printf 'FAIL %s\n  %s\n' "$1" "$2"
  failures=$((failures + 1))
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

good="${tmpdir}/good.just"
cat > "$good" << 'EOF'
# A comment naming `cargo build` without --locked stays a comment.
build:
    SQLX_OFFLINE=true cargo build --workspace --locked
    # an indented comment about cargo test also stays a comment
    cargo test --locked --doc --workspace
    cargo sqlx prepare --check -- --locked --tests
    cargo fmt --all
    cargo machete
    cargo deny check
    cargo sqlx migrate run
EOF

bad="${tmpdir}/bad.just"
cat > "$bad" << 'EOF'
build:
    cargo build --workspace
EOF

rc=0
"$guard" "$good" > /dev/null 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then
  pass 'locked, exempt, and comment lines all pass'
else
  fail 'locked, exempt, and comment lines all pass' "guard exited ${rc} on the clean fixture"
fi

rc=0
out="$("$guard" "$bad" 2>&1)" || rc=$?
if [ "$rc" -ne 1 ]; then
  fail 'an unlocked resolving invocation fails the guard' "exited ${rc}, expected 1"
elif ! printf '%s' "$out" | grep -qF "${bad}:2"; then
  fail 'the violation names its file and line' "output: ${out}"
else
  pass 'an unlocked resolving invocation fails the guard, naming file and line'
fi

rc=0
"$guard" > /dev/null 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then
  pass 'the real just recipes are clean'
else
  fail 'the real just recipes are clean' "guard exited ${rc} against the repo defaults"
fi

if [ "$failures" -ne 0 ]; then
  printf '\n%d cargo-locked-guard assertion(s) failed\n' "$failures" >&2
  exit 1
fi
echo 'OK: cargo-locked-guard passes clean input, fails unlocked invocations, and the repo recipes are clean'
