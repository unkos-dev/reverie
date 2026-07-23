#!/usr/bin/env bash
# Drop Snyk Container results for base-OS packages that carry no fix, so
# the code-scanning dashboard stays an actionable queue rather than a
# standing inventory of the Debian base layer.
#
# A result is withheld only when BOTH conditions hold:
#
#   1. Its rule ID is in the distro namespace (SNYK-DEBIAN<n>-...). An
#      application-layer dependency with no upstream fix is still
#      actionable (vendor it, replace it, patch it), so it stays.
#   2. That rule's help text states no fixed version exists.
#
# Both are evaluated against the current scan, so nothing is carried in a
# static list that could go stale. The moment Debian ships a fix, the
# remediation text changes, the result stops matching, and the alert
# reappears on its own. That is why this needs no `.snyk` policy file, no
# expiry dates, and no drift gate: the predicate is the gate.
#
# The match is positive and the filter fails closed. Anything this script
# cannot classify is kept. If Snyk reworded the remediation section, every
# finding would reappear as an alert, which is loud and correct, rather
# than staying hidden.
#
# stdout is a Markdown summary of what was withheld, for the step summary.
#
# Usage: filter-sarif-unfixable-os.sh <input.sarif> <output.sarif>
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <input.sarif> <output.sarif>" >&2
  exit 2
fi

input="$1"
output="$2"

# Codename is not pinned: a Debian major bump moves the namespace from
# DEBIAN13 to DEBIAN14 and this must follow it without an edit.
distro_pattern='^SNYK-DEBIAN[0-9]+-'
nofix_marker='There is no fixed version for'

# Rules and results are separate arrays; the remediation text lives on the
# rule, so resolve the withheld rule IDs per run before filtering results.
jq --arg distro "$distro_pattern" --arg nofix "$nofix_marker" '
  def unfixable:
    [ .tool.driver.rules[]?
      | select((.id // "") | test($distro))
      | select(((.help.text // "") + " " + (.help.markdown // "")) | contains($nofix))
      | .id ];
  .runs |= map(
    unfixable as $withheld
    | .results = ((.results // []) | map(
        (.ruleId // "") as $r | select(($withheld | index($r)) | not)
      ))
  )' "$input" >"$output"

total="$(jq '[.runs[]?.results[]?] | length' "$input")"
kept="$(jq '[.runs[]?.results[]?] | length' "$output")"

echo "### Base-OS findings withheld from code scanning"
echo
echo "Scanned ${total}, ingested ${kept}, withheld $((total - kept)) with no fixed version in the base distribution."
echo
echo "Ingested findings are the actionable ones. Anything the filter cannot"
echo "classify is ingested, so a reworded remediation section surfaces as"
echo "alerts rather than as silence."
echo
if [ "$total" -ne "$kept" ]; then
  echo "| Package | Withheld |"
  echo "| ------- | -------- |"
  jq -r --arg distro "$distro_pattern" --arg nofix "$nofix_marker" '
    [ .runs[]?
      | [ .tool.driver.rules[]?
          | select((.id // "") | test($distro))
          | select(((.help.text // "") + " " + (.help.markdown // "")) | contains($nofix))
          | .id ] as $withheld
      | .results[]?
      | (.ruleId // "") as $r
      | select($withheld | index($r))
      | $r | split("-") | .[2] // "unknown"
    ]
    | group_by(.)
    | map({ package: .[0], count: length })
    | sort_by(-.count, .package)
    | .[]
    | "| `\(.package | ascii_downcase)` | \(.count) |"' "$input"
fi
