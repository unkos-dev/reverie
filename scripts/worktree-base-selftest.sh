#!/usr/bin/env bash
# Self-test for scripts/worktree-base.sh. The resolver picks the start point
# for a brand-new worktree branch, so a silent bug in it looks exactly like a
# successful `just worktree` invocation that quietly bases the new branch on
# the wrong history; the bug this script exists to catch contaminated four
# real PR branches before it was caught in review. Every case runs against a
# small throwaway repo built under a mktemp dir, never the checkout running
# this test.
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

failures=0
ok() { printf 'ok   %s\n' "$1"; }
bad() { # <name> <detail...>
  printf 'FAIL %s\n' "$1"
  shift
  for line in "$@"; do printf '     %s\n' "$line"; done
  failures=$((failures + 1))
}

# The resolver derives its own repo root from $0, so a fixture must run its
# own copy from a scripts/ dir inside it: invoking the checkout's copy from a
# fixture cwd would still resolve refs against the real repo, not the
# fixture, and every case below would silently pass or fail against the
# wrong tree.
install_resolver() {
  local fixture="$1"
  mkdir -p "$fixture/scripts"
  cp "$repo_root/scripts/worktree-base.sh" "$fixture/scripts/worktree-base.sh"
}

# Assert the resolver's stdout for a fixture directory and argument list.
expect_output() {
  local name="$1" fixture="$2" want="$3"
  shift 3
  local got rc=0
  got="$(cd "$fixture" && ./scripts/worktree-base.sh "$@" 2> /dev/null)" || rc=$?
  if [ "$rc" -ne 0 ]; then
    bad "$name" "exited $rc, expected success"
    return
  fi
  if [ "$got" != "$want" ]; then
    bad "$name" "want: ${want}" "got:  ${got}"
    return
  fi
  ok "$name"
}

# Assert a nonzero exit, empty stdout, and a stderr diagnostic containing a
# given substring, so a rejection cannot pass by failing for the wrong reason
# or by leaking a start point on stdout despite the failure.
expect_failure() {
  local name="$1" fixture="$2" want_substr="$3"
  shift 3
  local rc=0 stdout_file out stdout_got
  stdout_file="$tmpdir/stdout.$$"
  (cd "$fixture" && ./scripts/worktree-base.sh "$@" 2> "$stdout_file.err" 1> "$stdout_file.out") || rc=$?
  out="$(cat "$stdout_file.err" 2> /dev/null || true)"
  stdout_got="$(cat "$stdout_file.out" 2> /dev/null || true)"
  rm -f "$stdout_file.err" "$stdout_file.out"
  if [ "$rc" -eq 0 ]; then
    bad "$name" "exited 0, expected failure"
    return
  fi
  if [ -n "$stdout_got" ]; then
    bad "$name" "unexpected stdout on failure: ${stdout_got}"
    return
  fi
  case "$out" in
    *"$want_substr"*) ok "$name" ;;
    *) bad "$name" "stderr did not contain '${want_substr}'" "stderr: ${out}" ;;
  esac
}

# --- fixture: a repo with both origin/main and a local main ----------------
# origin/main must win even when a local main also exists, so this fixture
# deliberately carries both.
both_repo="$tmpdir/both"
mkdir -p "$both_repo"
git init -q -b main "$both_repo"
git -C "$both_repo" config user.email selftest@example.invalid
git -C "$both_repo" config user.name 'Worktree Base Selftest'
echo one > "$both_repo/file.txt"
git -C "$both_repo" add -A
git -C "$both_repo" commit -qm 'base'
# No real remote is needed: show-ref only cares that the ref exists under
# refs/remotes/origin, not that a remote named "origin" is configured.
git -C "$both_repo" update-ref refs/remotes/origin/main "$(git -C "$both_repo" rev-parse HEAD)"
install_resolver "$both_repo"

expect_output 'existing origin/main is preferred over a local main' \
  "$both_repo" 'origin origin/main' \
  some-new-branch

# --- fixture: origin/main absent, local main present ------------------------
local_only_repo="$tmpdir/local-only"
mkdir -p "$local_only_repo"
git init -q -b main "$local_only_repo"
git -C "$local_only_repo" config user.email selftest@example.invalid
git -C "$local_only_repo" config user.name 'Worktree Base Selftest'
echo one > "$local_only_repo/file.txt"
git -C "$local_only_repo" add -A
git -C "$local_only_repo" commit -qm 'base'
install_resolver "$local_only_repo"

expect_output 'local main is used when origin/main is absent' \
  "$local_only_repo" 'local main' \
  some-new-branch

# --- fixture: neither origin/main nor local main exists ---------------------
# Current branch is named "trunk", not "main", so refs/heads/main is genuinely
# absent rather than merely untracked.
neither_repo="$tmpdir/neither"
mkdir -p "$neither_repo"
git init -q -b trunk "$neither_repo"
git -C "$neither_repo" config user.email selftest@example.invalid
git -C "$neither_repo" config user.name 'Worktree Base Selftest'
echo one > "$neither_repo/file.txt"
git -C "$neither_repo" add -A
git -C "$neither_repo" commit -qm 'base'
install_resolver "$neither_repo"

expect_failure 'missing base refs refuse to fall back to the caller HEAD' \
  "$neither_repo" 'refusing to guess a base' \
  some-new-branch

expect_failure 'the refusal names the fetch command that restores the base' \
  "$neither_repo" 'git fetch origin main' \
  some-new-branch

expect_output 'an explicit HEAD opts into basing on the current checkout' \
  "$neither_repo" 'explicit HEAD' \
  some-new-branch HEAD

# --- explicit base --------------------------------------------------------
# Reuse the "both" fixture: an explicit base must win even though origin/main
# and local main both resolve, proving precedence rather than only presence.
explicit_target="$(git -C "$both_repo" rev-parse HEAD)"
git -C "$both_repo" branch other-base
expect_output 'an explicit base overrides the resolution chain' \
  "$both_repo" "explicit other-base" \
  some-new-branch other-base

expect_output 'an explicit base still works when it is a bare commit-ish' \
  "$both_repo" "explicit ${explicit_target}" \
  some-new-branch "$explicit_target"

expect_failure 'an explicit base that does not resolve is rejected' \
  "$both_repo" 'does not resolve to a commit' \
  some-new-branch definitely-not-a-real-ref

if [ "$failures" -ne 0 ]; then
  printf '\n%d check(s) failed\n' "$failures" >&2
  exit 1
fi
printf '\nall checks passed\n'
