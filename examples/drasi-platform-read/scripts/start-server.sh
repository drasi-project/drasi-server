#!/bin/bash
# Start Drasi Server with platform example configuration

set -e

# Resolve absolute paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

echo "==================================="
echo "Drasi Platform Example - Startup"
echo "==================================="
echo ""

# Navigate to project root
cd "$PROJECT_ROOT"

# Build the server
echo "Building Drasi Server..."
cargo build --release

echo ""
echo "Starting Drasi Server with platform configuration..."
echo "Config: examples/drasi-platform/server-config.yaml"
echo ""
echo "The server will:"
echo "  - Connect to Redis at localhost:6379"
echo "  - Subscribe to stream 'facilities-db-change'"
echo "  - Calculate ComfortLevel for each Room"
echo "  - Log results to console"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""

# Start the server
./target/release/drasi-server --config examples/drasi-platform-read/server-config.yaml
