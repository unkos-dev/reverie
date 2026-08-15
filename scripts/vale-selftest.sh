#!/usr/bin/env bash
#
# Self-test for the Vale spelling configuration. Snippets are fed over stdin so
# no deliberately bad prose lands in the tree, where the other linters
# (markdownlint, typos, no-issue-refs) would see it.
#
# Exits non-zero on any mismatch so it can gate in CI.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail=0

# --no-exit keeps Vale at exit 0 so a `warning`-level alert still yields JSON.
alerts_for() {
  printf '%s\n' "$1" |
    vale --no-exit --output=JSON --ext=.md |
    jq -r '.[][].Check' |
    sort -u
}

expect_fires() {
  local rule="Custom.$1" snippet="$2" got
  # Exact match, so an accidental second match is caught rather than masked.
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

expect_silent "The parser extracts EPUB 3 metadata from the OPF package."
expect_silent "Run the database migration before starting the server."
expect_fires Spelling "The reciever drops the frame."

# en_AU is the house spelling, so the American form fires and the AU form does not.
expect_fires Spelling "The cache behavior is configurable."
expect_silent "The cache behaviour is configurable."
expect_fires Spelling "We Minimize the payload."
expect_silent "We minimise the payload."

# The accept-list covers identifiers, libraries, and brand terms, including
# possessives.
expect_silent "The axum handler reads from sqlx."
expect_silent "Reverie uses the Parchment theme."
# AU forms the bundled dictionary lacks are accept-listed; the American form is
# not.
expect_silent "The grid uses virtualised scrolling."
expect_fires Spelling "The grid uses virtualized scrolling."

# Entries match whole tokens, so a misspelling containing a short accepted term
# still fires. This holds down to acronym length (the two-letter `AG` entry).
expect_fires Spelling "The configg value is wrong."
expect_silent "AG Grid renders the table."
expect_fires Spelling "The agregate view is wrong."

# Prose scope: code spans are out, formatting and table cells are not.
expect_silent "The \`behavior\` identifier is code, not prose."
expect_fires Spelling "This decision **behavior** owns the contract."
expect_fires Spelling "$(printf -- '| Head |\n| --- |\n| behavior |\n')"

# A finding must not exit non-zero and block the gate on its own.
if printf '%s\n' "The spine behavior renders." | vale --output=line --ext=.md >/dev/null 2>&1; then
  echo "ok   Spelling warning is advisory (exit 0)"
else
  echo "FAIL Spelling must warn without failing the gate" >&2
  fail=1
fi

# Frontmatter needs a real file path, which stdin cannot provide.
scope_root=$(mktemp -d)
trap 'rm -rf "$scope_root"' EXIT
cp .vale.ini "$scope_root/"
cp -r styles "$scope_root/styles"
mkdir -p "$scope_root/docs/src"

checks_for() {
  ( cd "$scope_root" && vale --no-exit --output=JSON "$1" ) | jq -r '.[][].Check' | sort -u
}

printf -- '---\nconsulted: "behavior"\n---\n\nClean body.\n' >"$scope_root/docs/src/fm-consulted.md"
printf -- '---\ndescription: The behavior organised the rail.\n---\n\nClean body.\n' >"$scope_root/docs/src/fm-desc.md"

got=$(checks_for docs/src/fm-consulted.md)
if [ "$got" = "Custom.Spelling" ]; then
  echo "ok   consulted frontmatter is in prose scope"
else
  echo "FAIL consulted should fire Spelling, got [${got:-<none>}]" >&2
  fail=1
fi

got=$(checks_for docs/src/fm-desc.md)
if [ "$got" = "Custom.Spelling" ]; then
  echo "ok   description frontmatter is in prose scope"
else
  echo "FAIL description should fire Spelling, got [${got:-<none>}]" >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "vale self-test: FAILED" >&2
  exit 1
fi
echo "vale self-test: passed"
