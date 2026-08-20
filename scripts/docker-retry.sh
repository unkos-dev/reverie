#!/usr/bin/env sh
# Retry a command a bounded number of times with a fixed backoff.
#
# For image-build steps that reach the network. mise verifies every tool
# download's GitHub artifact attestation against api.github.com, which
# intermittently answers 504; that is a transient failure a second attempt
# clears and a longer timeout does not, since a gateway error arrives promptly.
# mise exposes timeouts but no retry of its own, so this supplies one, matching
# the `curl --retry` the mise installer itself uses.
#
# Bounded deliberately: three attempts distinguishes a blip from an outage, and
# an outage should fail the build rather than hang it. The final attempt's exit
# status is the script's, so a genuine failure still stops the build loudly.
set -eu

attempts=3
delay=5
n=1

while :; do
  # Captured inside the else branch: after `fi`, `$?` is the status of the
  # `if` compound, which is 0 whether or not the guarded command failed.
  if "$@"; then
    exit 0
  else
    status=$?
  fi
  if [ "$n" -ge "$attempts" ]; then
    echo "retry: '$*' failed after ${attempts} attempts" >&2
    exit "$status"
  fi
  echo "retry: attempt ${n} of ${attempts} failed (exit ${status}); retrying in ${delay}s" >&2
  n=$((n + 1))
  sleep "$delay"
done
