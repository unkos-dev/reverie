#!/usr/bin/env bash
#
# Self-test for scripts/no-issue-refs.sh. The checker enumerates the whole
# tracked tree via `git ls-files` and ignores its arguments, so this pins
# behaviour by file placement (tracked vs untracked, gated vs excluded) in a
# throwaway git repo under a tempdir rather than by a changed-files list. No
# fixture content touches the real repository.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
checker="${repo_root}/scripts/no-issue-refs.sh"

# Built at runtime so this file never contains the literal banned token; the
# checker walks its own source tree and must not flag this self-test.
tracker_ref="$(printf 'UN''K-%s' 123)"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

git -C "${tmp}" init -q -b main
git -C "${tmp}" config user.name "Issue-ref Selftest"
git -C "${tmp}" config user.email "selftest@example.invalid"
git -C "${tmp}" config commit.gpgsign false
git -C "${tmp}" commit -q --allow-empty -m "chore: base"

track() { # <path> <content> — written to the working tree and staged
  local path="$1" content="$2"
  mkdir -p "${tmp}/$(dirname "$path")"
  printf '%s\n' "$content" >"${tmp}/${path}"
  git -C "${tmp}" add -- "$path"
}

fail=0
# Every scenario passes an argument naming a file the scenario never
# touches: the checker must ignore it, so each call also proves enforcement
# does not depend on the caller's changed-files list.
expect() { # <name> <expected-exit>
  local name="$1" want="$2" got=0
  (cd "$tmp" && "$checker" "not-a-real-file.rs") >/dev/null 2>&1 || got=$?
  if [ "$got" -ne "$want" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${got}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

track backend/src/lib.rs "Validates the request."
git -C "${tmp}" commit -q -m "chore: clean source"
expect "clean tree passes" 0

track backend/src/violation.rs "See ${tracker_ref} for context."
git -C "${tmp}" commit -q -m "chore: introduce a violation"
expect "tracked gated violation fails on the whole tree" 1

git -C "${tmp}" rm -q --cached -- backend/src/violation.rs
rm -f "${tmp}/backend/src/violation.rs"
git -C "${tmp}" commit -q -m "chore: remove the violation"

track AGENTS.md "Process notes referencing ${tracker_ref}."
git -C "${tmp}" commit -q -m "chore: agent notes"
expect "agent-process file excluded despite the pattern" 0

track frontend/src/components/ui/button.tsx "// vendored primitive citing ${tracker_ref}"
git -C "${tmp}" commit -q -m "chore: vendored ui primitive"
expect "vendored ui primitive excluded despite the pattern" 0

# Written to the working tree but never staged: `git ls-files` cannot see it.
mkdir -p "${tmp}/backend/src"
printf '%s\n' "Local scratch note: ${tracker_ref}" >"${tmp}/backend/src/scratch.rs"
expect "untracked gated file ignored" 0

track "backend/src/nested/needs quoting.rs" "Flags ${tracker_ref} even with a space in the path."
git -C "${tmp}" commit -q -m "chore: filename with a space"
expect "gated filename with a space still enforced" 1

exit "$fail"
