#!/usr/bin/env bash
# Resolve the backend's dev configuration, then export only what is missing.
# Sourced by the rust plane's dev recipes; not executable on its own.
#
# The server loads `.env` through dotenvy, which does NOT overwrite variables
# already present in the process environment. So a recipe that exports a
# default unconditionally does not "default" it at all: it silently overrides
# whatever the developer put in `.env`, and the repository ships a `.env.example`
# that sets DATABASE_URL and REVERIE_PUBLIC_URL. Resolution order here is
# therefore environment, then `.env`, then the dev default, and a value that
# `.env` supplies is left for dotenvy to apply rather than pre-empted.
#
# REVERIE_PORT is the exception that must be exported either way: the readiness
# probe in scripts/dev-server.sh watches one port and the server binds another,
# so they have to agree on a single resolved value. Exporting the value read
# from `.env` is a no-op for dotenvy and makes the two agree by construction.

# Executing this file would resolve the configuration into a shell that exits
# immediately, which looks like success and changes nothing.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  echo "backend-dev-env.sh is sourced by the rust dev recipes, not executed" >&2
  exit 1
fi

# dotenvy searches for `.env` from the working directory upward, so mirror that
# walk rather than assuming the repository root.
dev_env_file() {
  local dir
  dir="$(pwd -P)"
  while :; do
    if [ -f "${dir}/.env" ]; then
      printf '%s\n' "${dir}/.env"
      return 0
    fi
    [ "$dir" = "/" ] && return 1
    dir="$(dirname "$dir")"
  done
}

_dev_env_file="$(dev_env_file || true)"

# Whether `.env` assigns KEY. Deliberately does not parse the value: knowing
# that dotenvy will supply one is enough to stay out of the way. `export KEY=`
# is accepted because dotenvy accepts it.
dev_env_defines() {
  [ -n "$_dev_env_file" ] || return 1
  grep -qE "^[[:space:]]*(export[[:space:]]+)?$1=" "$_dev_env_file"
}

# Export a dev default only when neither the environment nor `.env` has a value.
dev_env_default() {
  local key="$1" value="$2"
  [ -n "${!key:-}" ] && return 0
  dev_env_defines "$key" && return 0
  export "${key}=${value}"
}

# Last assignment wins, matching dotenvy. Surrounding quotes are stripped so a
# quoted port resolves the same way the server resolves it.
dev_env_value() {
  local raw
  [ -n "$_dev_env_file" ] || return 1
  raw="$(grep -E "^[[:space:]]*(export[[:space:]]+)?$1=" "$_dev_env_file" | tail -n 1)" || return 1
  raw="${raw#*=}"
  raw="${raw%$'\r'}"
  case "$raw" in
    \"*\") raw="${raw#\"}" && raw="${raw%\"}" ;;
    \'*\') raw="${raw#\'}" && raw="${raw%\'}" ;;
  esac
  printf '%s\n' "$raw"
}

# RLS-enforced runtime identity. Overriding it is a matter of setting
# DATABASE_URL in the environment or in `.env`, so there is no recipe-specific
# knob for it.
dev_env_default DATABASE_URL "postgres://reverie_app:reverie_app@localhost:5432/reverie_dev"
# Required whenever OPDS is enabled, which is the default. Feeds emit absolute
# URLs rooted here, so the dev default is this server's own origin: a reader
# pointed at the API then receives links back to the API, reachable whether or
# not the frontend is running. `.env.example` ships the same value.
dev_env_default REVERIE_PUBLIC_URL "http://localhost:3000"

if [ -z "${REVERIE_PORT:-}" ]; then
  REVERIE_PORT="$(dev_env_value REVERIE_PORT || true)"
fi
REVERIE_PORT="${REVERIE_PORT:-3000}"
# A port the probe cannot parse would send it to the wrong port and time out
# against a healthy server, so refuse rather than guess.
if ! [[ "$REVERIE_PORT" =~ ^[0-9]+$ ]] || [ "$REVERIE_PORT" -lt 1 ] || [ "$REVERIE_PORT" -gt 65535 ]; then
  echo "REVERIE_PORT must be a TCP port number; got '${REVERIE_PORT}'" >&2
  return 1
fi
export REVERIE_PORT
