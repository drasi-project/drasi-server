#!/bin/bash
# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Run End-to-End Script
#
# Runs the full getting-started demo as a self-checking integration test:
#   1. Starts the PostgreSQL container (via setup-database.sh).
#   2. Builds Drasi Server in release mode (skipped if SKIP_BUILD=1).
#   3. Launches the server in the background against server-config.yaml.
#   4. Waits for /health, then verifies sources and queries via REST.
#   5. Inserts a "Hello World" row and asserts it appears in the
#      hello-world-from query results (CDC end-to-end).
#   6. Stops the server. Leaves Postgres running unless CLEANUP=1
#      (then runs cleanup.sh).
#
# Environment overrides:
#   SERVER_HOST       (default: localhost)
#   SERVER_PORT       (default: 8080)
#   STARTUP_TIMEOUT   seconds to wait for /health     (default: 60)
#   RESULT_TIMEOUT    seconds to wait for CDC result  (default: 30)
#   SKIP_BUILD        if set, skip cargo build --release
#   CLEANUP           if set, run cleanup.sh on exit (tears down Postgres)
#   LOCAL_PLUGINS_DIR if set, override pluginRegistry to this local dir
#                     (useful when the OCI registry has no compatible
#                      build for your platform — e.g. recent darwin-arm64).
#                     Typical value: ../drasi-core/target/release/plugins
#
# Exit code: 0 on success, 1 on any failure.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SERVER_ROOT="$(cd "$EXAMPLE_DIR/../.." && pwd)"
CONFIG_FILE="$EXAMPLE_DIR/server-config.yaml"
SERVER_LOG="$EXAMPLE_DIR/server.log"
SERVER_BIN="$SERVER_ROOT/target/release/drasi-server"

SERVER_HOST="${SERVER_HOST:-localhost}"
SERVER_PORT="${SERVER_PORT:-8080}"
STARTUP_TIMEOUT="${STARTUP_TIMEOUT:-60}"
RESULT_TIMEOUT="${RESULT_TIMEOUT:-30}"
LOCAL_PLUGINS_DIR="${LOCAL_PLUGINS_DIR:-}"

POSTGRES_CONTAINER="getting-started-postgres"
POSTGRES_USER="drasi_user"
POSTGRES_DB="getting_started"

# Colors
RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
YELLOW=$'\033[1;33m'
BLUE=$'\033[0;34m'
NC=$'\033[0m'

log_info()  { echo "${GREEN}[INFO]${NC}  $*"; }
log_step()  { echo; echo "${BLUE}=== $* ===${NC}"; }
log_warn()  { echo "${YELLOW}[WARN]${NC}  $*"; }
log_error() { echo "${RED}[ERROR]${NC} $*" >&2; }

SERVER_PID=""

cleanup() {
  local exit_code=$?

  if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    log_info "Stopping Drasi Server (PID: $SERVER_PID)..."
    kill "$SERVER_PID" 2>/dev/null || true
    # Wait briefly for graceful shutdown
    for _ in 1 2 3 4 5; do
      kill -0 "$SERVER_PID" 2>/dev/null || break
      sleep 1
    done
    if kill -0 "$SERVER_PID" 2>/dev/null; then
      log_warn "Server did not exit; sending SIGKILL"
      kill -9 "$SERVER_PID" 2>/dev/null || true
    fi
    wait "$SERVER_PID" 2>/dev/null || true
  fi

  if [ "${CLEANUP:-}" = "1" ]; then
    log_info "CLEANUP=1 — tearing down PostgreSQL via cleanup.sh"
    "$SCRIPT_DIR/cleanup.sh" || true
  fi

  exit "$exit_code"
}
trap cleanup EXIT INT TERM

# --- Step 1: Postgres ---------------------------------------------------------

log_step "Step 1: Start PostgreSQL"

# Determine docker compose command
if command -v docker-compose >/dev/null 2>&1; then
  COMPOSE_CMD="docker-compose"
