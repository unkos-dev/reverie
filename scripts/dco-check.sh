#!/usr/bin/env bash
# Fails when any commit in base..head lacks a Signed-off-by trailer (DCO).
# Merge commits are exempt, matching the local commit-msg hook.
# Usage: dco-check.sh <base-sha> <head-sha>
set -euo pipefail

base="$1"
head="$2"

missing=0
while IFS= read -r sha; do
  if ! git show -s --format=%B "$sha" | grep -Eq '^Signed-off-by: .+ <.+@.+>'; then
    git show -s --format='missing Signed-off-by: %h %s' "$sha"
    missing=1
  fi
done < <(git rev-list --no-merges "${base}..${head}")

if [ "$missing" -ne 0 ]; then
  echo ""
  echo "Every commit must be signed off: git commit -s"
  echo "See CONTRIBUTING.md (Developer Certificate of Origin)."
  exit 1
fi

echo "DCO check passed."
