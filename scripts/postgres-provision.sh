#!/usr/bin/env bash

pg_fail() {
  printf 'postgres-provision: %s\n' "$1" >&2
  return 1
}

pg_root() {
  cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd
}

pg_image() {
  local root image
  root="$(pg_root)" || return 1
  image="$(docker compose -f "$root/docker/compose.dev.yml" config --no-interpolate --no-env-resolution --format json 2>/dev/null |
    jq -er '.services.postgres.image' 2>/dev/null)" || {
    pg_fail 'cannot resolve PostgreSQL image'
    return 1
  }
  [[ "$image" =~ ^postgres:[^\$[:space:]]+@sha256:[a-f0-9]{64}$ ]] || {
    pg_fail 'PostgreSQL image must be literal and digest-pinned'
    return 1
  }
  printf '%s\n' "$image"
}

pg_generate() {
  local key value
  for key in POSTGRES_PASSWORD REVERIE_APP_PASSWORD REVERIE_MIGRATOR_PASSWORD REVERIE_INGESTION_PASSWORD REVERIE_READONLY_PASSWORD; do
    value="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')" || return 1
    [[ "$value" =~ ^[a-f0-9]{48}$ ]] || {
      pg_fail 'credential generation failed'
      return 1
    }
    printf '%s=%s\n' "$key" "$value"
  done
}

pg_read_credentials() {
  local file="$1" line key value
  local -A values=()
  [[ -f "$file" && ! -L "$file" ]] || { pg_fail 'credential state is missing'; return 1; }
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ "$line" == *=* ]] || { pg_fail 'credential state is malformed'; return 1; }
    key="${line%%=*}"
    value="${line#*=}"
    case "$key" in
      POSTGRES_PASSWORD | REVERIE_APP_PASSWORD | REVERIE_MIGRATOR_PASSWORD | REVERIE_INGESTION_PASSWORD | REVERIE_READONLY_PASSWORD) ;;
      *) pg_fail 'credential state is malformed'; return 1 ;;
    esac
    [[ ! -v "values[$key]" && "$value" =~ ^[a-f0-9]{48}$ ]] || {
      pg_fail 'credential state is malformed'
      return 1
    }
    values["$key"]="$value"
  done < "$file"
  [[ "${#values[@]}" == 5 ]] || { pg_fail 'credential state is incomplete'; return 1; }
  for key in "${!values[@]}"; do
    export "$key=${values[$key]}"
  done
}

pg_owned_cleanup() {
  local failed=0
  if [[ "$pg_created" == 1 ]]; then
    if ! docker inspect --format '{{.Id}}' "$pg_container" >/dev/null 2>&1; then
      if ! docker info --format '{{.ServerVersion}}' >/dev/null 2>&1; then
        pg_fail "cleanup cannot inspect residual container $pg_container"
        failed=1
      fi
    else
      if [[ "$pg_started" == 1 ]]; then
        if ! docker exec "$pg_container" sh -c 'rm -f /var/run/postgresql/.s.PGSQL.* /var/run/postgresql/*.pid' >/dev/null 2>&1; then
          pg_fail "socket cleanup failed for $pg_container"
          failed=1
        fi
      fi
      if ! docker rm -f -v "$pg_container" >/dev/null 2>&1; then
        pg_fail "container cleanup failed for $pg_container"
        failed=1
      fi
    fi
  fi
  if [[ -n "$pg_dir" ]] && ! rm -rf -- "$pg_dir"; then
    pg_fail "directory cleanup failed for $pg_dir"
    failed=1
  fi
  return "$failed"
}

pg_owned_exit() {
  local status=$?
  trap - EXIT INT TERM
  if ! pg_owned_cleanup; then
    case "$status" in
      0 | 2 | 3) status=1 ;;
    esac
  fi
  exit "$status"
}

pg_owned_signal() {
  local signal="$1" attempt
  case "$signal" in
    INT) pg_cancel_status=130 ;;
    TERM) pg_cancel_status=143 ;;
  esac
  if [[ -n "$pg_child" ]]; then
    if ! kill -s "$signal" -- "-$pg_child" 2>/dev/null; then
      pg_fail "child signal forwarding failed for $pg_container"
    fi
    for ((attempt=0; attempt<100; attempt++)); do
      kill -0 -- "-$pg_child" 2>/dev/null || break
      sleep 0.1
    done
    if kill -0 -- "-$pg_child" 2>/dev/null; then
      kill -KILL -- "-$pg_child" 2>/dev/null || pg_fail "child kill failed for $pg_container"
    fi
  else
    case "$signal" in
      INT) exit 130 ;;
      TERM) exit 143 ;;
    esac
  fi
}