elif docker compose version >/dev/null 2>&1; then
  COMPOSE_CMD="docker compose"
else
  log_error "docker compose is not installed"
  exit 1
fi

# Ensure shared docker network exists (referenced as external by docker-compose.yml)
docker network inspect drasi-network >/dev/null 2>&1 || docker network create drasi-network >/dev/null

# Start (or reuse) the Postgres container
COMPOSE_DIR="$EXAMPLE_DIR/database"
log_info "Bringing up '${POSTGRES_CONTAINER}' via $COMPOSE_CMD"
(cd "$COMPOSE_DIR" && $COMPOSE_CMD up -d)

# Wait for Postgres to accept connections (as the built-in 'postgres' superuser)
log_info "Waiting for PostgreSQL to be ready..."
for i in $(seq 1 30); do
  if docker exec "$POSTGRES_CONTAINER" pg_isready -U postgres -d "$POSTGRES_DB" >/dev/null 2>&1; then
    log_info "PostgreSQL is ready"
    break
  fi
  if [ "$i" = "30" ]; then
    log_error "PostgreSQL did not become ready within 60s"
    docker logs --tail 80 "$POSTGRES_CONTAINER" >&2 || true
    exit 1
  fi
  sleep 2
done

# Bootstrap schema/user/publication that matches server-config.yaml.
# This is idempotent so re-runs against an existing volume are safe.
#
# IMPORTANT: schema/publication and replication slot creation MUST be
# in separate psql sessions. PostgreSQL silently rolls back DDL (e.g.
# CREATE PUBLICATION) when pg_create_logical_replication_slot is
# called later in the same session via DO/PERFORM.
# ALSO: a logical replication slot's catalog snapshot is captured when
# the slot is created. If the publication is dropped/recreated later,
# the slot can no longer see it ("publication does not exist" at
# START_REPLICATION). Therefore: whenever we drop+recreate the
# publication we MUST also drop+recreate the slot.
log_info "Applying schema (user, message table, publication)..."
docker exec -i "$POSTGRES_CONTAINER" psql -v ON_ERROR_STOP=1 -U postgres -d "$POSTGRES_DB" <<'SQL'
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_user WHERE usename = 'drasi_user') THEN
        CREATE USER drasi_user WITH REPLICATION LOGIN PASSWORD 'drasi_password';
    END IF;
END
$$;

GRANT ALL PRIVILEGES ON DATABASE getting_started TO drasi_user;
GRANT USAGE, CREATE ON SCHEMA public TO drasi_user;

DROP TABLE IF EXISTS message CASCADE;
DROP PUBLICATION IF EXISTS drasi_getting_started_pub;

CREATE TABLE message (
    messageid SERIAL PRIMARY KEY,
    "from"    VARCHAR(255) NOT NULL,
    message   TEXT         NOT NULL,
    created_at TIMESTAMP   DEFAULT CURRENT_TIMESTAMP
);
ALTER TABLE message REPLICA IDENTITY FULL;
ALTER TABLE message OWNER TO drasi_user;

INSERT INTO message (messageid, "from", message) VALUES
    (1, 'Buzz Lightyear',  'To infinity and beyond!'),
    (2, 'Brian Kernighan', 'Hello World'),
    (3, 'Antoninus',       'I am Spartacus'),
    (4, 'David',           'I am Spartacus');
SELECT setval('message_messageid_seq', (SELECT MAX(messageid) FROM message));

CREATE PUBLICATION drasi_getting_started_pub FOR TABLE message;
SQL

# Drop any existing slot first (its catalog snapshot is stale after
# the publication was recreated above), then create a fresh one in
# its own session.
log_info "(Re)creating replication slot in fresh session..."
docker exec -i "$POSTGRES_CONTAINER" psql -v ON_ERROR_STOP=1 -U postgres -d "$POSTGRES_DB" <<'SQL'
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_replication_slots WHERE slot_name = 'drasi_getting_started_slot') THEN
        PERFORM pg_drop_replication_slot('drasi_getting_started_slot');
    END IF;
