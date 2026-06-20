#!/usr/bin/env bash
#
# Self-test for the ReverieProse Vale style. Feeds known-slop and known-clean
# snippets to Vale over stdin and asserts that each rule fires on its target
# and stays silent on clean prose. Snippets are passed in-memory (no fixture
# files on disk), so the other tree-walking linters (markdownlint, prettier,
# typos, no-issue-refs) never see deliberately bad prose.
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
  local rule="ReverieProse.$1" snippet="$2"
  if alerts_for "$snippet" | grep -qx "$rule"; then
    echo "ok   $rule fires"
  else
    echo "FAIL $rule did not fire on: $snippet" >&2
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

# Each slop snippet must trip exactly its target rule.
expect_fires ThroatClearing "It's worth noting that the parser reads the OPF package."
expect_fires BusinessJargon "Let's circle back to the import pipeline tomorrow."
expect_fires Adverbs "The cache is fundamentally a key-value store."
expect_fires VagueDeclaratives "The implications are significant for the schema."
expect_fires EmDash "The reader loads the spine ${emdash} then renders the page."
expect_fires WhStarter "The store is durable. What makes this work is the log."

# Clean technical prose must trip nothing.
expect_silent "The parser extracts EPUB 3 metadata from the OPF package."
expect_silent "Run the database migration before starting the server."

if [ "$fail" -ne 0 ]; then
  echo "vale self-test: FAILED" >&2
  exit 1
fi
echo "vale self-test: passed"