pg_disposable() {
  set -euo pipefail
  local mode="$1" root image status attempt ready=0
  shift
  [[ "${1:-}" == -- && "$#" -gt 1 ]] || { pg_fail 'usage: test|schema -- COMMAND ARG...'; exit 1; }
  shift
  pg_dir='' pg_container='' pg_created=0 pg_started=0 pg_child='' pg_cancel_status=0
  trap pg_owned_exit EXIT
  trap 'pg_owned_signal INT' INT
  trap 'pg_owned_signal TERM' TERM
  root="$(pg_root)"
  image="$(pg_image)"
  umask 077
  pg_dir="$(mktemp -d "${TMPDIR:-/tmp}/rvpg.XXXXXXXX")"
  [[ "${#pg_dir}" -lt 80 ]] || { pg_fail 'TMPDIR is too long for a PostgreSQL socket; use a shorter temporary directory'; exit 1; }
  pg_container="reverie-test-${pg_dir##*.}"
  mkdir "$pg_dir/socket"
  pg_generate > "$pg_dir/credentials.env"
  pg_read_credentials "$pg_dir/credentials.env"
  # THREAT: Raw PostgreSQL startup diagnostics can contain substituted credentials.
  pg_created=1
  if ! docker create --name "$pg_container" --env-file "$pg_dir/credentials.env" \
    -e POSTGRES_USER=reverie -e POSTGRES_DB=reverie_test \
    -e POSTGRES_HOST_AUTH_METHOD=scram-sha-256 \
    -e 'POSTGRES_INITDB_ARGS=--auth-local=scram-sha-256 --auth-host=scram-sha-256' \
    -p 127.0.0.1::5432 \
    --mount "type=bind,src=$pg_dir/socket,dst=/var/run/postgresql" \
    --mount "type=bind,src=$root/docker/init-roles.sql,dst=/docker-entrypoint-initdb.d/01-init-roles.sql,readonly" \
    "$image" >/dev/null 2>&1; then
    pg_fail "container creation failed for $pg_container"
    exit 1
  fi
  if ! docker start "$pg_container" >/dev/null 2>&1; then
    pg_fail "container startup failed for $pg_container"
    exit 1
  fi
  pg_started=1
  export PGPASSWORD="$POSTGRES_PASSWORD"
  for ((attempt=0; attempt<60; attempt++)); do
    if docker exec -e PGPASSWORD "$pg_container" psql -X -h localhost -U reverie -d reverie_test -Atc 'SELECT 1' >/dev/null 2>&1; then
      ready=1
      break
    fi
    sleep 1
  done
  [[ "$ready" == 1 ]] || { pg_fail "authenticated readiness timed out for $pg_container"; exit 1; }
  export REVERIE_PG_CONTAINER="$pg_container"
  REVERIE_PG_PORT="$(docker inspect --format '{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$pg_container" 2>/dev/null)"
  [[ "$REVERIE_PG_PORT" =~ ^[0-9]+$ && "$REVERIE_PG_PORT" -gt 0 && "$REVERIE_PG_PORT" -le 65535 ]] || {
    pg_fail "invalid owned port for $pg_container"
    exit 1
  }
  export REVERIE_PG_PORT
  if [[ -S "$pg_dir/socket/.s.PGSQL.5432" ]]; then
    export REVERIE_PG_HOST="$pg_dir/socket"
    export DATABASE_URL="postgres:///reverie_test?host=$REVERIE_PG_HOST&user=reverie&password=$POSTGRES_PASSWORD"
  else
    export REVERIE_PG_HOST=127.0.0.1
    export DATABASE_URL="postgres://reverie:$POSTGRES_PASSWORD@127.0.0.1:$REVERIE_PG_PORT/reverie_test"
  fi
  unset DATABASE_URL_MIGRATION DATABASE_URL_INGESTION DATABASE_URL_FILE DATABASE_URL_MIGRATION_FILE DATABASE_URL_INGESTION_FILE
  if [[ "$mode" == schema ]]; then
    if ! PGPASSWORD="$REVERIE_MIGRATOR_PASSWORD" \
      DATABASE_URL="postgres:///reverie_test?host=$REVERIE_PG_HOST&port=$([[ "$REVERIE_PG_HOST" == /* ]] && printf 5432 || printf '%s' "$REVERIE_PG_PORT")&user=reverie_migrator" \
      sqlx migrate run --source "$root/backend/migrations" >/dev/null 2>&1; then
      pg_fail "schema preparation failed for $pg_container"
      exit 1
    fi
  fi
  printf 'postgres-provision: owned container=%s host=%s port=%s\n' "$pg_container" "$REVERIE_PG_HOST" "$REVERIE_PG_PORT" >&2
  unset PGPASSWORD
  env --default-signal=INT,TERM setsid --wait -- "$@" <&0 &
  pg_child=$!
  status=0
  while :; do
    wait "$pg_child" && status=0 || status=$?
    if ! kill -0 "$pg_child" 2>/dev/null; then
      break
    fi
  done
  [[ "$status" != 0 ]] || status="$pg_cancel_status"
  if kill -0 -- "-$pg_child" 2>/dev/null; then
    if ! kill -TERM -- "-$pg_child" 2>/dev/null; then
      pg_fail "descendant termination failed for $pg_container"
      [[ "$status" != 0 ]] || status=1
    fi
    for ((attempt=0; attempt<100; attempt++)); do
      kill -0 -- "-$pg_child" 2>/dev/null || break
      sleep 0.1
    done
    if kill -0 -- "-$pg_child" 2>/dev/null; then
      kill -KILL -- "-$pg_child" 2>/dev/null || pg_fail "descendant kill failed for $pg_container"
      pg_fail "descendant termination timed out for $pg_container"
      [[ "$status" != 0 ]] || status=1
    fi
  fi
  exit "$status"
}

