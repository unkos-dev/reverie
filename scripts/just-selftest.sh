#!/usr/bin/env bash
#
# Self-test for the just task graph (root justfile + js/rust/infra/docs modules).
# Proves three things without running the real toolchain:
#   1. just is new enough for the features the recipes use (cross-module deps).
#   2. Every expected aggregate and module recipe exists and its graph resolves.
#   3. A failing system-PATH tool aborts its recipe (fail-fast propagation).
#
# It deliberately does NOT assert canonical formatting: `just --fmt` is an unstable
# feature whose output differs across versions, so a fixed format would fail on some
# just in the supported range. justfile formatting is not gated elsewhere either.
#
# Mirrors scripts/vale-scope-test.sh: a stubbed binary on PATH exercises behaviour
# without invoking the real tool, and a pass/fail tally gates CI. No deliberately
# broken recipe lands in the tree; the negative case stubs cargo/shellcheck instead.
#
# Run from anywhere; it resolves the repo root. Exits non-zero on any mismatch.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail=0

# Minimum just version. Cross-module dependencies (check: js::check) are the
# binding floor at >= 1.42; mod and [working-directory] stabilised earlier. just
# has no built-in required-version, so this assertion is the guard. CI pins separately.
MIN_MAJOR=1
MIN_MINOR=42

# Version string is overridable for this test's own guard check; unset in normal
# runs, where it reads the real `just --version`.
ver="${JUST_SELFTEST_FAKE_VERSION:-$(just --version | awk '{print $2}')}"
IFS='.' read -r major minor _ <<<"$ver"
if [ "$major" -lt "$MIN_MAJOR" ] ||
  { [ "$major" -eq "$MIN_MAJOR" ] && [ "$minor" -lt "$MIN_MINOR" ]; }; then
  echo "FAIL just $ver is too old; upgrade to >= ${MIN_MAJOR}.${MIN_MINOR}" >&2
  exit 1
fi
echo "ok   just $ver meets minimum >= ${MIN_MAJOR}.${MIN_MINOR}"

# The guard must reject a sub-minimum version (exercises the comparison logic). The
# child re-runs this script with a faked low version and exits at the guard above.
if JUST_SELFTEST_FAKE_VERSION="${MIN_MAJOR}.$((MIN_MINOR - 1)).0" bash "$0" >/dev/null 2>&1; then
  echo "FAIL version guard accepted a sub-minimum just" >&2
  fail=1
else
  echo "ok   version guard rejects just < ${MIN_MAJOR}.${MIN_MINOR}"
fi

# Presence: every expected recipe must appear in `just --summary` (namespaced
# mod::recipe form). Exact token match so an aggregate `check` is not satisfied by
# a module `js::check`.
mapfile -t EXPECTED <<'EOF'
check
lint
fmt
test
build
js::oxlint
js::stylelint
js::markdownlint
js::detect
js::fmt
js::fmt-check
js::lint
js::a11y
js::test
js::test-coverage
js::build
js::check
rust::fmt
rust::fmt-check
rust::clippy
rust::doc-lint
rust::drift
rust::test
rust::doctests
rust::sqlx-check
rust::build
rust::lint
rust::check
infra::shellcheck
infra::hadolint
infra::typos
infra::yamllint
infra::actionlint
infra::prose
infra::vale-selftest
infra::vale-scope-test
infra::greptile-config
infra::no-issue-refs
infra::no-plan-refs
infra::lint
infra::check
docs::install
docs::build
docs::links
docs::check
EOF

if ! summary="$(just --summary 2>&1)"; then
  echo "FAIL just --summary failed (justfile parse error?):" >&2
  echo "$summary" >&2
  exit 1
fi
read -ra recipes <<<"$summary"
has_recipe() {
  local want="$1" got
  for got in "${recipes[@]}"; do
    [ "$got" = "$want" ] && return 0
  done
  return 1
}
missing=()
for r in "${EXPECTED[@]}"; do
  has_recipe "$r" || missing+=("$r")
done
if [ "${#missing[@]}" -eq 0 ]; then
  echo "ok   all ${#EXPECTED[@]} expected recipes present"
else
  echo "FAIL missing recipes: ${missing[*]}" >&2
  fail=1
fi

# Aggregate dependency graphs must resolve (dry-run; no tools run, no tree writes).
# Catches a mistyped cross-module dependency that --summary cannot (it proves a
# recipe is defined, not that an aggregate's dependency names resolve).
for agg in check lint fmt test build; do
  if just --dry-run "$agg" >/dev/null 2>&1; then
    echo "ok   aggregate $agg resolves"
  else
    echo "FAIL aggregate $agg has an unresolvable dependency" >&2
    fail=1
  fi
done

# Negative / fail-fast: stub a system-PATH tool to exit 1, then assert the recipe
# that calls it exits non-zero. This proves `set -e` in `set shell` propagates a
# failing tool's exit through the recipe (and, via the dependency form, the
# aggregate). JS-plane tools are NOT stubbed here: npm resolves
# node_modules/.bin, not PATH, so a PATH stub would be inert; JS fail-fast holds
# transitively (npm run returns the script's exit code) and at the aggregate level.
stub=$(mktemp -d)
trap 'rm -rf "$stub"' EXIT
for bin in cargo shellcheck; do
  printf '#!/usr/bin/env bash\nexit 1\n' >"$stub/$bin"
  chmod +x "$stub/$bin"
done

assert_fails() {
  local recipe="$1"
  if PATH="$stub:$PATH" just "$recipe" >/dev/null 2>&1; then
    echo "FAIL $recipe exited 0 with a stubbed failing tool (no fail-fast)" >&2
    fail=1
  else
    echo "ok   $recipe aborts when its tool fails"
  fi
}
assert_fails rust::clippy
assert_fails infra::shellcheck

if [ "$fail" -ne 0 ]; then
  echo "just self-test: FAILED" >&2
  exit 1
fi
echo "just self-test: passed"
