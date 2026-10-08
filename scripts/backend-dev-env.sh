#!/usr/bin/env bash
# Load process, parsed dev-file and retained database-state inputs in that order.
# Sourced by dev recipes; explicit empty values remain application inputs.

# Executing this file would resolve the configuration into a shell that exits
# immediately, which looks like success and changes nothing.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  echo "backend-dev-env.sh is sourced by the rust dev recipes, not executed" >&2
  exit 1
fi

# An explicitly set REVERIE_DEV_ENV names a file that must exist; the default
# path is optional; absent database inputs require retained provisioning state.
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

_dev_env_databases() {
  local _key _dev_state_loaded
  for _key in POSTGRES_PASSWORD REVERIE_APP_PASSWORD REVERIE_MIGRATOR_PASSWORD REVERIE_INGESTION_PASSWORD REVERIE_READONLY_PASSWORD; do
    local -I "$_key"
  done
  local -a _dev_database_inputs
  _dev_database_inputs=("$@")
  if [[ "$#" == 0 ]]; then
    _dev_database_inputs=(DATABASE_URL DATABASE_URL_INGESTION DATABASE_URL_MIGRATION)
  fi
  if [[ "${REVERIE_AUTO_MIGRATE:-false}" == true ]]; then
    _dev_database_inputs+=(DATABASE_URL_MIGRATION)
  fi
  _dev_state_loaded=0
  for _key in "${_dev_database_inputs[@]}"; do
    [[ ! -v "$_key" ]] || continue
    if [[ "$_dev_state_loaded" == 0 ]]; then
      # shellcheck source=scripts/postgres-provision.sh
      source "$(dirname "${BASH_SOURCE[0]}")/postgres-provision.sh"
      pg_dev_load || return 1
      _dev_state_loaded=1
    fi
    case "$_key" in
      DATABASE_URL) dev_env_default "$_key" "postgres://reverie_app:$REVERIE_APP_PASSWORD@localhost:5432/reverie_dev" ;;
      DATABASE_URL_INGESTION) dev_env_default "$_key" "postgres://reverie_ingestion:$REVERIE_INGESTION_PASSWORD@localhost:5432/reverie_dev" ;;
      DATABASE_URL_MIGRATION)
        if [[ "$#" == 1 && "${1:-}" == DATABASE_URL_MIGRATION ]]; then
          dev_env_default "$_key" "postgres:///reverie_dev?host=$pg_dev_socket&user=reverie_migrator&password=$REVERIE_MIGRATOR_PASSWORD"
        else
          dev_env_default "$_key" "postgres://reverie_migrator:$REVERIE_MIGRATOR_PASSWORD@localhost:5432/reverie_dev"
        fi
        ;;
    esac
  done
}
_dev_env_databases "$@" || return 1
# Required whenever OPDS is enabled, which is the default. Feeds emit absolute
# URLs rooted here, so the dev default is this server's own origin: a reader
# pointed at the API then receives links back to the API, reachable whether or
# not the frontend is running. `.env.example` ships the same value.
dev_env_default REVERIE_PUBLIC_URL "http://localhost:3000"
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
