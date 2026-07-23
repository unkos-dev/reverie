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
#   2. That rule's help text states no fixed version exists FOR THE
#      RELEASE NAMED IN ITS OWN RULE ID.
#
# The second condition is version-tied on purpose. A bare "no fixed
# version" substring would also match an advisory that mentions an
# older release in passing while offering an upgrade for the release
# actually being scanned, and that is a fixable finding this must never
# withhold. Matching "no fixed version for `Debian:<n>`" against the
# `<n>` parsed out of the rule ID closes that gap.
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

# Release number is captured rather than pinned: a Debian major bump
# moves the namespace from DEBIAN13 to DEBIAN14 and both the namespace
# test and the remediation marker must follow it without an edit.
distro_pattern='^SNYK-DEBIAN(?<release>[0-9]+)-'
nofix_marker='There is no fixed version for'

# Rules and results are separate arrays; the remediation text lives on the
# rule, so resolve the withheld rule IDs per run before filtering results.
jq --arg distro "$distro_pattern" --arg nofix "$nofix_marker" '
  def unfixable:
    [ .tool.driver.rules[]?
      | select((.id // "") | test($distro))
      | ((.id // "") | capture($distro).release) as $release
      | select(((.help.text // "") + " " + (.help.markdown // ""))
               | contains($nofix + " `Debian:" + $release + "`"))
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
# Derived by differencing the two files rather than by re-running the
# predicate, so the summary reports what the filter actually did and
# cannot drift away from it.
if [ "$total" -ne "$kept" ]; then
  echo "| Package | Withheld |"
  echo "| ------- | -------- |"
  jq -rn --slurpfile before "$input" --slurpfile after "$output" '
    def tally: [ .[0].runs[]?.results[]? | .ruleId // "unknown" ]
      | group_by(.) | map({ key: .[0], value: length }) | from_entries;
    ($after | tally) as $kept
    | ($before | tally)
    | to_entries
    | map({ package: (.key | split("-") | .[2] // "unknown"),
            count: (.value - ($kept[.key] // 0)) })
    | map(select(.count > 0))
    | group_by(.package)
    | map({ package: .[0].package, count: (map(.count) | add) })
    | sort_by(-.count, .package)
    | .[]
    | "| `\(.package | ascii_downcase)` | \(.count) |"'
fi
