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

# Plugin Smoke Test
#
# Builds the Drasi Server in both static and dynamic modes, starts it,
# creates every registered source, reaction, and bootstrapper plugin via
# the REST API, and verifies that none of them causes a process crash.
#
# Usage:
#   ./tests/plugin_smoke_test.sh              # Test both modes
#   ./tests/plugin_smoke_test.sh --static     # Test only static build
#   ./tests/plugin_smoke_test.sh --dynamic    # Test only dynamic build
#   ./tests/plugin_smoke_test.sh --skip-build # Skip build, use existing binaries

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# Counters
TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0

# Options
RUN_STATIC=true
RUN_DYNAMIC=true
SKIP_BUILD=false
SERVER_PID=""
TEMP_CONFIG=""

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --static      Test only static build"
    echo "  --dynamic     Test only dynamic build"
    echo "  --skip-build  Skip build, use existing binaries"
    echo "  --help        Show this help"
}

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --static)  RUN_STATIC=true; RUN_DYNAMIC=false; shift ;;
        --dynamic) RUN_STATIC=false; RUN_DYNAMIC=true; shift ;;
        --skip-build) SKIP_BUILD=true; shift ;;
        --help) usage; exit 0 ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

# Cleanup on exit
cleanup() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
        echo -e "\n${YELLOW}Stopping server (PID $SERVER_PID)...${NC}"
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    if [[ -n "$TEMP_CONFIG" ]] && [[ -f "$TEMP_CONFIG" ]]; then
        rm -f "$TEMP_CONFIG"
    fi
}
trap cleanup EXIT

# Find an available port
find_free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("",0)); print(s.getsockname()[1]); s.close()' 2>/dev/null \
        || shuf -i 10000-60000 -n 1
}

# Wait for server to be ready
wait_for_server() {
    local port=$1
    local max_wait=30
    local waited=0
    while ! curl -sf "http://localhost:${port}/health" >/dev/null 2>&1; do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            echo -e "${RED}Server process died during startup!${NC}"
            return 1
        fi
        sleep 1
        waited=$((waited + 1))
        if [[ $waited -ge $max_wait ]]; then
            echo -e "${RED}Server did not become ready within ${max_wait}s${NC}"
            return 1
        fi
    done
    return 0
}

# Check if server process is still alive
assert_alive() {
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo -e "${RED}CRASH${NC}"
        return 1
    fi
    return 0
}

# Create a source via API and verify no crash
test_create_source() {
    local port=$1
    local instance_id=$2
    local kind=$3
    local id=$4
    local config=$5

    TOTAL=$((TOTAL + 1))
    local url="http://localhost:${port}/api/v1/instances/${instance_id}/sources"
    local body
    if [[ -n "$config" ]]; then
        body="{\"kind\":\"${kind}\",\"id\":\"${id}\",\"autoStart\":false,${config}}"
    else
        body="{\"kind\":\"${kind}\",\"id\":\"${id}\",\"autoStart\":false}"
    fi

    printf "  %-45s" "source/${kind} (${id})"
    local http_code
    http_code=$(curl -sf -o /dev/null -w "%{http_code}" \
        -X POST "$url" \
        -H "Content-Type: application/json" \
        -d "$body" 2>/dev/null) || http_code="000"

    if ! assert_alive; then
        FAILED=$((FAILED + 1))
        return 1
    fi

    if [[ "$http_code" == "200" ]]; then
        echo -e "${GREEN}OK${NC} (HTTP ${http_code})"
        PASSED=$((PASSED + 1))
    elif [[ "$http_code" == "000" ]]; then
        echo -e "${RED}FAIL${NC} (connection refused / crash)"
        FAILED=$((FAILED + 1))
    else
        # Non-200 but server is still alive — config issue, not a crash
        echo -e "${YELLOW}SKIP${NC} (HTTP ${http_code} — plugin needs external deps)"
        SKIPPED=$((SKIPPED + 1))
    fi
}

