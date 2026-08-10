#!/usr/bin/env bash
# Resolve the backend's dev configuration, then export only what is missing.
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
# same key overwrites an earlier one (last assignment wins). Nothing is
# exported here; export happens in dev_env_default so environment precedence
# is enforced in one place.
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
    _val="${_line#*=}"
    case "$_val" in
      \"*\") _val="${_val#\"}" && _val="${_val%\"}" ;;
      \'*\') _val="${_val#\'}" && _val="${_val%\'}" ;;
    esac
    _dev_file_vals["$_key"]="$_val"
  done < "$REVERIE_DEV_ENV"
fi

# Whether the file assigns KEY, for callers that need to distinguish
# "defaulted" from "file-supplied" without exporting.
dev_env_defines() {
  [[ -v "_dev_file_vals[$1]" ]]
}

# Export KEY only when the process environment does not already define it
# (even to an empty string): environment wins over the file, and the file
# wins over the caller's default.
dev_env_default() {
  local key="$1" default_value="$2"
  [[ -v "$key" ]] && return 0
  if dev_env_defines "$key"; then
    export "${key}=${_dev_file_vals[$key]}"
  else
    export "${key}=${default_value}"
  fi
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

if [ -z "${REVERIE_PORT:-}" ] && dev_env_defines REVERIE_PORT; then
  REVERIE_PORT="${_dev_file_vals[REVERIE_PORT]}"
fi
REVERIE_PORT="${REVERIE_PORT:-3000}"
# A port the probe cannot parse would send it to the wrong port and time out
# against a healthy server, so refuse rather than guess.
if ! [[ "$REVERIE_PORT" =~ ^[0-9]+$ ]] || [ "$REVERIE_PORT" -lt 1 ] || [ "$REVERIE_PORT" -gt 65535 ]; then
  echo "REVERIE_PORT must be a TCP port number; got '${REVERIE_PORT}'" >&2
  return 1
fi
export REVERIE_PORT
