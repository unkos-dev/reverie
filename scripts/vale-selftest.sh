#!/usr/bin/env bash
#
# Self-test for the ReverieProse Vale style. Feeds known-bad and known-clean
# snippets to Vale over stdin and asserts that each rule fires on its target
# and stays silent on clean prose. Stdin snippets are passed in-memory; the
# path-scoped cases (the [adr/**] WhStarter exemption) need real file paths, so
# they use a throwaway mini-repo under a tempdir. Either way no deliberately bad
# prose lands in the repo tree, so the other tree-walking linters (markdownlint,
# oxfmt, typos, no-issue-refs) never see it.
#
# Run from anywhere; it resolves the repo root so Vale finds .vale.ini. Exits
# non-zero on any mismatch so it can gate in CI.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail=0

# alerts_for <text> -> sorted, unique list of fired ReverieProse check names.
# --no-exit keeps Vale at exit 0 so a `warning`-level alert still yields JSON.
alerts_for() {
  printf '%s\n' "$1" |
    vale --no-exit --output=JSON --ext=.md |
    jq -r '.[][].Check' |
    sort -u
}

expect_fires() {
  local rule="ReverieProse.$1" snippet="$2" got
  # Exact match: the snippet must trip its target rule and nothing else, so an
  # accidental second match (a regression) is caught rather than masked.
  got=$(alerts_for "$snippet")
  if [ "$got" = "$rule" ]; then
    echo "ok   $rule fires (only)"
  else
    echo "FAIL expected only [$rule], got [${got:-<none>}] on: $snippet" >&2
    fail=1
  fi
}

expect_silent() {
  local snippet="$1" got
  got=$(alerts_for "$snippet")
  if [ -z "$got" ]; then
    echo "ok   clean snippet is silent"
  else
    echo "FAIL clean snippet fired [$got] on: $snippet" >&2
    fail=1
  fi
}

# Each bad snippet must trip exactly its target rule.
expect_fires ThroatClearing "It's worth noting that the parser reads the OPF package."
expect_fires BusinessJargon "Let's circle back to the import pipeline tomorrow."
expect_fires Adverbs "The cache is fundamentally a key-value store."
expect_fires VagueDeclaratives "The implications are significant for the schema."
expect_fires WhStarter "The store is durable. What makes this work is the log."

# Clean technical prose must trip nothing.
expect_silent "The parser extracts EPUB 3 metadata from the OPF package."
expect_silent "Run the database migration before starting the server."

# The spelling check (Australian English, en_AU dictionary) is advisory: it fires
# at `warning`, so the exact-match assertion also proves it does not co-trip a
# mechanical rule on a plain misspelling.
expect_fires Spelling "The reciever drops the frame."
# Replace policy: en_AU is the house spelling, so the American form fires while
# the Australian form stays silent.
expect_fires Spelling "The cache behavior is configurable."
expect_silent "The cache behaviour is configurable."
# The vocabulary accept-list silences identifiers, library names, and brand terms
# (a base term also covers its possessive form).
expect_silent "The axum handler reads from sqlx."
expect_silent "Reverie uses the Parchment theme."
# Accept-list entries match whole tokens, not substrings: a misspelling that
# merely contains a short accepted term (config) still fires, so unanchored
# short entries do not silently swallow typos.
expect_fires Spelling "The configg value is wrong."
# Same anchoring holds at acronym length: the two-letter `AG` entry accepts the
# bare token while a misspelling that starts with it still fires.
expect_silent "AG Grid renders the table."
expect_fires Spelling "The agregate view is wrong."

# House style is Australian: an American spelling of a common word warns (it is
# not accept-listed), while the Australian form stays silent. Guards against the
# accept-list re-admitting American common words and bypassing the AU nudge.
expect_fires Spelling "We Minimize the payload."
expect_silent "We minimise the payload."
# AU forms the bundled en_AU dictionary lacks are accept-listed (virtualised,
# virtualiser); the American form is not, so it still warns.
expect_silent "The grid uses virtualised scrolling."
expect_fires Spelling "The grid uses virtualized scrolling."

