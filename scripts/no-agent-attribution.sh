#!/usr/bin/env bash
# Reject machine/session attribution while preserving ordinary human trailers.
set -euo pipefail

usage() {
  echo "usage: no-agent-attribution.sh message-file <path> | range <base> <head>" >&2
  exit 2
}

check_message() {
  local label="$1" message="$2" findings trailers
  trailers="$(printf '%s\n' "$message" | git interpret-trailers --parse)"
  findings="$(printf '%s\n' "$trailers" | grep -niE \
    '^(generated-(by|with)|session-(id|link|url)|agent|model|tool):|^co-authored-by:[[:space:]]*(claude|codex|chatgpt|github copilot|gemini|gpt-[0-9][^<]*|cursor|windsurf)[[:space:]]*<' || true)"
  if [ -n "$findings" ]; then
    printf 'Machine or session attribution found in %s:\n%s\n' "$label" "$findings" >&2
    return 1
  fi
}

[ "$#" -ge 1 ] || usage
case "$1" in
  message-file)
    [ "$#" -eq 2 ] || usage
    [ -f "$2" ] || { echo "commit message file does not exist: $2" >&2; exit 2; }
    check_message "$2" "$(<"$2")"
    ;;
  range)
    [ "$#" -eq 3 ] || usage
    base="$2"
    head="$3"
    [ -n "$base" ] && [ -n "$head" ] || usage
    git rev-parse --verify "${base}^{commit}" >/dev/null 2>&1 || { echo "invalid base commit: $base" >&2; exit 2; }
    git rev-parse --verify "${head}^{commit}" >/dev/null 2>&1 || { echo "invalid head commit: $head" >&2; exit 2; }
    git merge-base --is-ancestor "$base" "$head" || { echo "base is not an ancestor of head" >&2; exit 2; }
    fail=0
    while IFS= read -r commit; do
      [ -n "$commit" ] || continue
      message="$(git log -1 --format=%B "$commit")"
      check_message "$commit" "$message" || fail=1
    done < <(git rev-list --no-merges "${base}..${head}")
    exit "$fail"
    ;;
  *) usage ;;
esac
