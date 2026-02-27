#!/bin/bash
# Post-create script for Drasi Server Getting Started tutorial

set -e

echo "🔧 Initializing Drasi Server Getting Started tutorial environment..."

# Ensure the shared Docker network exists (for connecting to PostgreSQL container)
echo "🌐 Creating shared Docker network..."
docker network create drasi-network 2>/dev/null || true

# Install system dependencies
echo "🐘 Installing system dependencies (PostgreSQL client)..."
sudo apt-get update && sudo apt-get install -y postgresql-client

# Make scripts executable
if [ -d "examples/getting-started/scripts" ]; then
    echo "📜 Making example scripts executable..."
    chmod +x examples/getting-started/scripts/*.sh
fi

# Download pre-built Drasi Server and SSE CLI binaries
echo "⬇️  Downloading Drasi Server and SSE CLI binaries..."
./examples/getting-started/scripts/download.sh

echo ""
echo "✅ Drasi Server Getting Started tutorial environment is ready!"
