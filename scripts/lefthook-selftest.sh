#!/usr/bin/env bash
#
# Behavioral self-test for the lefthook git-hook config. Builds a throwaway git
# repo, installs the real lefthook.yml, and asserts hook firing, the doublestar
# glob engine, stage_fixed re-staging, the accepted partial-stage divergence,
# and the index-lock-free formatter group.
#
# The temp repo lives UNDER the repo root so node resolves the repo's
# node_modules: oxfmt, markdownlint, and commitlint are node-local, unlike the
# system-PATH linters. The lint config files the hooks consult are copied in so
# the temp repo behaves like the real one. No network, no database, so it is
# safe to gate in CI.
#
# Run from anywhere; exits non-zero on any mismatch.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
LEFTHOOK="$ROOT/node_modules/.bin/lefthook"

T="$(mktemp -d -p "$ROOT")"
trap 'rm -rf "$T"' EXIT

cp "$ROOT/lefthook.yml" "$T/"
cp -r "$ROOT/scripts" "$T/"
cp "$ROOT/.oxfmtrc.json" "$ROOT/.yamllint.yaml" "$ROOT/_typos.toml" "$T/"

cd "$T"
git init -q
git config user.email selftest@reverie.invalid
git config user.name "lefthook selftest"
git config commit.gpgsign false
export PATH="$ROOT/node_modules/.bin:$PATH"
"$LEFTHOOK" install >/dev/null

fail=0
pass() { echo "ok   $1"; }
bad() {
  echo "FAIL $1" >&2
  fail=1
}

# Returns the commit/hook exit code without aborting under set -e.
commit() { git commit "$@" >/dev/null 2>&1; }

mkdir -p backend/src

# 1. Happy path: a clean staged file commits.
printf 'pub fn two() -> u8 {\n    2\n}\n' >backend/src/lib.rs
git add backend/src/lib.rs
if commit -m "test: clean file"; then
  pass "clean commit succeeds"
else
  bad "clean commit was blocked"
fi

# 2. Issue-ref guard blocks a backend source file.
printf 'pub const N: u8 = 0; // UNK-123\n' >backend/src/bad.rs
git add backend/src/bad.rs
if commit -m "test: bad ref"; then
  bad "issue-ref violation was not blocked"
else
  pass "issue-ref guard blocks a UNK reference"
fi
git reset -q HEAD backend/src/bad.rs
rm -f backend/src/bad.rs

# 3. commit-msg rejects a non-conventional message.
if git commit --allow-empty -m "Bad Capitalized subject" >/dev/null 2>&1; then
  bad "non-conventional commit message was not blocked"
else
  pass "commit-msg blocks a non-conventional message"
fi

# 4. doublestar glob matrix: a file directly under backend/src AND a nested one
#    are both handed to the rust-scoped guard. The gobwas default would skip the
#    direct one; no-issue-refs prints every matched path, so both must appear.
mkdir -p backend/src/nested
printf 'pub const A: u8 = 0; // UNK-1\n' >backend/src/main.rs
printf 'pub const B: u8 = 0; // UNK-2\n' >backend/src/nested/inner.rs
git add backend/src/main.rs backend/src/nested/inner.rs
out="$("$LEFTHOOK" run pre-commit 2>&1 || true)"
if grep -q "backend/src/main.rs" <<<"$out" && grep -q "backend/src/nested/inner.rs" <<<"$out"; then
  pass "doublestar matches direct-under-base and nested files"
else
  bad "doublestar glob missed a file (direct-under-base or nested)"
fi
git reset -q HEAD backend/src/main.rs backend/src/nested/inner.rs
rm -f backend/src/main.rs backend/src/nested/inner.rs

# 5. stage_fixed re-stages the formatter output: an unformatted file commits as
#    its formatted bytes.
printf 'x=1\ny=2\n' >restage.toml
git add restage.toml
if commit -m "test: restage"; then
  if [ "$(git show HEAD:restage.toml)" = "$(printf 'x = 1\ny = 2')" ]; then
    pass "stage_fixed re-stages oxfmt output"
  else
    bad "committed blob is not the formatted content"
  fi
else
  bad "restage commit was blocked"
fi

# 6. Partial-stage fidelity: with one hunk staged and another left dirty, the
#    staged hunk is formatted and committed while the unstaged hunk is excluded
#    from the commit AND preserved in the working tree. lefthook hides unstaged
#    changes during the hook, matching lint-staged, so stage_fixed never leaks an
#    unstaged hunk into the commit.
printf 'a = 1\nb = 2\nc = 3\n' >partial.toml
git add partial.toml
if ! commit -m "test: partial base"; then
  bad "partial-stage base commit was blocked"
fi
# Stage an unformatted hunk (a), then dirty a second hunk (c) without staging it.
printf 'a=10\nb = 2\nc = 3\n' >partial.toml
git add partial.toml
printf 'a=10\nb = 2\nc=30\n' >partial.toml
if commit -m "test: partial commit"; then
  committed="$(git show HEAD:partial.toml)"
  if [ "$committed" = "$(printf 'a = 10\nb = 2\nc = 3')" ] && grep -q 'c=30' partial.toml; then
    pass "partial-stage fidelity (staged hunk formatted+committed, unstaged hunk excluded+preserved)"
  else
    bad "partial-stage fidelity broken (unstaged hunk leaked or was lost)"
  fi
else
  bad "partial-stage commit was blocked"
fi

# 7. Index-lock-free formatter group: staging a YAML and another oxfmt-owned type
#    together fires both stage_fixed commands. The piped group serialises their
#    git-add calls, so repeated commits never hit index.lock.
race_fail=""
for i in 1 2 3 4 5; do
  printf 'key%s: %s\n' "$i" "$i" >race.yml
  printf 'num%s = %s\n' "$i" "$i" >race.toml
  git add race.yml race.toml
  if out="$(git commit -m "test: race $i" 2>&1)"; then
    if grep -qi 'index.lock' <<<"$out"; then
      race_fail="index.lock race on commit $i"
      break
    fi
  else
    race_fail="race commit $i failed: $out"
    break
  fi
done
if [ -z "$race_fail" ]; then
  pass "no index.lock race across repeated yaml+toml commits"
else
  bad "$race_fail"
fi

if [ "$fail" -ne 0 ]; then
  echo "lefthook self-test: FAILED" >&2
  exit 1
fi
echo "lefthook self-test: passed"
