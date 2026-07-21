#!/usr/bin/env bash
# Guard: static backend allowlist and prose guards that ci.yml's
# backend-checks job runs before installing a toolchain, database, or any
# other dependency, so a violation fails in seconds. Single-sourced here per
# the justfile header's stated philosophy (CI command definitions live once);
# ci.yml invokes this script instead of duplicating the grep logic inline.
#
# Four independent guards, run in the same order as backend-checks:
#   1. runtime sqlx::query/raw_sql invocations outside the allowlist
#   2. raw axum .route( registrations outside the allowlist
#   3. chrono imports outside the allowlist
#   4. em dashes in the generated config schema
#
# Exit-code handling is load-bearing in guards 1-3: grep exit 1 means
# "clean", exit >1 is a real error that must not read as clean, and an
# empty/missing allowlist fails here (set -e) instead of failing open
# downstream.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail=0

# Runs one grep-allowlist guard: `pattern` over `path`, filtered against the
# non-comment lines of `allowlist`, reporting any surviving match as a
# violation. Args: label, pattern, path, allowlist, advice-lines...
run_grep_guard() {
    local label="$1" pattern="$2" path="$3" allowlist="$4"
    shift 4
    local advice=("$@")

    local allow
    allow=$(grep -vE '^\s*(#|$)' "$allowlist")

    local rc=0
    local matches=''
    matches=$(grep -rnE "$pattern" "$path") || rc=$?
    if [ "$rc" -gt 1 ]; then
        exit "$rc"
    fi

    local violations=''
    if [ -n "$matches" ]; then
        rc=0
        violations=$(grep -v -F -f <(printf '%s\n' "$allow") <<<"$matches") || rc=$?
        if [ "$rc" -gt 1 ]; then
            exit "$rc"
        fi
    fi

    if [ -n "$violations" ]; then
        echo "${label} detected outside the allowlist." >&2
        for line in "${advice[@]}"; do
            echo "$line" >&2
        done
        echo "" >&2
        echo "$violations" >&2
        fail=1
    fi
}

# 1. Compile-time SQL macros are mandatory for the data path; the allowlist
# documents justified runtime carve-outs (DDL, dynamic SQL, set_config GUC,
# enum-drift probes).
run_grep_guard \
    "Disallowed runtime sqlx::query invocations" \
    'sqlx::(query[a-z_]*|raw_sql)\(' \
    backend/src/ \
    .github/sqlx-runtime-allowlist.txt \
    "Use compile-time macros (query!, query_as!, query_scalar!)" \
    "or add a justified entry to .github/sqlx-runtime-allowlist.txt."

# 2. Every shipped endpoint must register via OpenApiRouter + routes!(...)
# backed by #[utoipa::path] so the generated spec stays complete; raw
# `.route(` registrations bypass it.
run_grep_guard \
    "Raw .route( registrations" \
    '\.route\(' \
    backend/src/ \
    .github/axum-route-allowlist.txt \
    "Register endpoints via OpenApiRouter + routes!(...) with #[utoipa::path]," \
    "or add a justified entry to .github/axum-route-allowlist.txt."

# 3. The backend standardises on the `time` crate (backend/AGENTS.md);
# chrono exists in Cargo.toml solely for openidconnect's test-support
# signature.
run_grep_guard \
    "chrono imports" \
    'use chrono|extern crate chrono' \
    backend/src/ \
    .github/chrono-allowlist.txt \
    "Reverie uses the time crate (backend/AGENTS.md); chrono is" \
    "reserved for the documented openidconnect test-support carve-out." \
    "Justify any new entry in .github/chrono-allowlist.txt."

# 4. The schema descriptions are config-struct doc-comments (schemars folds
# them in); the Vale gate covers Markdown but not this generated JSON. Match
# raw U+2014 UTF-8 bytes (LC_ALL=C + fixed-string) so the check never
# depends on grep's PCRE or locale support: a PCRE-mode grep rejecting
# `\x{2014}` would exit non-zero and pass the gate open.
schema=backend/config.schema.json
rc=0
matches=$(LC_ALL=C grep -nF $'\xe2\x80\x94' "$schema") || rc=$?
if [ "$rc" -gt 1 ]; then
    exit "$rc"
fi
if [ -n "$matches" ]; then
    printf '%s\n' "$matches" >&2
    echo "${schema} contains an em dash (lines above); recast the source doc-comment in backend/src/config and regenerate." >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "OK: backend static guards hold (sqlx-runtime, axum-route, chrono, config-schema em dash)"
