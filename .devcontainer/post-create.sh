#!/bin/bash
# Post-create script for Drasi Server devcontainer

set -e

echo "🔧 Initializing Drasi Server development environment..."

# Install PostgreSQL client for database interactions
echo "🐘 Installing PostgreSQL client and OpenSSL development libraries..."
sudo apt-get update && sudo apt-get install -y postgresql-client libssl-dev pkg-config

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
echo ""
echo "Quick start:"
echo "  1. Start PostgreSQL:  cd examples/getting-started/scripts && ./setup-database.sh"
echo "  2. Start server:      ./target/release/drasi-server --config examples/getting-started/server-config.yaml"
echo "  3. Open viewer:       cd examples/getting-started/scripts && ./open-viewer.sh"
echo ""
echo "See examples/getting-started/README.md for the full tutorial."
