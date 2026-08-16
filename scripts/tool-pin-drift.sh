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
      sub(/^[^:]*:/, "", key)
      count = split(key, parts, "/")
      print parts[count]
      next
    }
    /^\[/ { in_tools = 0; in_tool_block = 0 }
    in_tools && NF > 1 {
      key = $1
      gsub(/[[:space:]"]/, "", key)
      # Strip the backend prefix as the table-block branch above does. An
      # inline entry can carry one too (`"npm:npm" = "11.18.0"`), and without
      # this the census emits the literal `npm:npm`, whose derived patterns
      # can never match the bare tool name a workflow would actually pin.
      sub(/^[^:]*:/, "", key)
      count = split(key, parts, "/")
      print parts[count]
    }
    in_tool_block && $1 ~ /^(bin|filter_bins)$/ {
      value = $2
      gsub(/[[:space:]"]/, "", value)
      print value
    }
  ' mise.toml | sort -u
)

# Positive control, run on every invocation. The census has three independent
# producers above (an inline [tools] entry, a [tools."backend:name"] block, and
# that block's bin/filter_bins value), and any one can stop matching while the
# others keep the census non-empty, which silently drops a whole class of tool
# from the check. One representative per producer is asserted, chosen so no
# single mise.toml entry answers for two of them. An empty census is the
# degenerate case of the same failure: without this the loop below reads one
# empty line, derives its patterns from an empty tool name, and the run ends by
# announcing a clean result it never established.
for expected in just nextest cargo-nextest; do
  if ! grep -qxF "$expected" <<<"$managed_tools"; then
    echo "tool-pin-drift: '${expected}' is absent from the mise.toml census, so one of the three producers has stopped matching; refusing to report that no workflow re-pins a managed tool. If that tool was deliberately dropped, pick a new representative for its producer" >&2
    exit 2
  fi
done

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
