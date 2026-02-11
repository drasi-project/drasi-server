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

# Drasi Trading Demo Startup Script
# This script starts all components of the trading demo in the correct order

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRASI_SERVER_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"

# Create logs directory if it doesn't exist
mkdir -p "$LOG_DIR"

echo "======================================"
echo "   Drasi Trading Demo Startup"
echo "======================================"
echo ""

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to wait for a service to be ready
wait_for_service() {
    local url=$1
    local service_name=$2
    local max_attempts=30
    local attempt=0
    
    echo -n "Waiting for $service_name to be ready..."
    while [ $attempt -lt $max_attempts ]; do
        if curl -s -o /dev/null -w "%{http_code}" "$url" | grep -q "200\|204"; then
            echo -e " ${GREEN}✓${NC}"
            return 0
        fi
        sleep 2
        attempt=$((attempt + 1))
        echo -n "."
    done
    echo -e " ${RED}✗${NC}"
    echo "Failed to connect to $service_name after $max_attempts attempts"
    return 1
}

# Check prerequisites
echo "Checking prerequisites..."

if ! command_exists docker; then
    echo -e "${RED}Error: Docker is not installed${NC}"
    exit 1
fi

if ! command_exists npm; then
    echo -e "${RED}Error: Node.js/npm is not installed${NC}"
    exit 1
fi

if ! command_exists python3; then
    echo -e "${RED}Error: Python 3 is not installed${NC}"
    exit 1
fi

if [ ! -f "$DRASI_SERVER_ROOT/target/release/drasi-server" ]; then
    echo -e "${YELLOW}Drasi Server binary not found. Building...${NC}"
    cd "$DRASI_SERVER_ROOT"
    cargo build --release
fi

echo -e "${GREEN}All prerequisites met!${NC}"
echo ""

# Step 1: Start PostgreSQL
echo "Step 1: Starting PostgreSQL database..."
cd "$SCRIPT_DIR/database"
docker-compose up -d

# Wait for PostgreSQL to be ready
echo -n "Waiting for PostgreSQL to be ready..."
max_attempts=30
attempt=0
while [ $attempt -lt $max_attempts ]; do
    if docker-compose exec -T postgres pg_isready -U drasi_user -d trading_demo >/dev/null 2>&1; then
        echo -e " ${GREEN}✓${NC}"
        break
    fi
    sleep 2
    attempt=$((attempt + 1))
    echo -n "."
done

if [ $attempt -eq $max_attempts ]; then
    echo -e " ${RED}✗${NC}"
    echo "PostgreSQL failed to start. Check logs with: docker-compose logs postgres"
    exit 1
fi

# Verify replication slot and publication
echo "Verifying PostgreSQL replication setup..."
SLOT_EXISTS=$(docker-compose exec -T postgres psql -U drasi_user -d trading_demo -t -c "SELECT slot_name FROM pg_replication_slots WHERE slot_name = 'drasi_trading_slot';" | tr -d ' ')
if [ -n "$SLOT_EXISTS" ]; then
    echo -e "Replication slot: ${GREEN}✓${NC}"
else
    echo -e "Replication slot: ${YELLOW}Will be created by Drasi Server${NC}"
fi

PUB_EXISTS=$(docker-compose exec -T postgres psql -U drasi_user -d trading_demo -t -c "SELECT pubname FROM pg_publication WHERE pubname = 'drasi_trading_pub';" | tr -d ' ')
if [ -n "$PUB_EXISTS" ]; then
    echo -e "Publication: ${GREEN}✓${NC}"
else
    echo -e "Publication: ${RED}Missing - creating...${NC}"
    docker-compose exec -T postgres psql -U postgres -d trading_demo -c "CREATE PUBLICATION drasi_trading_pub FOR TABLE stocks, portfolio, stock_prices;"
fi

# Step 2: Start Drasi Server
echo ""
echo "Step 2: Starting Drasi Server (sources only - app creates queries dynamically)..."

cd "$DRASI_SERVER_ROOT"
RUST_LOG=info,drasi_server::sources::postgres=debug \
    ./target/release/drasi-server --config "examples/trading/server/trading-sources-only.yaml" > "$LOG_DIR/drasi-server.log" 2>&1 &
DRASI_PID=$!
echo "Drasi Server started with PID: $DRASI_PID"
echo "Replication source will bootstrap initial data from PostgreSQL..."

