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

# Drasi Playground Startup Script

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRASI_SERVER_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"

# Create logs directory
mkdir -p "$LOG_DIR"

echo "======================================"
echo "   Drasi Playground Startup"
echo "======================================"
echo ""

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Wait for service
wait_for_service() {
    local url=$1
    local service_name=$2
    local max_attempts=30
    local attempt=0

    echo -n "Waiting for $service_name..."
    while [ $attempt -lt $max_attempts ]; do
        if curl -s -o /dev/null -w "%{http_code}" "$url" | grep -q "200\|204"; then
            echo -e " ${GREEN}✓${NC}"
            return 0
        fi
        sleep 1
        attempt=$((attempt + 1))
        echo -n "."
    done
    echo -e " ${RED}✗${NC}"
    return 1
}

# Check prerequisites
echo "Checking prerequisites..."

if ! command_exists npm; then
    echo -e "${RED}Error: Node.js/npm is not installed${NC}"
    exit 1
fi

if [ ! -f "$DRASI_SERVER_ROOT/target/release/drasi-server" ]; then
    echo -e "${YELLOW}Drasi Server binary not found. Building...${NC}"
    cd "$DRASI_SERVER_ROOT"
    cargo build --release
fi

echo -e "${GREEN}All prerequisites met!${NC}"
echo ""

# Step 1: Start Drasi Server
echo "Step 1: Starting Drasi Server..."
cd "$DRASI_SERVER_ROOT"
nohup ./target/release/drasi-server --config examples/playground/server/playground.yaml > "$LOG_DIR/drasi-server.log" 2>&1 &
DRASI_PID=$!
echo $DRASI_PID > "$LOG_DIR/drasi-server.pid"
echo -e "${GREEN}Drasi Server started (PID: $DRASI_PID)${NC}"

# Wait for Drasi Server
if ! wait_for_service "http://localhost:8380/health" "Drasi Server"; then
    echo -e "${RED}Failed to start Drasi Server${NC}"
    exit 1
fi

# Check sources
echo "Verifying sources are running..."
sleep 2

# Step 2: Install and start React app
echo ""
echo "Step 2: Starting React app..."
cd "$SCRIPT_DIR/app"

if [ ! -d "node_modules" ]; then
    echo "Installing npm dependencies..."
    npm install
fi

nohup npm run dev > "$LOG_DIR/react-app.log" 2>&1 &
REACT_PID=$!
echo $REACT_PID > "$LOG_DIR/react-app.pid"
echo -e "${GREEN}React app started (PID: $REACT_PID)${NC}"

# Wait for Vite dev server
if ! wait_for_service "http://localhost:5373" "React app"; then
    echo -e "${RED}Failed to start React app${NC}"
    exit 1
fi

# Save all PIDs
echo "$DRASI_PID" > "$LOG_DIR/pids.txt"
echo "$REACT_PID" >> "$LOG_DIR/pids.txt"

echo ""
echo "======================================"
echo -e "${GREEN}   Drasi Playground is ready!${NC}"
echo "======================================"
echo ""
echo "Access the playground at:"
echo -e "  ${GREEN}http://localhost:5373${NC}"
echo ""
echo "Drasi Server API:"
echo -e "  ${GREEN}http://localhost:8380${NC}"
echo ""
echo "To stop the playground, run:"
echo -e "  ${YELLOW}./stop-demo.sh${NC}"
echo ""
echo "Logs are available in: $LOG_DIR"
echo ""
