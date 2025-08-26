#!/bin/bash

# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http:#www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Drasi Trading Demo Stop Script
# This script stops all components of the trading demo

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "======================================"
echo "   Stopping Drasi Trading Demo"
echo "======================================"
echo ""

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Stop processes using saved PIDs
if [ -f /tmp/drasi-demo-generator.pid ]; then
    PID=$(cat /tmp/drasi-demo-generator.pid)
    if kill $PID 2>/dev/null; then
        echo -e "Stopped price generator (PID: $PID) ${GREEN}✓${NC}"
    else
        echo -e "Price generator already stopped ${YELLOW}✓${NC}"
    fi
    rm /tmp/drasi-demo-generator.pid
fi

if [ -f /tmp/drasi-demo-react.pid ]; then
    PID=$(cat /tmp/drasi-demo-react.pid)
    if kill $PID 2>/dev/null; then
        echo -e "Stopped React app (PID: $PID) ${GREEN}✓${NC}"
    else
        echo -e "React app already stopped ${YELLOW}✓${NC}"
    fi
    rm /tmp/drasi-demo-react.pid
fi

if [ -f /tmp/drasi-demo-server.pid ]; then
    PID=$(cat /tmp/drasi-demo-server.pid)
    if kill $PID 2>/dev/null; then
        echo -e "Stopped Drasi Server (PID: $PID) ${GREEN}✓${NC}"
    else
        echo -e "Drasi Server already stopped ${YELLOW}✓${NC}"
    fi
    rm /tmp/drasi-demo-server.pid
fi

# Stop PostgreSQL
echo "Stopping PostgreSQL..."
cd "$SCRIPT_DIR/database"
docker-compose down

echo ""
echo -e "${GREEN}Demo stopped successfully!${NC}"
echo ""

# Optional: Ask if user wants to clean up data
read -p "Do you want to remove PostgreSQL data volume? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    docker-compose down -v
    echo -e "PostgreSQL data volume removed ${GREEN}✓${NC}"
    echo "Note: This will require re-initialization on next start"
fi

# Optional: Clean up logs
read -p "Do you want to remove log files? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -f /tmp/drasi-server.log /tmp/react-app.log /tmp/price-generator.log
    echo -e "Log files removed ${GREEN}✓${NC}"
fi