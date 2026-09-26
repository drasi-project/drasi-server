#!/bin/bash
# Post-create script for Drasi Server default development environment

set -e

echo "🔧 Initializing Drasi Server development environment..."

# Install system dependencies
echo "📦 Installing system dependencies (OpenSSL, Protobuf, Clang)..."
sudo apt-get update && sudo apt-get install -y \
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
cargo build --locked

echo ""
echo "✅ Drasi Server development environment is ready!"
echo ""
echo "Getting started:"
echo "  cargo run -- --config <your-config.yaml>"
echo "  cargo test"
echo ""
echo "See examples/ for sample configurations."