END
$$;
SQL
docker exec -i "$POSTGRES_CONTAINER" psql -v ON_ERROR_STOP=1 -U postgres -d "$POSTGRES_DB" <<'SQL'
SELECT pg_create_logical_replication_slot('drasi_getting_started_slot', 'pgoutput');
SQL

# Verify both publication and slot are present before continuing.
verify_out=$(docker exec "$POSTGRES_CONTAINER" psql -tA -U postgres -d "$POSTGRES_DB" \
    -c "SELECT (SELECT count(*) FROM pg_publication WHERE pubname='drasi_getting_started_pub')
         || ',' ||
        (SELECT count(*) FROM pg_replication_slots WHERE slot_name='drasi_getting_started_slot');" \
    2>&1 | tr -d '[:space:]')
if [ "$verify_out" != "1,1" ]; then
  log_error "Schema verification failed: expected publication=1,slot=1 got '$verify_out'"
  exit 1
fi

log_info "Schema applied (publication + slot verified)"

# --- Step 2: Build server -----------------------------------------------------

if [ "${SKIP_BUILD:-}" = "1" ]; then
  log_step "Step 2: Skipping build (SKIP_BUILD=1)"
  if [ ! -x "$SERVER_BIN" ]; then
    log_error "SKIP_BUILD set but binary not found at $SERVER_BIN"
    exit 1
  fi
else
  log_step "Step 2: Build Drasi Server (release)"
  (cd "$SERVER_ROOT" && cargo build --release)
fi

# --- Step 3: Start server -----------------------------------------------------

log_step "Step 3: Start Drasi Server"
log_info "  Binary: $SERVER_BIN"
log_info "  Config: $CONFIG_FILE"
log_info "  Log:    $SERVER_LOG"

# server-config.yaml uses ${POSTGRES_PASSWORD} etc.; export from .env if present.
if [ -f "$EXAMPLE_DIR/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  source "$EXAMPLE_DIR/.env"
  set +a
fi
: "${POSTGRES_PASSWORD:=drasi_password}"
export POSTGRES_PASSWORD

# If the user did not explicitly set LOCAL_PLUGINS_DIR, try to
# auto-detect a sibling drasi-core build. The OCI registry currently
# does not publish darwin-arm64 builds for source/postgres,
# reaction/log, or reaction/sse with the SDK version this branch
# requires, so on Apple Silicon the registry path will fail.
REQUIRED_PLUGINS=(libdrasi_source_postgres libdrasi_bootstrap_postgres
                  libdrasi_reaction_log libdrasi_reaction_sse)
plugin_ext="dylib"
case "$(uname -s)" in
  Linux)   plugin_ext="so"    ;;
  Darwin)  plugin_ext="dylib" ;;
  MINGW*|MSYS*|CYGWIN*) plugin_ext="dll" ;;
esac

local_plugins_have_all() {
  local dir="$1"
  [ -d "$dir" ] || return 1
  local p
  for p in "${REQUIRED_PLUGINS[@]}"; do
    [ -f "$dir/${p}.${plugin_ext}" ] || return 1
  done
  return 0
}

if [ -z "$LOCAL_PLUGINS_DIR" ]; then
  CANDIDATE="$SERVER_ROOT/../drasi-core/target/release/plugins"
  if local_plugins_have_all "$CANDIDATE"; then
    LOCAL_PLUGINS_DIR="$CANDIDATE"
    log_info "Auto-detected sibling drasi-core build with required plugins."
    log_info "Using LOCAL_PLUGINS_DIR='$LOCAL_PLUGINS_DIR'"
    log_info "(Set LOCAL_PLUGINS_DIR=skip to force the OCI registry path.)"
  elif [ -d "$SERVER_ROOT/../drasi-core" ] && [ "${BUILD_LOCAL_PLUGINS:-}" = "1" ]; then
    log_info "BUILD_LOCAL_PLUGINS=1 — running 'make build-local-plugins'..."
    (cd "$SERVER_ROOT" && make build-local-plugins)
    if local_plugins_have_all "$CANDIDATE"; then
      LOCAL_PLUGINS_DIR="$CANDIDATE"
      log_info "Built and using LOCAL_PLUGINS_DIR='$LOCAL_PLUGINS_DIR'"
    fi
  fi
