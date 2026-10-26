#!/bin/bash
# Smoke test for Drasi Server using the hello-world (mock source) config.
# Requires the binary, curl and Python 3.
#
# Usage:
#   SERVER_BINARY=./drasid ./run-integration-test.sh
#
# Environment variables:
#   SERVER_BINARY  - path to the drasi-server binary (required)
#   PLUGINS_DIR    - optional isolated plugin directory
#   SERVER_PORT    - port to use (default: 8080)
#   PYTHON         - Python executable (default: python3)

set -e

# ── Configuration ──────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_FILE="${CONFIG_FILE:-$SCRIPT_DIR/config.yaml}"
SERVER_BINARY="${SERVER_BINARY:?SERVER_BINARY must be set}"
PLUGINS_DIR="${PLUGINS_DIR:-}"
SERVER_LOG="${SERVER_LOG:-$SCRIPT_DIR/server.log}"
SERVER_PORT="${SERVER_PORT:-8080}"

# ── Colors ─────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

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

  server_args=(--config "$CONFIG_FILE" --port "$SERVER_PORT")
  if [ -n "$PLUGINS_DIR" ]; then
    mkdir -p "$PLUGINS_DIR"
    server_args+=(--plugins-dir "$PLUGINS_DIR")
  fi
  "$SERVER_BINARY" "${server_args[@]}" > "$SERVER_LOG" 2>&1 &
  SERVER_PID=$!
  log_info "Server started with PID: $SERVER_PID"

  # Wait up to 60 s for the health endpoint
  log_info "Waiting for server to become ready..."
  for _ in $(seq 1 30); do
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
      log_error "Server process exited unexpectedly!"
      log_error "=== Server log ==="
      cat "$SERVER_LOG"
      exit 1
    fi
    if curl -sf "http://localhost:$SERVER_PORT/health" > /dev/null 2>&1; then
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

# ── Main ───────────────────────────────────────────────────────────────────
main() {
  log_info "=== Hello-World Smoke Test ==="
  start_server
  if ! "${PYTHON:-python3}" "$SCRIPT_DIR/assert-api.py" \
      --url "http://localhost:$SERVER_PORT" \
      --output "${SMOKE_REPORT:-$SCRIPT_DIR/smoke.json}"; then
    cat "$SERVER_LOG"
    return 1
  fi
  kill -0 "$SERVER_PID"
}

main "$@"
