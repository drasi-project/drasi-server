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

# Drasi Playground Shutdown Script

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"
PID_FILE="$LOG_DIR/pids.txt"

echo "======================================"
echo "   Drasi Playground Shutdown"
echo "======================================"
echo ""

# Color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Kill process by PID
kill_process() {
    local pid=$1
    local name=$2

    if ps -p $pid > /dev/null 2>&1; then
        echo -n "Stopping $name (PID: $pid)..."
        kill $pid 2>/dev/null || true
        sleep 2

        # Force kill if still running
        if ps -p $pid > /dev/null 2>&1; then
            kill -9 $pid 2>/dev/null || true
        fi
        echo -e " ${GREEN}✓${NC}"
    else
        echo "$name (PID: $pid) is not running"
    fi
}

# Read PIDs and stop processes
if [ -f "$PID_FILE" ]; then
    # Read all PIDs
    PIDS=($(cat "$PID_FILE"))

    if [ ${#PIDS[@]} -ge 2 ]; then
        kill_process ${PIDS[0]} "Drasi Server"
        kill_process ${PIDS[1]} "React app"
    fi

    # Clean up PID file
    rm -f "$PID_FILE"
    rm -f "$LOG_DIR/drasi-server.pid"
    rm -f "$LOG_DIR/react-app.pid"
else
    echo -e "${YELLOW}No PID file found. Attempting to find processes...${NC}"

    # Try to find and kill by port
    DRASI_PID=$(lsof -ti:8380 || true)
    REACT_PID=$(lsof -ti:5373 || true)

    if [ -n "$DRASI_PID" ]; then
        kill_process $DRASI_PID "Drasi Server"
    fi

    if [ -n "$REACT_PID" ]; then
        kill_process $REACT_PID "React app"
    fi
fi

echo ""
echo -e "${GREEN}Drasi Playground stopped successfully${NC}"
echo ""