# Create a reaction via API and verify no crash
test_create_reaction() {
    local port=$1
    local instance_id=$2
    local kind=$3
    local id=$4
    local config=$5

    TOTAL=$((TOTAL + 1))
    local url="http://localhost:${port}/api/v1/instances/${instance_id}/reactions"
    local body
    if [[ -n "$config" ]]; then
        body="{\"kind\":\"${kind}\",\"id\":\"${id}\",\"queries\":[],\"autoStart\":false,${config}}"
    else
        body="{\"kind\":\"${kind}\",\"id\":\"${id}\",\"queries\":[],\"autoStart\":false}"
    fi

    printf "  %-45s" "reaction/${kind} (${id})"
    local http_code
    http_code=$(curl -sf -o /dev/null -w "%{http_code}" \
        -X POST "$url" \
        -H "Content-Type: application/json" \
        -d "$body" 2>/dev/null) || http_code="000"

    if ! assert_alive; then
        FAILED=$((FAILED + 1))
        return 1
    fi

    if [[ "$http_code" == "200" ]]; then
        echo -e "${GREEN}OK${NC} (HTTP ${http_code})"
        PASSED=$((PASSED + 1))
    elif [[ "$http_code" == "000" ]]; then
        echo -e "${RED}FAIL${NC} (connection refused / crash)"
        FAILED=$((FAILED + 1))
    else
        echo -e "${YELLOW}SKIP${NC} (HTTP ${http_code} — plugin needs external deps)"
        SKIPPED=$((SKIPPED + 1))
    fi
}

# Run all plugin creation tests against a running server
run_plugin_tests() {
    local port=$1
    local instance_id=$2

    echo ""
    echo "--- Sources ---"

    test_create_source "$port" "$instance_id" "mock" "smoke-mock" \
        "\"dataType\":{\"type\":\"generic\"},\"intervalMs\":60000"

    test_create_source "$port" "$instance_id" "http" "smoke-http" \
        "\"host\":\"localhost\",\"port\":19999"

    test_create_source "$port" "$instance_id" "grpc" "smoke-grpc" \
        "\"endpoint\":\"http://localhost:19999\""

    test_create_source "$port" "$instance_id" "postgres" "smoke-pg-src" \
        "\"database\":\"testdb\",\"user\":\"testuser\""

    test_create_source "$port" "$instance_id" "mssql" "smoke-mssql-src" \
        "\"database\":\"testdb\",\"user\":\"testuser\""

    test_create_source "$port" "$instance_id" "platform" "smoke-platform-src" \
        "\"redisUrl\":\"redis://localhost:16379\",\"streamKey\":\"test-stream\""

    echo ""
    echo "--- Reactions ---"

    test_create_reaction "$port" "$instance_id" "log" "smoke-log" \
        ""

    test_create_reaction "$port" "$instance_id" "http" "smoke-http-react" \
        "\"baseUrl\":\"http://localhost:19999\""

    test_create_reaction "$port" "$instance_id" "http-adaptive" "smoke-http-adaptive" \
        "\"baseUrl\":\"http://localhost:19999\""

    test_create_reaction "$port" "$instance_id" "grpc" "smoke-grpc-react" \
        "\"endpoint\":\"localhost:19999\""

    test_create_reaction "$port" "$instance_id" "grpc-adaptive" "smoke-grpc-adaptive" \
        "\"endpoint\":\"localhost:19999\""

    test_create_reaction "$port" "$instance_id" "sse" "smoke-sse" \
        ""

    test_create_reaction "$port" "$instance_id" "profiler" "smoke-profiler" \
        "\"windowSize\":100,\"reportIntervalSecs\":10"

    test_create_reaction "$port" "$instance_id" "storedproc-postgres" "smoke-sp-pg" \
        "\"user\":\"testuser\",\"password\":\"testpass\",\"database\":\"testdb\""

    test_create_reaction "$port" "$instance_id" "storedproc-mysql" "smoke-sp-mysql" \
        "\"user\":\"testuser\",\"password\":\"testpass\",\"database\":\"testdb\""

    test_create_reaction "$port" "$instance_id" "storedproc-mssql" "smoke-sp-mssql" \
        "\"user\":\"testuser\",\"password\":\"testpass\",\"database\":\"testdb\""

    test_create_reaction "$port" "$instance_id" "platform" "smoke-platform-react" \
        "\"redisUrl\":\"redis://localhost:16379\""

    echo ""
    echo "--- Final Health Check ---"
    TOTAL=$((TOTAL + 1))
    printf "  %-45s" "server still responsive"
    if curl -sf "http://localhost:${port}/health" >/dev/null 2>&1 && assert_alive; then
        echo -e "${GREEN}OK${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}FAIL${NC}"
        FAILED=$((FAILED + 1))
    fi
}

