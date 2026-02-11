#!/bin/bash
# Post-create script for Drasi Server Playground

set -e

echo "🔧 Initializing Drasi Server Playground environment..."

# Install system dependencies
echo "📦 Installing system dependencies (OpenSSL, Protobuf, Clang)..."
sudo apt-get update && sudo apt-get install -y \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    clang \
    libclang-dev

# Build Drasi Server
echo "🔨 Building Drasi Server (this may take a few minutes)..."
cargo build --release

# Make scripts executable
chmod +x examples/playground/start.sh examples/playground/stop.sh

# Start the playground in background
echo "🚀 Starting Playground..."
nohup bash examples/playground/start.sh > /tmp/playground-startup.log 2>&1 &

echo ""
echo "✅ Drasi Server Playground is starting!"
echo "   Check startup progress: tail -f /tmp/playground-startup.log"
echo "   Playground App: http://localhost:5373"
echo "   Drasi API:      http://localhost:8380"
