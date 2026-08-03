#!/usr/bin/env bash
# Self-test for scripts/preflight-scope.sh. The scoper decides what NOT to
# run, so a silent bug in it looks exactly like a fast, green preflight: the
# expensive failure mode is a lane wrongly omitted, which no other test can
# catch. Every case here pins an exact lane list rather than asserting a
# substring, so a lane quietly dropped from a mapping fails loudly.
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$repo_root"

scope='scripts/preflight-scope.sh'
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

failures=0

# Assert the lane list produced for a newline-separated set of changed paths.
expect_lanes() {
  local name="$1" paths="$2" want="$3" got rc=0
  got="$(printf '%s' "$paths" | "$scope" --files-from - 2> /dev/null)" || rc=$?
  if [ "$rc" -ne 0 ]; then
    printf 'FAIL %s: exited %d, expected success\n' "$name" "$rc"
    failures=$((failures + 1))
    return
  fi
  if [ "$got" != "$want" ]; then
    printf 'FAIL %s\n  want: %s\n  got:  %s\n' "$name" "$(echo "$want" | tr '\n' ' ')" "$(echo "$got" | tr '\n' ' ')"
    failures=$((failures + 1))
    return
  fi
  printf 'ok   %s\n' "$name"
}

# Assert a nonzero exit whose diagnostics mention a given substring, so a
# rejection cannot pass the test by failing for an unrelated reason.
expect_failure() {
  local name="$1" want_substr="$2"
  shift 2
  local out rc=0
  out="$("$@" 2>&1)" || rc=$?
  if [ "$rc" -eq 0 ]; then
    printf 'FAIL %s: exited 0, expected failure\n' "$name"
    failures=$((failures + 1))
    return
  fi
  case "$out" in
    *"$want_substr"*) printf 'ok   %s\n' "$name" ;;
    *)
      printf 'FAIL %s: failed as expected but without %q\n  output: %s\n' "$name" "$want_substr" "$out"
      failures=$((failures + 1))
      ;;
  esac
}

FULL='rust::guards
infra::check
db-up
rust::check
rust::doc-lint
rust::test
rust::doctests
rust::sqlx-check
rust::machete
rust::deny
js::check
js::test
js::build
js::font-integrity
js::a11y
docs::check
infra::zizmor'

BACKEND='rust::guards
infra::check
db-up
rust::check
rust::doc-lint
rust::test
rust::doctests
rust::sqlx-check
rust::machete'

FRONTEND='infra::check
js::check
js::test
js::build
js::font-integrity
js::a11y'

# --- happy paths: one filter each -------------------------------------------

expect_lanes 'backend source selects the rust lanes' \
  'backend/src/lib.rs' "$BACKEND"

expect_lanes 'a nested backend path still matches backend/**' \
  'backend/src/domain/deep/nested/mod.rs' "$BACKEND"

expect_lanes 'frontend source selects the js lanes' \
  'frontend/src/App.tsx' "$FRONTEND"

expect_lanes 'docs content selects the docs lane' \
  'docs/src/content/docs/index.mdx' 'infra::check
docs::check'

expect_lanes 'a workflow edit selects the zizmor audit' \
  '.github/workflows/docs.yml' 'infra::check
docs::check
infra::zizmor'

# A literal pattern must match only that exact path, never a sibling that
# shares its prefix; `vite.config.ts` is listed literally under frontend.
expect_lanes 'a literal pattern matches the exact path' \
  'vite.config.ts' "$FRONTEND"

expect_lanes 'a literal pattern does not match a prefix sibling' \
  'vite.config.ts.bak' 'infra::check'

# The committed filter file is double-quoted because the repo formatter
# rewrites single quotes; a hand-written single-quoted pattern must still
# parse, since a silently dropped pattern removes a lane from the gate.
printf '%s\n' 'frontend:' "  - 'hand/written.ts'" > "$tmpdir/quotes.yml"
got_quotes="$(printf 'hand/written.ts' | "$scope" --filters "$tmpdir/quotes.yml" --files-from -)"
if [ "$got_quotes" = "$FRONTEND" ]; then
  printf 'ok   %s\n' 'single-quoted patterns parse alongside double-quoted ones'
else
  printf 'FAIL %s\n  got: %s\n' 'single-quoted patterns parse alongside double-quoted ones' "$got_quotes"
  failures=$((failures + 1))
fi

# --- always-run lane ---------------------------------------------------------

# The repo-lint mirror is unconditional because CI's repo-lint job is: it
# lints the whole tree, so an unfiltered file can still break it.
expect_lanes 'an unfiltered file still runs the repo-lint mirror' \
  'README.md' 'infra::check'

