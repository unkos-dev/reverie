#!/usr/bin/env bash
# mise.toml is the single source for lint tool pins: CI provisions these
# tools with mise, so a per-workflow version pin is a regression back to the
# dual-pin drift this guard used to reconcile. Reject any workflow line that
# pins a mise-managed tool by version.
set -euo pipefail

managed_tools=$(
  awk -F' *= *' '
    /^\[tools\]$/ { in_tools = 1; in_tool_block = 0; next }
    /^\[tools\."[^"]+"\]$/ {
      in_tools = 0
      in_tool_block = 1
      key = $0
      sub(/^\[tools\."/, "", key)
      sub(/"\]$/, "", key)
      count = split(key, parts, "/")
      print parts[count]
      next
    }
    /^\[/ { in_tools = 0; in_tool_block = 0 }
    in_tools && NF > 1 {
      key = $1
      gsub(/[[:space:]"]/, "", key)
      print key
    }
    in_tool_block && $1 ~ /^(bin|filter_bins)$/ {
      value = $2
      gsub(/[[:space:]"]/, "", value)
      print value
    }
  ' mise.toml | sort -u
)

fail=0
while IFS= read -r tool; do
  # Inline pins (`just@1.56.0`, `yamllint==1.38.0`) and Renovate annotations
  # (`depName=rhysd/actionlint` with the version on a following line). An
  # annotation naming a mise-managed tool always fronts a duplicated pin, so
  # it is rejected even though the version sits on another line.
  for pattern in "\b${tool}[@=]=?[0-9]" "depName=(\S+/)?${tool}(\s|$)"; do
    rc=0
    matches=$(grep -nE "$pattern" .github/workflows/*.yml) || rc=$?
    if [ "$rc" -gt 1 ]; then
      exit "$rc"
    fi
    if [ -n "$matches" ]; then
      printf '%s\n' "$matches" >&2
      echo "${tool} is version-pinned in a workflow; mise.toml is the single source of truth" >&2
      fail=1
    fi
  done
done <<<"$managed_tools"

if [ "$fail" -eq 0 ]; then
  echo "no workflow re-pins a mise-managed tool"
fi
exit "$fail"