# Give the server a moment to try binding to ports
sleep 2

# Check if the server process is still running
if ! kill -0 $DRASI_PID 2>/dev/null; then
    echo -e "${RED}✗ Drasi Server failed to start${NC}"
    echo "Checking log for errors..."
    tail -10 "$LOG_DIR/drasi-server.log" | grep -E "ERROR|Error|error" || tail -5 "$LOG_DIR/drasi-server.log"
    echo ""
    echo "Common issues:"
    echo "  - Port 8280 already in use (check with: lsof -i :8280)"
    echo "  - Port 9100 already in use (check with: lsof -i :9100)"
    echo "  - PostgreSQL connection failed"
    echo ""
    echo "To kill existing Drasi servers: pkill -f drasi-server"
    exit 1
fi

# Wait for Drasi Server to be ready
if ! wait_for_service "http://localhost:8280/health" "Drasi Server"; then
    echo -e "${RED}✗ Drasi Server API is not responding${NC}"
    echo "Server process is running but API is not available"
    echo "Check logs: tail -50 $LOG_DIR/drasi-server.log"
    kill $DRASI_PID 2>/dev/null
    exit 1
fi

# Verify sources are running
echo "Verifying Drasi sources..."
SOURCE_STATUS=$(curl -s http://localhost:8280/api/v1/sources)
if echo "$SOURCE_STATUS" | grep -q '"status":"running"'; then
    echo -e "PostgreSQL replication source: ${GREEN}✓ Running${NC}"
    echo -e "HTTP source: ${GREEN}✓ Running${NC}"
else
    echo -e "Sources: ${YELLOW}Starting...${NC}"
fi

# Give bootstrap time to complete
echo "Allowing time for bootstrap to complete..."
sleep 3

# Step 3: Install React app dependencies (if needed)
echo ""
echo "Step 3: Setting up React application..."
cd "$SCRIPT_DIR/app"
if [ ! -d "node_modules" ]; then
    echo "Installing npm dependencies..."
    npm install
else
    echo "Dependencies already installed"
fi

# Step 4: Start React app
echo "Starting React application..."
npm run dev > "$LOG_DIR/react-app.log" 2>&1 &
REACT_PID=$!
echo "React app started with PID: $REACT_PID"

# Wait for React app (Vite dev server runs on 5273)
wait_for_service "http://localhost:5273" "React application"

# Step 5: Install Python dependencies
echo ""
echo "Step 4: Setting up price generator..."
cd "$SCRIPT_DIR/mock-generator"
if ! python3 -c "import requests" 2>/dev/null; then
    echo "Installing Python dependencies..."
    pip3 install requests
else
    echo "Python dependencies already installed"
fi

# Step 6: Start price generator
echo "Starting simple price generator..."
python3 simple_price_generator.py > "$LOG_DIR/price-generator.log" 2>&1 &
GENERATOR_PID=$!
echo "Price generator started with PID: $GENERATOR_PID"

# Summary
echo ""
echo "======================================"
echo -e "${GREEN}   Demo Started Successfully!${NC}"
echo "======================================"
echo ""
echo "Access the demo at:"
echo "  • Trading UI: http://localhost:5273"
echo "  • Drasi API: http://localhost:8280"
echo "  • HTTP Source: http://localhost:9100"
echo "  • SSE Stream: http://localhost:8281/events"
echo ""
echo "Process PIDs:"
echo "  • Drasi Server: $DRASI_PID"
echo "  • React App: $REACT_PID"
echo "  • Price Generator: $GENERATOR_PID"
echo ""
echo "Logs are available at:"
echo "  • Drasi Server: $LOG_DIR/drasi-server.log"
echo "  • React App: $LOG_DIR/react-app.log"
echo "  • Price Generator: $LOG_DIR/price-generator.log"
echo ""
echo "To stop the demo, run: ./stop-demo.sh"
echo ""

# Save PIDs for stop script
echo "$DRASI_PID" > /tmp/drasi-demo-server.pid
echo "$REACT_PID" > /tmp/drasi-demo-react.pid
echo "$GENERATOR_PID" > /tmp/drasi-demo-generator.pid

# Keep script running and forward signals
trap "echo 'Stopping demo...'; kill $DRASI_PID $REACT_PID $GENERATOR_PID 2>/dev/null; cd $SCRIPT_DIR/database && docker-compose down; exit" INT TERM

echo "Press Ctrl+C to stop all services"
wait