# Start server and run tests for a given build mode
test_build_mode() {
    local mode=$1        # "static" or "dynamic"
    local binary=$2      # path to the binary

    echo -e "\n${CYAN}============================================${NC}"
    echo -e "${CYAN}  Testing ${mode} build${NC}"
    echo -e "${CYAN}============================================${NC}"

    if [[ ! -x "$binary" ]]; then
        echo -e "${RED}Binary not found or not executable: ${binary}${NC}"
        echo -e "${RED}Run the build first or use --skip-build=false${NC}"
        TOTAL=$((TOTAL + 1))
        FAILED=$((FAILED + 1))
        return 1
    fi

    local port
    port=$(find_free_port)
    local instance_id="smoke-test"

    # Create a minimal config
    TEMP_CONFIG=$(mktemp /tmp/drasi-smoke-XXXXXX.yaml)
    cat > "$TEMP_CONFIG" <<EOF
apiVersion: drasi.io/v1
id: "${instance_id}"
host: "0.0.0.0"
port: ${port}
logLevel: "info"
persistConfig: false
sources: []
queries: []
reactions: []
EOF

    echo "Starting server on port ${port}..."

    # Start the server in the background, redirect output to log file
    local server_log
    server_log=$(mktemp /tmp/drasi-smoke-server-XXXXXX.log)
    "$binary" --config "$TEMP_CONFIG" --port "$port" >"$server_log" 2>&1 &
    SERVER_PID=$!

    # Wait for it to be ready
    if ! wait_for_server "$port"; then
        echo -e "${RED}Server failed to start in ${mode} mode${NC}"
        echo "Server log (last 30 lines):"
        tail -30 "$server_log" 2>/dev/null | sed 's/^/    /'
        if [[ -n "$SERVER_PID" ]]; then
            kill "$SERVER_PID" 2>/dev/null || true
            wait "$SERVER_PID" 2>/dev/null || true
        fi
        SERVER_PID=""
        rm -f "$server_log"
        TOTAL=$((TOTAL + 1))
        FAILED=$((FAILED + 1))
        return 1
    fi

    echo -e "${GREEN}Server ready (PID ${SERVER_PID})${NC}"

    # Run plugin creation tests
    run_plugin_tests "$port" "$instance_id"

    # Stop the server gracefully
    echo ""
    echo "Stopping server..."
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
    rm -f "$TEMP_CONFIG" "$server_log"
    TEMP_CONFIG=""

    echo -e "${GREEN}Server stopped cleanly${NC}"
}

# ============================================================
# Main
# ============================================================

cd "$PROJECT_DIR"

echo -e "${CYAN}╔══════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     Drasi Plugin Smoke Test Suite        ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════╝${NC}"

# Static build
if $RUN_STATIC; then
    STATIC_BINARY="target/debug/drasi-server"

    if ! $SKIP_BUILD; then
        echo -e "\n${CYAN}Building static binary...${NC}"
        cargo build 2>&1 | tail -3
        echo -e "${GREEN}Static build complete${NC}"
    fi

    test_build_mode "static" "$STATIC_BINARY"
fi

# Dynamic build
if $RUN_DYNAMIC; then
    DYNAMIC_BINARY="target/debug/drasi-server"

    if ! $SKIP_BUILD; then
        echo -e "\n${CYAN}Building dynamic binary + plugins...${NC}"
        make build-dynamic 2>&1 | tail -10
        echo -e "${GREEN}Dynamic build complete${NC}"
    fi

    test_build_mode "dynamic" "$DYNAMIC_BINARY"
fi

# ============================================================
# Summary
# ============================================================

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║              Test Summary                ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════╝${NC}"
echo ""
echo -e "  Total:   ${TOTAL}"
echo -e "  Passed:  ${GREEN}${PASSED}${NC}"
echo -e "  Skipped: ${YELLOW}${SKIPPED}${NC}"
echo -e "  Failed:  ${RED}${FAILED}${NC}"
echo ""

if [[ $FAILED -eq 0 ]]; then
    echo -e "${GREEN}All plugin smoke tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed — see output above${NC}"
    exit 1
fi