expect_lanes 'no changed paths select nothing at all' '' ''

# A blank entry must be discarded, not treated as a path: the empty string is
# a prefix of every `**` pattern, so keeping it would select every lane.
expect_lanes 'a blank entry is discarded rather than matched' \
  'README.md
' 'infra::check'

# --- multi-filter union ------------------------------------------------------

expect_lanes 'a cargo manifest edit unions backend and audit' \
  'backend/Cargo.toml' 'rust::guards
infra::check
db-up
rust::check
rust::doc-lint
rust::test
rust::doctests
rust::sqlx-check
rust::machete
rust::deny'

expect_lanes 'unrelated planes union rather than override' \
  'backend/src/lib.rs
frontend/src/App.tsx' 'rust::guards
infra::check
db-up
rust::check
rust::doc-lint
rust::test
rust::doctests
rust::sqlx-check
rust::machete
js::check
js::test
js::build
js::font-integrity
js::a11y'

# --- escalation --------------------------------------------------------------

for path in justfile rust.just mise.toml lefthook.yml scripts/doctor.sh .github/path-filters.yml; do
  expect_lanes "editing ${path} escalates to the full gate" "$path" "$FULL"
done

# An escalating path anywhere in the set wins, not just as the first entry.
expect_lanes 'escalation wins even alongside a narrow change' \
  'README.md
scripts/doctor.sh' "$FULL"

# --- CI-only filters ---------------------------------------------------------

# The Dockerfile matches iac and docker, both of which are locally unrunnable.
# The change must still be reported, and must not silently select nothing.
out="$(printf 'Dockerfile' | "$scope" --files-from - 2>&1)"
case "$out" in
  *'CI-only gates this change triggers'*) printf 'ok   %s\n' 'a CI-only match is reported, not swallowed' ;;
  *)
    printf 'FAIL %s\n  output: %s\n' 'a CI-only match is reported, not swallowed' "$out"
    failures=$((failures + 1))
    ;;
esac

# --- fail-closed behaviour ---------------------------------------------------

# A base ref that does not resolve must abort. Skipping every lane because
# the comparison could not be made is the one outcome this tool must never
# produce.
expect_failure 'an unresolvable base ref aborts' 'cannot resolve base ref' \
  "$scope" --base 'refs/heads/definitely-not-a-real-ref'

printf '%s\n' 'unmapped:' "  - 'nowhere.txt'" > "$tmpdir/unmapped.yml"
expect_failure 'a filter with no lane mapping aborts' 'no local lane mapping' \
  "$scope" --filters "$tmpdir/unmapped.yml" --files-from /dev/null

# picomatch's `*` stops at a path separator and bash's does not, so a pattern
# this matcher cannot translate exactly has to be refused rather than
# approximated in either direction.
printf '%s\n' 'backend:' "  - 'docs/*.md'" > "$tmpdir/glob.yml"
expect_failure 'an untranslatable glob aborts' 'unsupported glob' \
  "$scope" --filters "$tmpdir/glob.yml" --files-from /dev/null

printf '%s\n' 'backend:' '  patterns: {a: b}' > "$tmpdir/shape.yml"
expect_failure 'an unrecognised YAML shape aborts' 'unparsable line' \
  "$scope" --filters "$tmpdir/shape.yml" --files-from /dev/null

expect_failure 'a missing filters file aborts' 'missing' \
  "$scope" --filters "$tmpdir/does-not-exist.yml" --files-from /dev/null

# --- changed-path collection -------------------------------------------------

# Everything above feeds paths in through --files-from, which exercises the
# mapping but not the git query that produces those paths in real use. These
# two cases need a repository, so they build a throwaway one and run a copy of
# the scoper from inside it: the script derives its own root from $0, so a
# copy under <fixture>/scripts/ treats the fixture as the repo.
fixture="$tmpdir/fixture"
mkdir -p "$fixture/scripts" "$fixture/.github" "$fixture/backend" "$fixture/docs"
cp "$scope" "$fixture/scripts/preflight-scope.sh"
cp .github/path-filters.yml "$fixture/.github/path-filters.yml"
git -C "$fixture" init -q -b main
git -C "$fixture" config user.email selftest@example.invalid
git -C "$fixture" config user.name 'Preflight Selftest'
echo 'fn main() {}' > "$fixture/backend/moved.rs"
git -C "$fixture" add -A
git -C "$fixture" commit -qm 'base'
git -C "$fixture" checkout -q -b topic
git -C "$fixture" mv backend/moved.rs docs/moved.mdx
git -C "$fixture" commit -qm 'move across filter scopes'

