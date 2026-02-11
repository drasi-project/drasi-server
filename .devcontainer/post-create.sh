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
    libclang-dev

# Build Drasi Server
echo "🔨 Building Drasi Server (this may take a few minutes)..."
cargo build

echo ""
echo "✅ Drasi Server development environment is ready!"
echo ""
echo "Getting started:"
echo "  cargo run -- --config <your-config.yaml>"
echo "  cargo test"
echo ""
echo "See examples/ for sample configurations."
