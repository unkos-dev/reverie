#!/usr/bin/env bash
# Authenticate the retained development cluster over TCP and its host socket.
set -ueo pipefail

# shellcheck source=scripts/postgres-provision.sh
source "$(dirname "${BASH_SOURCE[0]}")/postgres-provision.sh"
pg_dev_load
pg_dev_auth || { pg_fail 'development database authentication failed'; exit 1; }