# git's rename detection reports only the destination of a move, which would
# select the docs lane and silently drop every Rust lane the vacated backend
# path required. Both sides of the move have to be named.
rename_got="$(cd "$fixture" && ./scripts/preflight-scope.sh --base main 2> /dev/null)"
rename_want="$BACKEND
docs::check"
if [ "$rename_got" = "$rename_want" ]; then
  printf 'ok   %s\n' 'a rename across filter scopes keeps the source lane'
else
  printf 'FAIL %s\n  want: %s\n  got:  %s\n' 'a rename across filter scopes keeps the source lane' \
    "$(echo "$rename_want" | tr '\n' ' ')" "$(echo "$rename_got" | tr '\n' ' ')"
  failures=$((failures + 1))
fi

# A git failure during path discovery must abort. Swallowed, it looks like a
# short changed-path list, which is exactly the confidently-narrowed gate this
# tool must never produce. The stub fails only the discovery subcommands, so
# base-ref resolution still succeeds and the run reaches the failure under test.
real_git="$(command -v git)"

# One stub dir per subcommand, each failing only its own, so the other
# discovery call still succeeds and every failure is attributed exactly.
for subcommand in diff ls-files; do
  stub_dir="$tmpdir/stub-$subcommand"
  mkdir -p "$stub_dir"
  cat > "$stub_dir/git" <<STUB
#!/usr/bin/env bash
# The real git is called by absolute path: this directory is first on PATH,
# so a bare 'git' here would recurse into the stub forever.
if [ "\$1" = '${subcommand}' ]; then
  echo 'stubbed git failure' >&2
  exit 1
fi
exec '${real_git}' "\$@"
STUB
  chmod +x "$stub_dir/git"

  stub_rc=0
  stub_out="$(cd "$fixture" && PATH="$stub_dir:$PATH" ./scripts/preflight-scope.sh --base main 2>&1)" || stub_rc=$?
  case "$stub_out" in
    *"git ${subcommand}"*) mentions_it=1 ;;
    *) mentions_it=0 ;;
  esac
  if [ "$stub_rc" -ne 0 ] && [ "$mentions_it" -eq 1 ]; then
    printf 'ok   %s\n' "a failing git ${subcommand} aborts instead of narrowing the gate"
  else
    printf 'FAIL %s\n  rc: %s\n  output: %s\n' \
      "a failing git ${subcommand} aborts instead of narrowing the gate" "$stub_rc" "$stub_out"
    failures=$((failures + 1))
  fi
done

# --- the shared-source invariant --------------------------------------------

# The whole design rests on CI reading the same file. An inlined `filters:`
# block in ci.yml would let the two gates drift apart with nothing failing.
if grep -q '^ *filters: \.github/path-filters\.yml$' .github/workflows/ci.yml; then
  printf 'ok   %s\n' 'ci.yml sources its filters from the shared file'
else
  printf 'FAIL %s\n' 'ci.yml no longer sources .github/path-filters.yml; the local scoper can now drift from CI'
  failures=$((failures + 1))
fi

# --- the lane-invocation invariant -------------------------------------------

# The selected lanes are recipe names handed to `just`. Passing them all on one
# command line breaks as soon as any lane takes parameters: `rust::test *args`
# consumes every following name as a nextest filter, so those lanes never run.
# The gate can still exit 0 that way, which is the same confidently-narrowed
# green this tool exists to prevent.
#
# Lane execution now lives in scripts/gate-run.sh, which loops, so the
# behavioural half of this invariant is asserted where it runs:
# gate-run-selftest.sh puts a variadic fixture lane ahead of another and proves
# the second one still runs. What stays here is the structural half this file
# is placed to see, that the recipe delegates rather than re-inlining the
# broken call. Comments are stripped first: prose in the recipe names the
# broken form to explain why it is banned, and that must not read as the form
# being present.
justfile_code="$(grep -v '^[[:space:]]*#' justfile)"
# shellcheck disable=SC2016  # the patterns are recipe text; $ must stay literal
if printf '%s\n' "$justfile_code" | grep -qF 'scripts/gate-run.sh preflight "${lanes[@]}"' &&
  ! printf '%s\n' "$justfile_code" | grep -qF 'just "${lanes[@]}"'; then
  printf 'ok   %s\n' 'preflight (the scoped default gate) delegates its lane list to the looping runner'
else
  printf 'FAIL %s\n' 'preflight passes its lane list to a single just call; a variadic lane swallows the lanes after it'
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  printf '\n%d check(s) failed\n' "$failures" >&2
  exit 1
fi
printf '\nall checks passed\n'
