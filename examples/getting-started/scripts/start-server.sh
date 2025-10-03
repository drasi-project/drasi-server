#!/bin/bash
# Start the Drasi Server with the Getting Started configuration

set -e

# Determine the script's directory and navigate to project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$PROJECT_ROOT"

echo "Building Drasi Server..."
cargo build --release

echo ""
echo "Starting Drasi Server with Getting Started configuration..."
echo "API Server: http://localhost:8080"
echo "HTTP Source: http://localhost:9000"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""

./target/release/drasi-server --config examples/getting-started/server-config.yaml
