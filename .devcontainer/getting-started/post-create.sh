#!/bin/bash
# Post-create script for Drasi Server Getting Started tutorial

set -e

echo "🔧 Initializing Drasi Server Getting Started tutorial environment..."

# Ensure the shared Docker network exists (for connecting to PostgreSQL container)
echo "🌐 Creating shared Docker network..."
docker network create drasi-network 2>/dev/null || true

# Install system dependencies
echo "🐘 Installing system dependencies (PostgreSQL client, OpenSSL, Protobuf, Clang, jq, oniguruma)..."
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

# Build and install Drasi Server
echo "🔨 Building Drasi Server (this may take a few minutes)..."
cargo install --path . --root . --locked

# Make scripts executable
if [ -d "examples/getting-started/scripts" ]; then
    echo "📜 Making example scripts executable..."
    chmod +x examples/getting-started/scripts/*.sh
fi

echo ""
echo "✅ Drasi Server Getting Started tutorial environment is ready!"
