#!/bin/bash
# Post-create script for Drasi Server Getting Started tutorial

set -e

echo "🔧 Initializing Drasi Server Getting Started tutorial environment..."

# Install system dependencies
echo "🐘 Installing system dependencies (PostgreSQL client)..."
sudo apt-get update && sudo apt-get install -y postgresql-client

# Download pre-built Drasi Server and SSE CLI binaries.
# Invoke via `bash` so we don't depend on the executable bit, which can be
# lost on Windows bind-mounts (e.g. workspace under OneDrive) where chmod
# is not permitted on the mounted filesystem.
echo "⬇️  Downloading Drasi Server and SSE CLI binaries..."
bash examples/getting-started/scripts/download.sh

echo ""
echo "✅ Drasi Server Getting Started tutorial environment is ready!"
