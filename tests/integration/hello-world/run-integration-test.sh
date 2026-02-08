#!/bin/bash
# Smoke test for Drasi Server using the hello-world (mock source) config.
# No external dependencies required — just the binary and curl.
#
# Usage:
#   SERVER_BINARY=./drasid ./run-integration-test.sh
#
# Environment variables:
#   SERVER_BINARY  - path to the drasi-server binary (required)
#   SERVER_PORT    - port to use (default: 8080)

set -e

# ── Configuration ──────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_FILE="${CONFIG_FILE:-$SCRIPT_DIR/config.yaml}"
SERVER_BINARY="${SERVER_BINARY:?SERVER_BINARY must be set}"
SERVER_LOG="${SERVER_LOG:-$SCRIPT_DIR/server.log}"
SERVER_PORT="${SERVER_PORT:-8080}"

# ── Colors ─────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

TESTS_PASSED=0
TESTS_FAILED=0

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }

# ── Cleanup ────────────────────────────────────────────────────────────────
cleanup() {
  if [ -n "$SERVER_PID" ]; then
    log_info "Stopping server (PID: $SERVER_PID)..."
    if kill -0 "$SERVER_PID" 2>/dev/null; then
      kill "$SERVER_PID" 2>/dev/null || true
      sleep 2
      if kill -0 "$SERVER_PID" 2>/dev/null; then
        kill -9 "$SERVER_PID" 2>/dev/null || true
      fi
    fi
    wait "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# ── Start server ───────────────────────────────────────────────────────────
start_server() {
  log_info "Starting Drasi Server..."
  log_info "  Binary: $SERVER_BINARY"
  log_info "  Config: $CONFIG_FILE"

  if [ ! -f "$SERVER_BINARY" ]; then
    log_error "Server binary not found at $SERVER_BINARY"
    exit 1
  fi
  if [ ! -x "$SERVER_BINARY" ]; then
    log_warn "Binary is not executable, adding +x"
    chmod +x "$SERVER_BINARY"
  fi
  if [ ! -f "$CONFIG_FILE" ]; then
    log_error "Config file not found at $CONFIG_FILE"
    exit 1
  fi

  "$SERVER_BINARY" --config "$CONFIG_FILE" > "$SERVER_LOG" 2>&1 &
  SERVER_PID=$!
  log_info "Server started with PID: $SERVER_PID"

  # Wait up to 60 s for the health endpoint
  log_info "Waiting for server to become ready..."
  for i in $(seq 1 30); do
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
      log_error "Server process exited unexpectedly!"
      log_error "=== Server log ==="
      cat "$SERVER_LOG"
      exit 1
    fi
    if curl -sf http://localhost:$SERVER_PORT/health > /dev/null 2>&1; then
      log_info "Server is ready!"
      return 0
    fi
    sleep 2
  done

  log_error "Server did not become ready within 60 s"
  log_error "=== Server log ==="
  cat "$SERVER_LOG"
  exit 1
}

# ── Test helpers ───────────────────────────────────────────────────────────
run_test() {
  local name="$1"; shift
  echo ""
  log_info "Running test: $name"
  if "$@"; then
    log_info "✓ $name"
    ((TESTS_PASSED++))
  else
    log_error "✗ $name"
    ((TESTS_FAILED++))
  fi
}

# ── Individual tests ───────────────────────────────────────────────────────
test_health() {
  local resp
  resp=$(curl -sf http://localhost:$SERVER_PORT/health)
  echo "  Health: $resp"
  echo "$resp" | grep -qi "ok"
}

test_sources() {
  local resp
  resp=$(curl -sf http://localhost:$SERVER_PORT/api/v1/sources)
  echo "  Sources: $resp"
  echo "$resp" | grep -q "test-source"
}

test_queries() {
  local resp
  resp=$(curl -sf http://localhost:$SERVER_PORT/api/v1/queries)
  echo "  Queries: $resp"
  echo "$resp" | grep -q "all-sensors"
}

test_query_results() {
  # The mock source emits data every 3 s; wait a bit for at least one cycle
  log_info "  Waiting for mock data (6 s)..."
  sleep 6

  local resp
  resp=$(curl -sf http://localhost:$SERVER_PORT/api/v1/queries/all-sensors/results)
  echo "  Results: $resp"
  # Should contain at least one SensorId field
  echo "$resp" | grep -q "SensorId"
}

# ── Main ───────────────────────────────────────────────────────────────────
main() {
  log_info "=== Hello-World Smoke Test ==="
  start_server

  set +e
  run_test "Health endpoint"       test_health
  run_test "Sources listed"        test_sources
  run_test "Queries listed"        test_queries
  run_test "Query produces results" test_query_results
  set -e

  echo ""
  log_info "=== Summary ==="
  log_info "Passed: $TESTS_PASSED"
  log_info "Failed: $TESTS_FAILED"

  if [ "$TESTS_FAILED" -eq 0 ]; then
    log_info "All tests passed! ✓"
  else
    log_error "Some tests failed! ✗"
    log_error "=== Server log ==="
    cat "$SERVER_LOG"
    return 1
  fi
}

main "$@"
