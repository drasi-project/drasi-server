#!/bin/bash

# Interactive demo showing data flow from Source → Query → Reaction
# Uses internal mock source for simplicity

set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

CONFIG_FILE="tests/configs/test_query_results_simple.yaml"
SERVER_PID=""

echo -e "${CYAN}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║         Drasi Server Interactive Demo                     ║${NC}"
echo -e "${CYAN}║                                                           ║${NC}"
echo -e "${CYAN}║  Shows data flow: Source → Query → Reaction              ║${NC}"
echo -e "${CYAN}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Stopping demo...${NC}"
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
    echo -e "${GREEN}Demo stopped!${NC}"
}

# Set trap for cleanup
trap cleanup EXIT

echo -e "${BLUE}📋 Configuration:${NC}"
echo "   - Source: Internal mock counter (updates every 1 second)"
echo "   - Query: Matches all Counter nodes and returns their values"
echo "   - Reaction: Logs query results to console"
echo ""

echo -e "${BLUE}🔨 Building Drasi Server...${NC}"
cargo build --release

echo -e "\n${BLUE}🚀 Starting Drasi Server...${NC}"

# Start server and capture output
RUST_LOG=info ./target/release/drasi-server --config "$CONFIG_FILE" 2>&1 | while IFS= read -r line; do
    # Highlight different types of messages
    if [[ "$line" == *"Counter:"* ]]; then
        echo -e "${GREEN}📊 $line${NC}"
    elif [[ "$line" == *"[counter-logger]"* ]]; then
        echo -e "${CYAN}🔔 $line${NC}"
    elif [[ "$line" == *"Server started"* ]]; then
        echo -e "${GREEN}✅ $line${NC}"
        echo -e "\n${YELLOW}👀 Watch the counter values increment below:${NC}\n"
    elif [[ "$line" == *"ERROR"* ]]; then
        echo -e "${RED}❌ $line${NC}"
    else
        echo "$line"
    fi
done &

SERVER_PID=$!

# Keep the script running
wait $SERVER_PID