fi

# Allow explicit opt-out: LOCAL_PLUGINS_DIR=skip means "do not use local".
if [ "${LOCAL_PLUGINS_DIR:-}" = "skip" ]; then
  LOCAL_PLUGINS_DIR=""
fi

# If a local plugins directory is in play, generate a temp config that
# overrides pluginRegistry to bypass the OCI registry entirely.
EFFECTIVE_CONFIG="$CONFIG_FILE"
SERVER_EXTRA_ARGS=()
if [ -n "$LOCAL_PLUGINS_DIR" ]; then
  if [ ! -d "$LOCAL_PLUGINS_DIR" ]; then
    log_error "LOCAL_PLUGINS_DIR='$LOCAL_PLUGINS_DIR' is not a directory"
    log_error "Hint: build it with 'make build-local-plugins' from the drasi-server repo root,"
    log_error "      or unset LOCAL_PLUGINS_DIR to use the OCI registry."
    exit 1
  fi
  if ! local_plugins_have_all "$LOCAL_PLUGINS_DIR"; then
    log_error "LOCAL_PLUGINS_DIR='$LOCAL_PLUGINS_DIR' is missing required plugins:"
    for p in "${REQUIRED_PLUGINS[@]}"; do
      [ -f "$LOCAL_PLUGINS_DIR/${p}.${plugin_ext}" ] || log_error "  - ${p}.${plugin_ext}"
    done
    log_error "Hint: rebuild them with 'make build-local-plugins' from the drasi-server repo root."
    exit 1
  fi
  ABS_PLUGINS_DIR="$(cd "$LOCAL_PLUGINS_DIR" && pwd)"
  EFFECTIVE_CONFIG="$EXAMPLE_DIR/server-config.local.yaml"
  log_info "Generating $EFFECTIVE_CONFIG with pluginRegistry='$ABS_PLUGINS_DIR'"
  # Strip any existing pluginRegistry: line, then prepend a fresh one.
  {
    echo "# Auto-generated by run-end-to-end.sh — DO NOT EDIT"
    echo "pluginRegistry: \"$ABS_PLUGINS_DIR\""
    grep -v '^[[:space:]]*pluginRegistry:' "$CONFIG_FILE"
  } >"$EFFECTIVE_CONFIG"
  # Skip cosign verification because local builds aren't signed.
  SERVER_EXTRA_ARGS+=(--skip-verification --plugins-dir "$ABS_PLUGINS_DIR")
fi

"$SERVER_BIN" --config "$EFFECTIVE_CONFIG" ${SERVER_EXTRA_ARGS[@]+"${SERVER_EXTRA_ARGS[@]}"} >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
log_info "Server started with PID: $SERVER_PID"

log_info "Waiting up to ${STARTUP_TIMEOUT}s for http://${SERVER_HOST}:${SERVER_PORT}/health..."
deadline=$(( $(date +%s) + STARTUP_TIMEOUT ))
while :; do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    log_error "Server process died during startup"
    log_error "--- tail of $SERVER_LOG ---"
    tail -n 80 "$SERVER_LOG" >&2 || true
    exit 1
  fi
  if curl -fsS "http://${SERVER_HOST}:${SERVER_PORT}/health" >/dev/null 2>&1; then
    log_info "Server is ready"
    break
  fi
  if [ "$(date +%s)" -ge "$deadline" ]; then
    log_error "Server did not become healthy within ${STARTUP_TIMEOUT}s"
    log_error "--- tail of $SERVER_LOG ---"
    tail -n 80 "$SERVER_LOG" >&2 || true
    exit 1
  fi
  sleep 1
