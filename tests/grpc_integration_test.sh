#!/bin/bash
# Test script for gRPC source and reaction integration

set -e

echo "========================================="
echo "Testing gRPC Source and Reaction"
echo "========================================="

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Start the server with gRPC configuration
echo "Starting Drasi server with gRPC configuration..."
cargo run -- --config configs/grpc_example.yaml > server.log 2>&1 &
SERVER_PID=$!

# Wait for server to start
echo "Waiting for server to start..."
sleep 5

# Check if server started successfully
if ! ps -p $SERVER_PID > /dev/null; then
    echo -e "${RED}✗ Server failed to start${NC}"
    cat server.log
    exit 1
fi

echo -e "${GREEN}✓ Server started successfully${NC}"

# Check server health
echo "Checking server health..."
HEALTH_RESPONSE=$(curl -s http://localhost:8080/health)
if [[ $HEALTH_RESPONSE == *"healthy"* ]]; then
    echo -e "${GREEN}✓ Server is healthy${NC}"
else
    echo -e "${RED}✗ Server health check failed${NC}"
    echo "Response: $HEALTH_RESPONSE"
    kill $SERVER_PID
    exit 1
fi

# Check gRPC source status
echo "Checking gRPC source status..."
SOURCE_STATUS=$(curl -s http://localhost:8080/sources/grpc-events | jq -r '.data.status')
if [[ $SOURCE_STATUS == "running" ]]; then
    echo -e "${GREEN}✓ gRPC source is running${NC}"
else
    echo -e "${RED}✗ gRPC source is not running: $SOURCE_STATUS${NC}"
    kill $SERVER_PID
    exit 1
fi

# Build and run the gRPC client example (if it exists)
if [ -f "examples/grpc_client_example.rs" ]; then
    echo "Running gRPC client example..."
    cargo run --example grpc_client_example 2>/dev/null || {
        echo -e "${RED}✗ gRPC client example failed${NC}"
        kill $SERVER_PID
        exit 1
    }
    echo -e "${GREEN}✓ gRPC client example completed${NC}"
else
    echo "Note: gRPC client example not found, skipping client test"
fi

# Check query status
echo "Checking query status..."
QUERY_STATUS=$(curl -s http://localhost:8080/queries/all-nodes | jq -r '.data.status')
if [[ $QUERY_STATUS == "running" ]]; then
    echo -e "${GREEN}✓ Query is running${NC}"
else
    echo -e "${RED}✗ Query is not running: $QUERY_STATUS${NC}"
fi

# Check reaction status
echo "Checking gRPC reaction status..."
REACTION_STATUS=$(curl -s http://localhost:8080/reactions/grpc-sink | jq -r '.data.status')
if [[ $REACTION_STATUS == "running" ]]; then
    echo -e "${GREEN}✓ gRPC reaction is running${NC}"
else
    echo -e "${RED}✗ gRPC reaction is not running: $REACTION_STATUS${NC}"
fi

# Stop the server
echo "Stopping server..."
kill $SERVER_PID
wait $SERVER_PID 2>/dev/null || true

echo "========================================="
echo -e "${GREEN}✓ gRPC integration test completed${NC}"
echo "========================================="

# Clean up
rm -f server.log