#!/bin/bash
# Post-create script for Drasi Server Trading Demo

set -e

echo "🔧 Initializing Drasi Server Trading Demo environment..."

# Ensure the shared Docker network exists
echo "🌐 Creating shared Docker network..."
docker network create drasi-network 2>/dev/null || true

# Install system dependencies
echo "📦 Installing system dependencies (PostgreSQL client, OpenSSL, Protobuf, Clang)..."
sudo apt-get update && sudo apt-get install -y \
    postgresql-client \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    clang \
    libclang-dev \
    libjq-dev \
    libonig-dev

# Set JQ_LIB_DIR for the jq-sys crate (architecture-aware)
export JQ_LIB_DIR="/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)"

# Build Drasi Server
echo "🔨 Building Drasi Server (this may take a few minutes)..."
cargo build --release

# Install Python dependencies for price generator
echo "🐍 Installing Python dependencies..."
pip3 install requests

# Make scripts executable
chmod +x examples/trading/start-demo.sh examples/trading/stop-demo.sh

# Start the trading demo in background
echo "🚀 Starting Trading Demo..."
nohup bash examples/trading/start-demo.sh > /tmp/trading-demo-startup.log 2>&1 &

echo ""
echo "✅ Drasi Server Trading Demo is starting!"
echo "   Check startup progress: tail -f /tmp/trading-demo-startup.log"
echo "   Trading App: http://localhost:5273"
echo "   Drasi API:   http://localhost:8280"
