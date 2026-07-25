#!/usr/bin/env bash
# Self-test scripts/db-ready.sh against a stubbed PATH, so the assertions
# never touch a real psql, socket, or database.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
probe="${repo_root}/scripts/db-ready.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

# Stub PATH: real bash/coreutils passed through by absolute-path symlink,
# psql stubbed with a controllable exit code that records its argv so the
# DSN the probe hands libpq is assertable, not assumed.
stub_bin="${tmp}/bin"
mkdir -p "${stub_bin}"
for real in env bash dirname; do
  ln -s "$(command -v "${real}")" "${stub_bin}/${real}"
done

cat >"${stub_bin}/psql" <<'PSQL_STUB'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" >"${DB_READY_STUB_ARGV_FILE}"
exit "${DB_READY_STUB_PSQL_RC:-0}"
PSQL_STUB
chmod +x "${stub_bin}/psql"

argv_file="${tmp}/psql-argv"
export DB_READY_STUB_ARGV_FILE="${argv_file}"

fail=0

check() { # <name> <want-exit> <actual-exit>
  local name="$1" want="$2" got="$3"
  if [ "${got}" -ne "${want}" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${got}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

# --- reachable database reads as ready ---
rc=0
PATH="${stub_bin}" HOME="${tmp}/home" XDG_STATE_HOME="" "${probe}" || rc=$?
check "reachable database exits zero" 0 "${rc}"

# The probe must aim libpq at the socket, as the schema-owner role, with a
# bounded connect timeout; a drifted DSN would silently probe the wrong
# thing while db-up kept trusting it.
expected_dsn="postgres:///reverie_dev?host=${tmp}/home/.local/state/reverie/pgsock&user=reverie&password=reverie&connect_timeout=2"
if grep -qF "${expected_dsn}" "${argv_file}"; then
  echo "ok   probe DSN targets the socket with the schema-owner role"
else
  echo "FAIL probe DSN drifted; psql received:"
  cat "${argv_file}"
  fail=1
fi

# --- XDG_STATE_HOME overrides the socket directory root, mirroring the
# compose mount's interpolation ---
rc=0
PATH="${stub_bin}" HOME="${tmp}/home" XDG_STATE_HOME="${tmp}/xdg-state" "${probe}" || rc=$?
check "XDG_STATE_HOME run exits zero" 0 "${rc}"
if grep -qF "host=${tmp}/xdg-state/reverie/pgsock" "${argv_file}"; then
  echo "ok   XDG_STATE_HOME overrides the socket directory"
else
  echo "FAIL XDG_STATE_HOME not honored; psql received:"
  cat "${argv_file}"
  fail=1
fi

# --- an unreachable database reads as not ready (db-up falls through) ---
rc=0
PATH="${stub_bin}" HOME="${tmp}/home" DB_READY_STUB_PSQL_RC=2 "${probe}" || rc=$?
check "unreachable database exits nonzero" 2 "${rc}"

# --- a missing psql binary reads as not ready, never as ready ---
stub_bin_no_psql="${tmp}/bin-no-psql"
mkdir -p "${stub_bin_no_psql}"
for f in "${stub_bin}"/*; do
  name="$(basename "${f}")"
  [ "${name}" = "psql" ] && continue
  cp -P "${f}" "${stub_bin_no_psql}/${name}"
done
rc=0
PATH="${stub_bin_no_psql}" HOME="${tmp}/home" "${probe}" || rc=$?
check "missing psql exits nonzero" 1 "${rc}"

exit "${fail}"