done

# --- Step 4: REST sanity checks -----------------------------------------------

TESTS_PASSED=0
TESTS_FAILED=0

assert_endpoint() {
  local name="$1" url="$2" needle="$3"
  log_info "Check: $name"
  local body
  if ! body=$(curl -fsS "$url"); then
    log_error "  ✗ $name — request failed"
    TESTS_FAILED=$((TESTS_FAILED + 1))
    return 1
  fi
  if echo "$body" | grep -q -- "$needle"; then
    log_info "  ✓ $name"
    TESTS_PASSED=$((TESTS_PASSED + 1))
    return 0
  fi
  log_error "  ✗ $name — response missing '$needle'"
  log_error "    body: $body"
  TESTS_FAILED=$((TESTS_FAILED + 1))
  return 1
}

log_step "Step 4: REST sanity checks"
assert_endpoint "sources list contains postgres-messages" \
  "http://${SERVER_HOST}:${SERVER_PORT}/api/v1/sources" \
  '"postgres-messages"' || true
assert_endpoint "queries list contains hello-world-from" \
  "http://${SERVER_HOST}:${SERVER_PORT}/api/v1/queries" \
  '"hello-world-from"' || true
assert_endpoint "hello-world-from is configured" \
  "http://${SERVER_HOST}:${SERVER_PORT}/api/v1/queries/hello-world-from" \
  '"id":"hello-world-from"' || true
assert_endpoint "message-count is configured" \
  "http://${SERVER_HOST}:${SERVER_PORT}/api/v1/queries/message-count" \
  '"id":"message-count"' || true

# --- Step 5: CDC change-detection --------------------------------------------

log_step "Step 5: CDC change-detection"

UNIQUE_FROM="EndToEnd-$(date +%s)"
log_info "Inserting message ('${UNIQUE_FROM}', 'Hello World')"
docker exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c \
  "INSERT INTO message (\"from\", message) VALUES ('${UNIQUE_FROM}', 'Hello World');" \
  >/dev/null

log_info "Polling /api/v1/queries/hello-world-from/results for up to ${RESULT_TIMEOUT}s..."
deadline=$(( $(date +%s) + RESULT_TIMEOUT ))
matched=0
while [ "$(date +%s)" -lt "$deadline" ]; do
  results=$(curl -fsS "http://${SERVER_HOST}:${SERVER_PORT}/api/v1/queries/hello-world-from/results" || true)
  if echo "$results" | grep -q "$UNIQUE_FROM"; then
    matched=1
    break
  fi
  sleep 1
done

if [ "$matched" = "1" ]; then
  log_info "  ✓ Inserted row appeared in query results"
  TESTS_PASSED=$((TESTS_PASSED + 1))
else
  log_error "  ✗ Inserted row did not appear within ${RESULT_TIMEOUT}s"
  log_error "    last results: ${results:-<empty>}"
  TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# --- Summary ------------------------------------------------------------------

log_step "Summary"
log_info "Passed: $TESTS_PASSED"
if [ "$TESTS_FAILED" -gt 0 ]; then
  log_error "Failed: $TESTS_FAILED"
  log_error "--- tail of $SERVER_LOG ---"
  tail -n 80 "$SERVER_LOG" >&2 || true
  exit 1
fi

echo
echo "${GREEN}All getting-started end-to-end checks passed ✓${NC}"
echo
if [ "${CLEANUP:-}" != "1" ]; then
  echo "PostgreSQL container '${POSTGRES_CONTAINER}' is still running for further exploration."
  echo "Run './scripts/cleanup.sh' (or re-run with CLEANUP=1) to tear it down."
fi