# Edge cases that lock in deliberate design choices.
# WhStarter is paragraph-scoped, so a Wh- word in a heading is exempt.
expect_silent "# How the reader caches pages"
# Vale lints prose only: an American spelling inside an inline code span is
# ignored, so an identifier is never held to the en_AU house spelling.
expect_silent "The \`behavior\` identifier is code, not prose."
# Near miss: "deep" without "dive" must not trip BusinessJargon.
expect_silent "We dug deep into the schema before the migration."

# Formatting is no bypass: an emphasised token still reaches the prose scope.
expect_fires Spelling "This decision **behavior** owns the contract."
# A table-cell finding fires too: table cells are not exempt.
expect_fires Spelling "$(printf -- '| Head |\n| --- |\n| behavior |\n')"
# Spelling is advisory: it surfaces at `warning`, so a finding must not exit
# non-zero and block the commit or CI gate on its own.
if printf '%s\n' "The spine behavior renders." | vale --output=line --ext=.md >/dev/null 2>&1; then
  echo "ok   Spelling warning is advisory (exit 0)"
else
  echo "FAIL Spelling must warn without failing the gate" >&2
  fail=1
fi

# Path-scoped behaviour can't be exercised over stdin (no file path), so build a
# throwaway mini-repo from the real .vale.ini + styles, with one Wh-opener line
# under both adr/ (WhStarter exempt) and docs/src/ (WhStarter fires). Deleting
# the [adr/**] exemption from .vale.ini makes the first assertion fail.
scope_root=$(mktemp -d)
trap 'rm -rf "$scope_root"' EXIT
cp .vale.ini "$scope_root/"
cp -r styles "$scope_root/styles"
mkdir -p "$scope_root/adr" "$scope_root/docs/src"
wh_opener="The store is durable. What makes this work is the log."
printf '%s\n' "$wh_opener" >"$scope_root/adr/decision.md"
printf '%s\n' "$wh_opener" >"$scope_root/docs/src/page.md"

# checks_for <relpath> -> sorted, unique fired checks for a file in the mini-repo.
checks_for() {
  ( cd "$scope_root" && vale --no-exit --output=JSON "$1" ) | jq -r '.[][].Check' | sort -u
}

got=$(checks_for adr/decision.md)
if [ -z "$got" ]; then
  echo "ok   WhStarter exempt under adr/"
else
  echo "FAIL adr/ should be WhStarter-exempt, fired [$got]" >&2
  fail=1
fi

got=$(checks_for docs/src/page.md)
if [ "$got" = "ReverieProse.WhStarter" ]; then
  echo "ok   WhStarter fires outside the adr/ exemption (docs/src/)"
else
  echo "FAIL docs/src/ should fire only WhStarter, got [${got:-<none>}]" >&2
  fail=1
fi

# Frontmatter scope on real files uses the same parser as vale-lint.sh. Both the
# ADR consulted field and a prose description must reach the prose scope.
printf -- '---\nconsulted: "behavior"\n---\n\nClean body.\n' >"$scope_root/docs/src/fm-consulted.md"
printf -- '---\ndescription: The behavior organised the rail.\n---\n\nClean body.\n' >"$scope_root/docs/src/fm-desc.md"

got=$(checks_for docs/src/fm-consulted.md)
if [ "$got" = "ReverieProse.Spelling" ]; then
  echo "ok   consulted frontmatter is in prose scope (real file)"
else
  echo "FAIL consulted should fire Spelling, got [${got:-<none>}]" >&2
  fail=1
fi

got=$(checks_for docs/src/fm-desc.md)
if [ "$got" = "ReverieProse.Spelling" ]; then
  echo "ok   description frontmatter is in prose scope (real file)"
else
  echo "FAIL description should fire Spelling, got [${got:-<none>}]" >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "vale self-test: FAILED" >&2
  exit 1
fi
echo "vale self-test: passed"
