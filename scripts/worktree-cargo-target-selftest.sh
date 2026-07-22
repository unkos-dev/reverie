#!/usr/bin/env bash
# Self-test `just worktree`'s per-worktree cargo target-dir isolation. The
# generated-reference drift check only reads recipe metadata and never
# executes the file write, so a wrong path or malformed cargo config would
# otherwise reach CI unnoticed. This runs the real recipe (not a copy)
# against disposable branches, so a regression in the recipe itself, not a
# stand-in, fails the build.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"

# A sibling of the checkout, not $TMPDIR: the recipe itself refuses to
# create a worktree on a tmpfs, and a sandboxed dev environment's default
# temp dir often is one, while the checkout's own filesystem never is. This
# mirrors the recipe's own default WORKTREE_ROOT (parent_directory of the
# repo), so this selftest passes everywhere the recipe itself would work.
scratch_root="$(mktemp -d "$(dirname "${repo_root}")/.worktree-cargo-target-selftest.XXXXXX")"

pids="$$"
branch_ok="test/worktree-cargo-target-selftest-ok-${pids}"
branch_dirty="test/worktree-cargo-target-selftest-dirty-${pids}"
slug_ok="${branch_ok//\//-}"
slug_dirty="${branch_dirty//\//-}"
dest_ok="${scratch_root}/reverie/${slug_ok}"
dest_dirty="${scratch_root}/reverie/${slug_dirty}"

# shellcheck disable=SC2329  # invoked via the EXIT trap
cleanup() {
  git -C "${repo_root}" worktree remove --force "${dest_ok}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_dirty}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree prune >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_ok}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_dirty}" >/dev/null 2>&1 || true
  rm -rf "${scratch_root}"
}
trap cleanup EXIT

fail=0
ok() { echo "ok   $1"; }
bad() { # <name> <detail...>
  echo "FAIL $1"
  shift
  for line in "$@"; do echo "     ${line}"; done
  fail=1
}

# --- happy path: creating a worktree writes an isolated cargo target-dir
# config, leaves git status clean (the ignore rule covers it), and the
# worktree can be removed without --force. ---

create_out=""
create_rc=0
create_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_ok}" 2>&1)" || create_rc=$?

if [ "${create_rc}" -eq 0 ]; then
  ok "worktree creation exits zero"
else
  bad "worktree creation exits zero" "exit ${create_rc}" "${create_out}"
fi

if [ -f "${dest_ok}/.cargo/config.toml" ]; then
  ok "worktree creation writes .cargo/config.toml"
else
  bad "worktree creation writes .cargo/config.toml" "no such file: ${dest_ok}/.cargo/config.toml"
fi

# Exact-content check, not a loose grep: a malformed key name, missing
# quotes, or wrong path would still contain the substring "target-dir" while
# being unusable to cargo.
expected_config=$'[build]\ntarget-dir = "target"\n'
actual_config="$(cat "${dest_ok}/.cargo/config.toml" 2>/dev/null || true)"$'\n'
if [ "${actual_config}" = "${expected_config}" ]; then
  ok "config.toml has the expected [build] target-dir content"
else
  bad "config.toml has the expected [build] target-dir content" \
    "want: ${expected_config@Q}" "got:  ${actual_config@Q}"
fi

# The ignore rule is what makes the status-clean and remove-without-force
# assertions below meaningful, so assert it directly rather than inferring
# it from their outcome.
if git -C "${dest_ok}" check-ignore -q .cargo/config.toml; then
  ok ".cargo/config.toml is covered by the ignore rule"
else
  bad ".cargo/config.toml is covered by the ignore rule" "git check-ignore did not match it"
fi

status_out="$(git -C "${dest_ok}" status --porcelain 2>&1 || true)"
if [ -z "${status_out}" ]; then
  ok "worktree git status is clean after creation"
else
  bad "worktree git status is clean after creation" "${status_out}"
fi

remove_out=""
remove_rc=0
remove_out="$(git -C "${repo_root}" worktree remove "${dest_ok}" 2>&1)" || remove_rc=$?
if [ "${remove_rc}" -eq 0 ]; then
  ok "git worktree remove succeeds without --force"
else
  bad "git worktree remove succeeds without --force" "exit ${remove_rc}" "${remove_out}"
fi

# --- edge case / negative control: prove the clean-removal assertion above
# is actually load-bearing, not just git's unconditional behavior, by
# showing removal without --force fails once a real (non-ignored) untracked
# file is present. Without this, a regression that made every worktree
# removal succeed unconditionally (e.g. via --force creeping into the
# recipe or a test helper) would go unnoticed. ---

dirty_create_rc=0
(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_dirty}") >/dev/null 2>&1 || dirty_create_rc=$?
if [ "${dirty_create_rc}" -ne 0 ]; then
  bad "edge case: dirty-worktree fixture creation exits zero" "exit ${dirty_create_rc}"
else
  echo "not tracked, not ignored" >"${dest_dirty}/stray.txt"
  dirty_remove_rc=0
  git -C "${repo_root}" worktree remove "${dest_dirty}" >/dev/null 2>&1 || dirty_remove_rc=$?
  if [ "${dirty_remove_rc}" -ne 0 ]; then
    ok "git worktree remove fails without --force when a real untracked file is present"
  else
    bad "git worktree remove fails without --force when a real untracked file is present" \
      "removal succeeded even with an untracked, non-ignored file present"
  fi
fi

exit "${fail}"
