#!/usr/bin/env bash
# Validate the Greptile review-context config (greptile.json).
#
# customContext.files[].path entries pin in-repo files that Greptile loads as
# authoritative review context (ADRs, CLAUDE.md, codeguard, docs config). A pin
# to a moved, renamed, or superseded file fails silently at review time: Greptile
# loads nothing and the review context degrades with no signal. This gate turns
# that silent rot into a loud CI failure.
#
# Checks:
#   1. greptile.json is valid JSON.
#   2. every customContext.files[] entry carries a non-empty path and description.
#   3. every customContext.files[].path resolves to a file in the repo.
#
# jq ships on ubuntu-latest and the workspace image.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
config="greptile.json"

if [ ! -f "$config" ]; then
  echo "validate-greptile-config: $config not found" >&2
  exit 1
fi

# 1. valid JSON
if ! jq -e . "$config" >/dev/null 2>&1; then
  echo "validate-greptile-config: $config is not valid JSON" >&2
  exit 1
fi

# 2. required keys present (path + description, both non-empty) on every entry
missing_keys="$(jq -r '
  .customContext.files
  | to_entries[]
  | select((.value.path // "") == "" or (.value.description // "") == "")
  | "  entry \(.key): missing path or description"
' "$config")"
if [ -n "$missing_keys" ]; then
  echo "validate-greptile-config: customContext.files entries missing required keys:" >&2
  echo "$missing_keys" >&2
  exit 1
fi

# 3. every pinned path resolves to a tracked file
status=0
while IFS= read -r path; do
  if [ ! -f "$path" ]; then
    echo "validate-greptile-config: pinned path does not exist: $path" >&2
    status=1
  fi
done < <(jq -r '.customContext.files[].path' "$config")

if [ "$status" -ne 0 ]; then
  echo "validate-greptile-config: one or more customContext.files paths are missing (stale pin)." >&2
  exit 1
fi

count="$(jq '.customContext.files | length' "$config")"
echo "validate-greptile-config: OK ($config valid, all ${count} customContext.files paths resolve)."
