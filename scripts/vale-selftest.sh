#!/usr/bin/env bash
#
# Self-test for the ReverieProse Vale style. Feeds known-bad and known-clean
# snippets to Vale over stdin and asserts that each rule fires on its target
# and stays silent on clean prose. Stdin snippets are passed in-memory; the
# path-scoped cases (the [adr/**] WhStarter exemption) need real file paths, so
# they use a throwaway mini-repo under a tempdir. Either way no deliberately bad
# prose lands in the repo tree, so the other tree-walking linters (markdownlint,
# prettier, typos, no-issue-refs) never see it.
#
# Run from anywhere; it resolves the repo root so Vale finds .vale.ini. Exits
# non-zero on any mismatch so it can gate in CI.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail=0
# U+2014 (em dash) built from its UTF-8 bytes, so this file stays ASCII.
emdash=$(printf '\342\200\224')

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
expect_fires EmDash "The reader loads the spine ${emdash} then renders the page."
expect_fires WhStarter "The store is durable. What makes this work is the log."

# Clean technical prose must trip nothing.
expect_silent "The parser extracts EPUB 3 metadata from the OPF package."
expect_silent "Run the database migration before starting the server."

# Edge cases that lock in deliberate design choices.
# WhStarter is paragraph-scoped, so a Wh- word in a heading is exempt.
expect_silent "# How the reader caches pages"
# Vale lints prose only: an em dash inside an inline code span is ignored.
expect_silent "The \`a ${emdash} b\` operator is code, not prose."
# Near miss: "deep" without "dive" must not trip BusinessJargon.
expect_silent "We dug deep into the schema before the migration."

# Scope exclusions: EmDash is prose-only. These exercise the scope selector
# (~text.frontmatter & ~table.cell & ~table.header) over multi-line stdin, so
# structure stays silent while body prose still fires (locked in at line 57).
# Deleting the scope block from EmDash.yml makes one of these fire.
# Em dash in a YAML frontmatter value (the MADR `consulted` placeholder).
expect_silent "$(printf -- '---\nconsulted: "%s"\n---\n\nClean body prose.\n' "$emdash")"
# Em dash inside a table cell (a column marker, not prose).
expect_silent "$(printf -- '| Head |\n| --- |\n| a %s b |\n' "$emdash")"

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

if [ "$fail" -ne 0 ]; then
  echo "vale self-test: FAILED" >&2
  exit 1
fi
echo "vale self-test: passed"
