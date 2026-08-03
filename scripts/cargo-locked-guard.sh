#!/usr/bin/env bash
# Guard: every dependency-resolving cargo invocation in the just recipes
# passes --locked, so no recipe can take a version the lockfile never
# recorded (adr/2026-08-03-package-ingress-default-deny.md, the frozen-
# installs control). Non-resolving subcommands are exempt by pattern rather
# than per-line allowlist: fmt reads the tree, machete and deny read the
# lockfile without resolving, and `sqlx migrate run` replays SQL files. A
# new resolving invocation fails here instead of quietly reopening the gap
# the ADR records as closed.
#
# Usage: cargo-locked-guard.sh [justfile...]   defaults to the repo's set
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

files=("$@")
if [ "${#files[@]}" -eq 0 ]; then
  files=(justfile rust.just js.just docs.just infra.just)
fi

# `sqlx prepare` resolves (its inner cargo build does); bare `cargo sqlx`
# without prepare does not, so the pattern names the resolving subcommands
# outright instead of matching every cargo word.
resolving='cargo[[:space:]]+(build|check|clippy|doc|llvm-cov|nextest|run|test|sqlx[[:space:]]+prepare)([[:space:]]|$)'

fail=0
# /dev/null forces the file:line: prefix even for a single input file. grep
# exit 1 means "no cargo invocations at all", which is clean, not an error.
matches="$(grep -nE "$resolving" "${files[@]}" /dev/null || true)"
while IFS= read -r match; do
  [ -n "$match" ] || continue
  text="${match#*:*:}"
  case "$text" in
    *[[:space:]]'#'* | '#'*) continue ;;
  esac
  case "$text" in
    *'--locked'*) continue ;;
  esac
  printf 'cargo-locked-guard: missing --locked: %s\n' "$match" >&2
  fail=1
done <<< "$matches"

if [ "$fail" -ne 0 ]; then
  echo 'cargo-locked-guard: add --locked, or extend the non-resolving exemption in this script if the invocation genuinely resolves nothing' >&2
  exit 1
fi
echo 'cargo-locked-guard: every resolving cargo invocation passes --locked'
