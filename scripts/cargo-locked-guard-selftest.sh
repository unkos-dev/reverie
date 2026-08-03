#!/usr/bin/env bash
# Self-test scripts/cargo-locked-guard.sh against fixture files, so the
# guard's own matching cannot drift silently: a guard that stops matching is
# indistinguishable from a clean tree. Every resolving alternative gets its
# own violating line with an asserted total, so dropping one from the guard
# fails here instead of passing quietly.
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
    cargo llvm-cov nextest --workspace --locked
    cargo build --workspace --locked # a trailing comment on a locked line
    echo done # prose mentioning cargo build in a trailing comment only
    cargo fmt --all
    cargo machete
    cargo deny check
    cargo sqlx migrate run
    mise exec mold -- cargo install sqlx-cli --version 1.2.3
EOF

# One unlocked line per resolving subcommand the repo uses, plus the two
# bypass shapes: an unknown subcommand (must fail loudly, not pass as
# unrecognised) and a trailing comment on an unlocked line (must not exempt
# the code before it).
bad="${tmpdir}/bad.just"
cat > "$bad" << 'EOF'
build:
    cargo build --workspace
    cargo check --workspace
    cargo clippy --workspace
    cargo doc --no-deps
    cargo llvm-cov nextest --workspace
    cargo nextest run --workspace
    cargo run
    cargo test --doc
    cargo sqlx prepare -- --tests
    cargo bench --workspace
    cargo install left-pad
    cargo build --workspace # TODO: lock this later
EOF
expected_violations=12

rc=0
"$guard" "$good" > /dev/null 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then
  pass 'locked, exempt, and comment lines all pass'
else
  fail 'locked, exempt, and comment lines all pass' "guard exited ${rc} on the clean fixture"
fi

rc=0
out="$("$guard" "$bad" 2>&1)" || rc=$?
violations="$(printf '%s\n' "$out" | grep -c 'missing --locked' || true)"
if [ "$rc" -ne 1 ]; then
  fail 'unlocked resolving invocations fail the guard' "exited ${rc}, expected 1"
elif [ "$violations" -ne "$expected_violations" ]; then
  fail 'every violating alternative is reported' "counted ${violations}, expected ${expected_violations}: ${out}"
elif ! printf '%s' "$out" | grep -qF "${bad}:2"; then
  fail 'a violation names its file and line' "output: ${out}"
else
  pass "all ${expected_violations} violating alternatives fail, naming file and line"
fi

rc=0
"$guard" "${tmpdir}/does-not-exist.just" > /dev/null 2>&1 || rc=$?
if [ "$rc" -eq 2 ]; then
  pass 'an unreadable input aborts instead of reporting clean'
else
  fail 'an unreadable input aborts instead of reporting clean' "exited ${rc}, expected 2"
fi

rc=0
"$guard" > /dev/null 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then
  pass 'the real build and gate surfaces are clean'
else
  fail 'the real build and gate surfaces are clean' "guard exited ${rc} against the repo defaults"
fi

if [ "$failures" -ne 0 ]; then
  printf '\n%d cargo-locked-guard assertion(s) failed\n' "$failures" >&2
  exit 1
fi
echo 'OK: cargo-locked-guard fails every unlocked alternative, aborts on unreadable input, and the repo surfaces are clean'
