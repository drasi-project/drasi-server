#!/bin/bash
# Run integration tests for Drasi Server
# Can be run in CI or locally

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG_FILE="${CONFIG_FILE:-$SCRIPT_DIR/config.yaml}"
SERVER_BINARY="${SERVER_BINARY:-$PROJECT_ROOT/target/release/drasi-server}"
SERVER_LOG="${SERVER_LOG:-$SCRIPT_DIR/server.log}"
SERVER_PORT="${SERVER_PORT:-8080}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-getting_started}"
DB_USER="${DB_USER:-drasi_user}"
DB_PASSWORD="${DB_PASSWORD:-drasi_password}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

log_info() {
  echo -e "${GREEN}[INFO]${NC} $1"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $1"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $1"
}

cleanup() {
  if [ -n "$SERVER_PID" ]; then
    log_info "Stopping server (PID: $SERVER_PID)..."

    # Check if process is still running before trying to kill it
    if kill -0 $SERVER_PID 2>/dev/null; then
      kill $SERVER_PID 2>/dev/null || true
      # Wait briefly for graceful shutdown
      sleep 2
      # Force kill if still running
      if kill -0 $SERVER_PID 2>/dev/null; then
        kill -9 $SERVER_PID 2>/dev/null || true
      fi
    fi

    # Wait for process to fully exit (don't fail if it's already gone)
    wait $SERVER_PID 2>/dev/null || true
  fi
}

trap cleanup EXIT

# Start the server
start_server() {
  log_info "Starting Drasi Server..."
  log_info "  Binary: $SERVER_BINARY"
  log_info "  Config: $CONFIG_FILE"
  log_info "  Log: $SERVER_LOG"

  if [ ! -f "$SERVER_BINARY" ]; then
    log_error "Server binary not found at $SERVER_BINARY"
    log_info "Please build the server first: cargo build --release"
    exit 1
  fi

  if [ ! -f "$CONFIG_FILE" ]; then
    log_error "Config file not found at $CONFIG_FILE"
    exit 1
  fi

  # Start server in background
  $SERVER_BINARY --config "$CONFIG_FILE" > "$SERVER_LOG" 2>&1 &
  SERVER_PID=$!
  log_info "Server started with PID: $SERVER_PID"

  # Wait for server to start (max 60 seconds)
  log_info "Waiting for server to start..."
  for i in {1..30}; do
    # Check if process is still running
    if ! kill -0 $SERVER_PID 2>/dev/null; then
      log_error "Server process died!"
      log_error "=== Server log ==="
      cat "$SERVER_LOG"
      exit 1
    fi

    # Check if server is responding
    if curl -s http://localhost:$SERVER_PORT/health > /dev/null 2>&1; then
      log_info "Server is ready!"
      return 0
    fi

    if [ $i -eq 30 ]; then
      log_error "Server did not start in time"
      log_error "=== Server log ==="
      cat "$SERVER_LOG"
      exit 1
    fi

    sleep 2
  done
}

# Test helper functions
run_test() {
  local test_name="$1"
  local test_command="$2"

  echo ""
  log_info "Running test: $test_name"

  if eval "$test_command"; then
    log_info "✓ Test passed: $test_name"
    ((TESTS_PASSED++))
    return 0
  else
    log_error "✗ Test failed: $test_name"
    ((TESTS_FAILED++))
    return 1
  fi
}

test_health_endpoint() {
  local response=$(curl -s http://localhost:$SERVER_PORT/health)
  echo "Health response: $response"
  echo "$response" | grep -q "ok"
}

test_sources_endpoint() {
  local response=$(curl -s http://localhost:$SERVER_PORT/api/sources)
  echo "Sources response: $response"
  echo "$response" | grep -q "postgres-messages"
}

test_queries_endpoint() {
  local response=$(curl -s http://localhost:$SERVER_PORT/api/queries)
  echo "Queries response: $response"
  echo "$response" | grep -q "hello-world-from"
}

test_query_results() {
  # Wait for bootstrap to complete and results to be processed
  log_info "Waiting for bootstrap to complete and results to be processed..."
  sleep 15

  # Test hello-world-from query
  local response=$(curl -s http://localhost:$SERVER_PORT/api/queries/hello-world-from/results)
  echo "hello-world-from results: $response"

  # Check if data array has items
  if echo "$response" | grep -q '"data":\[\]'; then
    log_warn "Query returned empty results, checking query status..."
    local status=$(curl -s http://localhost:$SERVER_PORT/api/queries/hello-world-from)
    echo "Query status: $status"
    return 1
  fi

  echo "$response" | grep -q "Brian Kernighan"
}

test_aggregation_results() {
  # Test message-count query
  local response=$(curl -s http://localhost:$SERVER_PORT/api/queries/message-count/results)
  echo "message-count results: $response"
  echo "$response" | grep -q "Hello World"
}

test_change_detection() {
  log_info "Inserting new message into database..."

  # Insert a new message (SERIAL will auto-increment now)
  PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -U $DB_USER -d $DB_NAME <<EOF
INSERT INTO message ("from", message) VALUES ('Alice', 'Hello World');
EOF

  # Wait for change to propagate
  log_info "Waiting for change to propagate..."
  sleep 5

  # Verify the new message appears in query results
  local response=$(curl -s http://localhost:$SERVER_PORT/api/queries/hello-world-from/results)
  echo "Updated hello-world-from results: $response"
  echo "$response" | grep -q "Alice"
}

# Main test execution
main() {
  log_info "=== Drasi Server Integration Tests ==="
  log_info ""

  # Start the server
  start_server

  # Run tests (don't exit on failure due to set -e, we want to run all tests)
  set +e
  run_test "Health endpoint" "test_health_endpoint"
  run_test "Sources endpoint" "test_sources_endpoint"
  run_test "Queries endpoint" "test_queries_endpoint"
  run_test "Query results (filter)" "test_query_results"
  run_test "Query results (aggregation)" "test_aggregation_results"
  run_test "Change detection" "test_change_detection"
  set -e

  # Print summary
  echo ""
  log_info "=== Test Summary ==="
  log_info "Tests passed: $TESTS_PASSED"
  log_info "Tests failed: $TESTS_FAILED"

  # Determine exit code
  local exit_code=0
  if [ $TESTS_FAILED -eq 0 ]; then
    log_info "All tests passed! ✓"
  else
    log_error "Some tests failed! ✗"
    log_error "=== Server log ==="
    cat "$SERVER_LOG"
    exit_code=1
  fi

  # Return exit code (cleanup will happen via trap)
  return $exit_code
}

# Run main and capture exit code
main "$@"
exit_code=$?

# Exit with proper code
exit $exit_code
