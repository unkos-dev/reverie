#!/usr/bin/env bash
# mise.toml is the single source for lint tool pins: CI provisions these
# tools with mise, so a per-workflow version pin is a regression back to the
# dual-pin drift this guard used to reconcile. Reject any workflow line that
# pins a mise-managed tool by version.
set -euo pipefail

fail=0
while IFS= read -r tool; do
  rc=0
  matches=$(grep -nE "\b${tool}[@=]=?[0-9]" .github/workflows/*.yml) || rc=$?
  if [ "$rc" -gt 1 ]; then
    exit "$rc"
  fi
  if [ -n "$matches" ]; then
    printf '%s\n' "$matches" >&2
    echo "${tool} is version-pinned in a workflow; mise.toml is the single source of truth" >&2
    fail=1
  fi
done < <(awk -F' *= *' '/^\[tools\]/ { in_tools = 1; next } /^\[/ { in_tools = 0 } in_tools && NF > 1 { print $1 }' mise.toml)

if [ "$fail" -eq 0 ]; then
  echo "no workflow re-pins a mise-managed tool"
fi
exit "$fail"
