#!/usr/bin/env bash
# Guard: static backend allowlist guards that backend.yml's
# checks job runs before installing a toolchain, database, or any
# other dependency, so a violation fails in seconds. Single-sourced here per
# the justfile header's stated philosophy (CI command definitions live once);
# the workflow invokes this script instead of duplicating the grep logic inline.
#
# Two independent guards, run in the same order as that job:
#   1. runtime sqlx::query/raw_sql invocations outside the allowlist
#   2. raw axum .route( registrations outside the allowlist
#
# The datetime-crate boundary is NOT here: it asks what a path resolves
# to rather than how it is spelled, which a regex can only approximate.
# It lives in backend/clippy.toml as `disallowed-types`.
#
# Both guards reject zero query or route targets, including approved forms.
# Grep errors and empty or missing allowlists also fail.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail=0

# Runs one grep-allowlist guard: `pattern` over `path`, filtered against the
# non-comment lines of `allowlist`, reporting any surviving match as a
# violation. Args: label, pattern, target-pattern, path, allowlist, advice-lines...
run_grep_guard() {
    local label="$1" pattern="$2" target_pattern="$3" path="$4" allowlist="$5"
    shift 5
    local advice=("$@")

    local allow
    allow=$(grep -vE '^\s*(#|$)' "$allowlist")

    local rc=0
    grep -rE "$target_pattern" "$path" >/dev/null || rc=$?
    if [ "$rc" -gt 1 ]; then
        exit "$rc"
    fi
    if [ "$rc" -eq 1 ]; then
        echo "${label}: no target matches; refusing to report a clean tree" >&2
        exit 2
    fi

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
    'sqlx::(query[a-z_]*|raw_sql)!?\(' \
    backend/src/ \
    backend/guards/sqlx-runtime-allowlist.txt \
    "Use compile-time macros (query!, query_as!, query_scalar!)" \
    "or add a justified entry to backend/guards/sqlx-runtime-allowlist.txt."

# 2. Every shipped endpoint must register via OpenApiRouter + routes!(...)
# backed by #[utoipa::path] so the generated spec stays complete; raw
# `.route(` registrations bypass it.
run_grep_guard \
    "Raw .route( registrations" \
    '\.route\(' \
    '\.routes?\(' \
    backend/src/ \
    backend/guards/axum-route-allowlist.txt \
    "Register endpoints via OpenApiRouter + routes!(...) with #[utoipa::path]," \
    "or add a justified entry to backend/guards/axum-route-allowlist.txt."

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "OK: backend static guards hold (sqlx-runtime, axum-route)"