pg_dev_context() {
  local root context
  local -a fields
  root="$(pg_root)" || return 1
  context="$(POSTGRES_PASSWORD=discovery REVERIE_APP_PASSWORD=discovery REVERIE_MIGRATOR_PASSWORD=discovery \
    REVERIE_INGESTION_PASSWORD=discovery REVERIE_READONLY_PASSWORD=discovery \
    docker compose -f "$root/docker/compose.dev.yml" config --no-env-resolution --format json 2>/dev/null |
    jq -er '[.name, .volumes.pgdata.name, .services.postgres.container_name,
      (.services.postgres.volumes[] | select(.target == "/var/run/postgresql") | .source)] |
      if all(.[]; type == "string" and length > 0) then .[] else error("context") end' 2>/dev/null)" || {
    pg_fail 'cannot resolve development Compose resources'
    return 1
  }
  mapfile -t fields <<< "$context"
  [[ "${#fields[@]}" == 4 && "${fields[0]}" =~ ^[a-z0-9][a-z0-9_-]*$ && "${fields[1]}" =~ ^[A-Za-z0-9][A-Za-z0-9_.-]*$ ]] || {
    pg_fail 'invalid development Compose resources'
    return 1
  }
  pg_project="${fields[0]}"
  pg_volume="${fields[1]}"
  pg_dev_container="${fields[2]}"
  pg_dev_socket="${fields[3]}"
  pg_state_dir="${XDG_STATE_HOME:-$HOME/.local/state}/reverie/postgres/$pg_project"
}

pg_dev_credentials() {
  local override="${POSTGRES_PASSWORD-}" override_set="${POSTGRES_PASSWORD+x}"
  [[ -d "$pg_state_dir" && ! -L "$pg_state_dir" && "$(stat -c '%a:%u' "$pg_state_dir")" == "700:$UID" ]] || {
    pg_fail 'development credential directory is missing or not private; use db-up for a fresh volume or coordinate a confirmed reset'
    return 1
  }
  [[ -f "$pg_state_dir/credentials.env" && ! -L "$pg_state_dir/credentials.env" && \
    "$(stat -c '%a:%u' "$pg_state_dir/credentials.env")" == "600:$UID" ]] || {
    pg_fail 'development credential state is missing or not private; restore its original state or coordinate a confirmed reset'
    return 1
  }
  pg_read_credentials "$pg_state_dir/credentials.env" || return 1
  if [[ -n "$override_set" && "$override" != "$POSTGRES_PASSWORD" ]]; then
    pg_fail 'POSTGRES_PASSWORD conflicts with retained development state'
    return 1
  fi
}

pg_dev_load() {
  pg_dev_context || return 1
  pg_dev_credentials
}

pg_dev_auth() {
  PGPASSWORD="$POSTGRES_PASSWORD" docker exec -e PGPASSWORD "$pg_dev_container" \
    psql -X -h localhost -U reverie -d reverie_dev -Atc 'SELECT 1' >/dev/null 2>&1 || return 1
  command -v psql >/dev/null 2>&1 || return 0
  local host=127.0.0.1
  [[ ! -S "$pg_dev_socket/.s.PGSQL.5432" ]] || host="$pg_dev_socket"
  PGPASSWORD="$POSTGRES_PASSWORD" psql -X \
    "postgres:///reverie_dev?host=$host&port=5432&user=reverie&connect_timeout=2" -Atc 'SELECT 1' >/dev/null 2>&1
}

pg_dev_compose() {
  local root
  root="$(pg_root)" || return 1
  docker compose -f "$root/docker/compose.dev.yml" "$@" >/dev/null 2>&1 || {
    pg_fail "development Compose operation failed for $pg_project"
    return 1
  }
}

