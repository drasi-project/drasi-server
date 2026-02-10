#!/bin/bash
# Post-create script for Drasi Server devcontainer

set -e

echo "🔧 Initializing Drasi Server development environment..."

# Install PostgreSQL client for database interactions
echo "🐘 Installing system dependencies (PostgreSQL client, OpenSSL, Protobuf, Clang)..."
sudo apt-get update && sudo apt-get install -y \
    postgresql-client \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    clang \
    libclang-dev

# Build Drasi Server in release mode
echo "🔨 Building Drasi Server (this may take a few minutes)..."
cargo build --release

# Create symlink for consistent binary access
ln -sf ./target/release/drasi-server ./drasi-server

# Make scripts executable
if [ -d "examples/getting-started/scripts" ]; then
    echo "📜 Making example scripts executable..."
    chmod +x examples/getting-started/scripts/*.sh
fi

echo ""
echo "✅ Drasi Server development environment is ready!"
