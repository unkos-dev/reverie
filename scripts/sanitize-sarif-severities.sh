#!/usr/bin/env bash
# The code-scanning SARIF processor rejects any rule whose
# security-severity property is not parseable as a number, and Snyk
# stamps non-numeric values on rules that carry no CVSS score: null on
# container advisories, "undefined" on license findings. Drop only the
# invalid property so every finding still uploads; severity then falls
# back to the SARIF level, which is all the information those rules had.
#
# Usage: sanitize-sarif-severities.sh <input.sarif> <output.sarif>
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <input.sarif> <output.sarif>" >&2
  exit 2
fi

jq 'walk(if type == "object" and has("security-severity")
      and ((.["security-severity"] | try tonumber catch null) == null)
    then del(.["security-severity"]) else . end)' "$1" >"$2"