pg_dev_publish_state() {
  local pending
  pending="$(mktemp "$pg_state_dir/.credentials.XXXXXXXX")" || return 1
  if ! pg_generate > "$pending" || ! pg_read_credentials "$pending"; then
    rm -f -- "$pending" || pg_fail 'incomplete credential state cleanup failed'
    pg_fail 'development credential generation failed'
    return 1
  fi
  if ! mv -- "$pending" "$pg_state_dir/credentials.env"; then
    rm -f -- "$pending" || pg_fail 'unpublished credential state cleanup failed'
    pg_fail 'development credential publication failed'
    return 1
  fi
}

pg_dev_ensure_state() {
  if [[ -e "$pg_state_dir/credentials.env" || -L "$pg_state_dir/credentials.env" ]]; then
    pg_dev_credentials
    return
  fi
  if docker volume inspect "$pg_volume" >/dev/null 2>&1; then
    pg_fail "volume $pg_volume has no credential state; restore the original state or coordinate a confirmed reset"
    return 1
  fi
  docker info --format '{{.ServerVersion}}' >/dev/null 2>&1 || { pg_fail 'Docker daemon unavailable'; return 1; }
  [[ ! -v POSTGRES_PASSWORD ]] || { pg_fail 'fresh development credentials cannot adopt a POSTGRES_PASSWORD override'; return 1; }
  pg_dev_publish_state
}

pg_dev_lifecycle() {
  set -euo pipefail
  local mode="$1" confirmation="${2:-}" mounted label override_set="${POSTGRES_PASSWORD+x}"
  pg_dev_context
  umask 077
  [[ ! -L "$pg_state_dir" ]] || { pg_fail 'development state directory must not be a symlink'; exit 1; }
  mkdir -p -- "$pg_state_dir"
  [[ "$(stat -c '%a:%u' "$pg_state_dir")" == "700:$UID" ]] || { pg_fail 'development state directory must be private'; exit 1; }
  [[ ! -L "$pg_state_dir/provision.lock" ]] || { pg_fail 'development lock must not be a symlink'; exit 1; }
  exec 9> "$pg_state_dir/provision.lock"
  flock 9
  if [[ "$mode" == dev-reset ]]; then
    [[ "$confirmation" == "$pg_volume" ]] || { pg_fail "reset requires confirmation of volume $pg_volume; all its data will be lost"; exit 1; }
    if [[ -e "$pg_state_dir/credentials.env" ]]; then
      pg_dev_credentials
    elif [[ -n "$override_set" ]]; then
      pg_fail 'reset without retained state cannot adopt a POSTGRES_PASSWORD override'
      exit 1
    fi
    label="$(docker volume inspect "$pg_volume" --format '{{index .Labels "com.docker.compose.project"}}' 2>/dev/null)" || {
      pg_fail "cannot identify reset volume $pg_volume"
      exit 1
    }
    [[ "$label" == "$pg_project" ]] || { pg_fail 'reset volume belongs to a different Compose project'; exit 1; }
    if docker inspect "$pg_dev_container" >/dev/null 2>&1; then
      mounted="$(docker inspect "$pg_dev_container" --format '{{range .Mounts}}{{if eq .Destination "/var/lib/postgresql"}}{{.Name}}{{end}}{{end}}' 2>/dev/null)"
      [[ "$mounted" == "$pg_volume" ]] || { pg_fail 'development container mounts a different reset volume'; exit 1; }
    fi
    if [[ ! -e "$pg_state_dir/credentials.env" ]]; then
      pg_dev_publish_state
    fi
    pg_dev_compose down 9>&-
    docker volume rm "$pg_volume" >/dev/null 2>&1 9>&- || { pg_fail "confirmed volume removal failed for $pg_volume"; exit 1; }
  fi
  if [[ "$mode" == dev-down ]]; then
    pg_dev_credentials
    pg_dev_compose down 9>&-
  else
    pg_dev_ensure_state
    pg_dev_compose up -d --wait 9>&-
    pg_dev_auth 9>&- || { pg_fail "development authentication failed for $pg_project; credentials were not regenerated"; exit 1; }
  fi
  exec 9>&-
}

pg_main() {
  local mode="${1:-}"
  [[ "$#" -gt 0 ]] && shift
  case "$mode" in
    test | schema) pg_disposable "$mode" "$@" ;;
    dev-up | dev-down | dev-reset) pg_dev_lifecycle "$mode" "$@" ;;
    *) pg_fail 'usage: test|schema -- COMMAND ARG... | dev-up | dev-down | dev-reset CONFIRMED_VOLUME' ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -euo pipefail
  pg_main "$@"
fi
