#!/usr/bin/env bash
# Keep tool selection tied to the checkout when Cargo changes directories.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
export MISE_AUTO_INSTALL=0

die() { printf 'rust-exec: %s\n' "$1" >&2; exit 1; }

inspect_client() {
  local directory candidate client='' result rc=0
  command -v jq >/dev/null || { echo 'inspection unavailable: install jq' >&2; return 2; }
  directory="$(mise -C "$repo_root" where github:kunobi-ninja/kache 2>/dev/null)" ||
    { echo 'selected Kache is unavailable; run mise install in the checkout' >&2; return 2; }
  [[ $directory == /* && $directory != *$'\n'* ]] ||
    { echo 'inspection unavailable: invalid mise installation path' >&2; return 2; }
  for candidate in "$directory/bin/kache" "$directory/kache"; do
    if [[ -f $candidate && -x $candidate ]]; then
      candidate="$(readlink -f -- "$candidate")" || return 2
      [[ -z $client || $client == "$candidate" ]] ||
        { echo 'inspection unavailable: ambiguous Kache executable' >&2; return 2; }
      client=$candidate
    fi
  done
  [[ -n $client ]] || { echo 'selected Kache is unavailable; run mise install in the checkout' >&2; return 2; }
  command -v kache-lifecycle >/dev/null ||
    { echo 'inspection unavailable: install the machine-owned kache-lifecycle command' >&2; return 2; }
  result="$(kache-lifecycle inspect --client "$client" --json)" || rc=$?
  if ! jq -e -s --arg client "$client" --argjson rc "$rc" '
    def positive: type=="number" and .>0 and floor==.;
    def absolute: type=="string" and startswith("/") and (test("[\u0000-\u001f\u007f]")|not);
    length==1 and (.[0] |
      type=="object" and .schema_version==1 and .endpoint=="untested" and
      (.reason|type=="string") and (.remedy_argv|type=="array" and length>0 and all(.[];type=="string")) and
      ((.state=="compatible" and $rc==0) or
       ((.state=="unhealthy" or .state=="incompatible") and $rc==1) or (.state=="unknown" and $rc==2)) and
      (if .state=="compatible" then
        .client_path==$client and (.client_epoch|positive) and (.daemon_epoch|positive) and
        (.daemon_path|absolute) and (.main_pid|positive) and (.start_ticks|positive) and
        (.n_restarts|type=="number" and .>=0 and floor==.) and (.socket_path|absolute)
       else true end))
  ' <<<"$result" >/dev/null 2>&1; then
    echo 'inspection unavailable: malformed schema 1 result or inconsistent exit status' >&2
    return 2
  fi
  printf '%s\n' "$result"
  return "$rc"
}

mode="${1:-}"
shift || die 'expected a mode and command'
if [[ $mode == inspect ]]; then
  [[ $# == 0 ]] || die 'inspect takes no arguments'
  inspect_client
  exit $?
fi
if [[ $mode != __resolved ]]; then
  tools=()
  case "$mode" in
    compile|nextest)
      tools=(mold)
      if [[ ${CI:-} != true || ! -v RUSTC_WRAPPER ]]; then
        tools+=(github:kunobi-ninja/kache)
      fi
      [[ $mode != nextest ]] || tools+=(github:nextest-rs/nextest)
      ;;
    machete) tools=(github:bnjbvr/cargo-machete) ;;
    deny) tools=(github:EmbarkStudios/cargo-deny) ;;
    tools) ;;
    *) die 'unknown execution mode' ;;
  esac
  [[ ${1:-} == -- && $# -ge 2 ]] || die 'expected -- followed by command argv'
  shift
  for tool in "${tools[@]}"; do
    mise -C "$repo_root" where "$tool" >/dev/null 2>&1 ||
      die "required tool $tool is unavailable; run mise install in the checkout"
  done
  exec mise -C "$repo_root" exec "${tools[@]}" -- "$repo_root/scripts/rust-exec.sh" __resolved "$PWD" "$mode" "$@"
fi

[[ $# -ge 3 ]] || die 'incomplete resolved execution'
caller=$1 mode=$2
shift 2
cd "$caller"
case "$mode" in
  compile|nextest)
    if [[ ${CI:-} != true || ! -v RUSTC_WRAPPER ]]; then
      result=''
      if ! result="$(inspect_client)"; then
        if [[ -n $result ]]; then
          jq -r '"Kache " + .state + ": " + (.reason|@json) + "; remedy: " + (.remedy_argv|@sh)' <<<"$result" >&2
        fi
        die 'managed compilation refused; kache-lifecycle update honours the machine pin; an older pin needs an explicit maintainer change'
      fi
      socket="$(jq -r .socket_path <<<"$result")"
      [[ ! -v KACHE_SOCKET_PATH || $KACHE_SOCKET_PATH == "$socket" ]] ||
        die 'caller KACHE_SOCKET_PATH differs from the inspected managed endpoint'
      RUSTC_WRAPPER="$(jq -r .client_path <<<"$result")"
      export RUSTC_WRAPPER
      export KACHE_SOCKET_PATH="$socket"
    fi
    ;;
  machete|deny|tools) ;;
  *) die 'unknown resolved execution mode' ;;
esac
exec "$@"
