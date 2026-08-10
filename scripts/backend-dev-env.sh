#!/usr/bin/env bash
# Resolve the backend's dev configuration and export it into the environment.
# Sourced by the rust plane's dev recipes; not executable on its own.
#
# The server discovers no env file itself: `cargo run` reads only the process
# environment (backend/src/config/mod.rs Config::from_env). This script is the
# dev-only substitute, loading an out-of-tree env file so a checkout never
# doubles as a place to keep developer secrets. Resolution order is strictly
# environment, then the env file, then the dev default below: a value already
# in the process environment is never overwritten by the file, and a value in
# the file is never overwritten by the default. Getting this backwards would
# silently clobber whatever the developer already had set.
#
# Every key the file defines is exported, not only the ones with a
# recipe-known dev default (DATABASE_URL, REVERIE_PUBLIC_URL,
# DATABASE_URL_MIGRATION): the file pass below exports each parsed key that
# the environment does not already define, then dev_env_default only fills in
# the handful of keys the recipes need a fallback for.

# Executing this file would resolve the configuration into a shell that exits
# immediately, which looks like success and changes nothing.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  echo "backend-dev-env.sh is sourced by the rust dev recipes, not executed" >&2
  exit 1
fi

# An explicitly set REVERIE_DEV_ENV names a file that must exist; the default
# path is a convenience that silently falls back to dev defaults on a fresh
# clone, so its absence is not an error.
_dev_env_explicit="${REVERIE_DEV_ENV:-}"
REVERIE_DEV_ENV="${REVERIE_DEV_ENV:-$HOME/reverie/dev/env}"

if [ -n "$_dev_env_explicit" ] && [ ! -f "$REVERIE_DEV_ENV" ]; then
  echo "REVERIE_DEV_ENV=${REVERIE_DEV_ENV} does not exist" >&2
  return 1
fi

# Parse the env file into an associative array so a later assignment for the
# same key overwrites an earlier one (last assignment wins), then export every
# parsed key the environment does not already define. A key with no valid
# shell-identifier form is skipped rather than attempted: a malformed line
# must not make `export` fail and abort the sourcing recipe under `set -e`.
declare -A _dev_file_vals=()
if [ -f "$REVERIE_DEV_ENV" ]; then
  while IFS= read -r _line || [ -n "$_line" ]; do
    _line="${_line%$'\r'}"
    [[ "$_line" =~ ^[[:space:]]*$ ]] && continue
    [[ "$_line" =~ ^[[:space:]]*# ]] && continue
    [[ "$_line" == *=* ]] || continue
    _line="${_line#"${_line%%[![:space:]]*}"}"
    if [[ "$_line" == "export "* ]]; then
      _line="${_line#export }"
    fi
    _key="${_line%%=*}"
    [[ "$_key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
    _val="${_line#*=}"
    case "$_val" in
      \"*\") _val="${_val#\"}" && _val="${_val%\"}" ;;
      \'*\') _val="${_val#\'}" && _val="${_val%\'}" ;;
    esac
    _dev_file_vals["$_key"]="$_val"
  done < "$REVERIE_DEV_ENV"
fi
for _key in "${!_dev_file_vals[@]}"; do
  [[ -v "$_key" ]] || export "${_key}=${_dev_file_vals[$_key]}"
done

# Export KEY to default_value when neither the environment nor the file pass
# above has already set it. The file pass already exported every key the file
# defines, so this only ever fires for a key the file left unset.
dev_env_default() {
  local key="$1" default_value="$2"
  [[ -v "$key" ]] && return 0
  export "${key}=${default_value}"
}

# RLS-enforced runtime identity. Overriding it is a matter of setting
# DATABASE_URL in the environment or in the dev env file, so there is no
# recipe-specific knob for it.
dev_env_default DATABASE_URL "postgres://reverie_app:reverie_app@localhost:5432/reverie_dev"
# Required whenever OPDS is enabled, which is the default. Feeds emit absolute
# URLs rooted here, so the dev default is this server's own origin: a reader
# pointed at the API then receives links back to the API, reachable whether or
# not the frontend is running. `.env.example` ships the same value.
dev_env_default REVERIE_PUBLIC_URL "http://localhost:3000"
# `cargo run -- migrate` no longer self-loads anything, so the migration DSN
# needs the same three-way precedence as every other dev default.
dev_env_default DATABASE_URL_MIGRATION "postgres://reverie_migrator:reverie_migrator@localhost:5432/reverie_dev"

# The file pass above already exported REVERIE_PORT if the file defines it
# and the environment did not; this just supplies the last-resort default.
REVERIE_PORT="${REVERIE_PORT:-3000}"
# A port the probe cannot parse would send it to the wrong port and time out
# against a healthy server, so refuse rather than guess.
if ! [[ "$REVERIE_PORT" =~ ^[0-9]+$ ]] || [ "$REVERIE_PORT" -lt 1 ] || [ "$REVERIE_PORT" -gt 65535 ]; then
  echo "REVERIE_PORT must be a TCP port number; got '${REVERIE_PORT}'" >&2
  return 1
fi
export REVERIE_PORT
