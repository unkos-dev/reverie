#!/usr/bin/env bash
#
# Self-test for scripts/dco-check.sh. Builds a throwaway git repo under a
# tempdir with signed-off, unsigned, and merge commits, and asserts the
# checker passes and fails on the right ranges. No fixture history touches
# the real repository.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
checker="${repo_root}/scripts/dco-check.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

git -C "${tmp}" init -q -b main
git -C "${tmp}" config user.name "DCO Selftest"
git -C "${tmp}" config user.email "selftest@example.invalid"
git -C "${tmp}" config commit.gpgsign false

commit() { # <file> <commit flags...>
  local file="$1"
  shift
  echo x >>"${tmp}/${file}"
  git -C "${tmp}" add "${file}"
  git -C "${tmp}" commit -q "$@"
}

fail=0
expect() { # <name> <expected-exit> <base> <head>
  local name="$1" want="$2" base="$3" head="$4" got=0
  (cd "${tmp}" && "${checker}" "${base}" "${head}") >/dev/null || got=$?
  if [ "${got}" -ne "${want}" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${got}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

# The base commit is excluded from every base..head range, so it can stay
# unsigned without affecting any case.
commit a.txt -m "chore: base"
git -C "${tmp}" tag base

commit a.txt -s -m "chore: signed one"
commit a.txt -s -m "chore: signed two"
git -C "${tmp}" tag signed
expect "all commits signed off" 0 base signed

expect "empty range" 0 signed signed

# The merge commit itself carries no trailer; --no-merges must exempt it.
git -C "${tmp}" checkout -q -b side base
commit b.txt -s -m "chore: side signed"
git -C "${tmp}" checkout -q main
git -C "${tmp}" merge -q --no-ff --no-edit side
git -C "${tmp}" tag merged
expect "unsigned merge commit exempt" 0 base merged

commit a.txt -m "chore: unsigned"
git -C "${tmp}" tag unsigned
expect "unsigned commit rejected" 1 base unsigned

exit "${fail}"
